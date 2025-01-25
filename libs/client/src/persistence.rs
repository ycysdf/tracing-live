use crate::TLReconnectAndPersistenceSetting;
use binrw::io::TakeSeekExt;
use binrw::{binrw, BinRead, BinResult, BinWrite};
use bytes::{BufMut, Bytes, BytesMut};
use futures_util::SinkExt;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, AsyncWriteExt};
use tokio::task::JoinHandle;
use tracing_lv_core::{TLAppInfo, TLMsg, TracingLiveMsgSubscriber};
use uuid::Uuid;
use zstd::zstd_safe::WriteBuf;

pub trait RecordsPersistence {}

pub struct RecordsPersistenceToFile {
    compressor: zstd::bulk::Compressor<'static>,
    compressor_with_dict: zstd::bulk::Compressor<'static>,
    decompressor_with_dict: zstd::bulk::Decompressor<'static>,
    decompressor: zstd::bulk::Decompressor<'static>,
    cursor_buf: Cursor<Vec<u8>>,
    buf: Vec<u8>,
    pub compression_level: i32,
}

impl RecordsPersistenceToFile {
    pub fn new(compression_level: i32) -> std::io::Result<Self> {
        Ok(RecordsPersistenceToFile {
            compressor: zstd::bulk::Compressor::new(compression_level)?,
            compressor_with_dict: zstd::bulk::Compressor::new(compression_level)?,
            decompressor_with_dict: zstd::bulk::Decompressor::new()?,
            decompressor: zstd::bulk::Decompressor::new()?,
            cursor_buf: Default::default(),
            buf: vec![],
            compression_level,
        })
    }
}

#[derive(Default, Clone, Copy, Debug)]
#[binrw]
#[brw(little)]
pub struct MemorySpan {
    pub pos: u64,
    pub size: u64,
}

impl MemorySpan {
    pub const NONE: MemorySpan = MemorySpan { pos: 0, size: 0 };
    pub const SIZE: usize = (u64::BITS as usize / 8) * 2;
    pub fn is_none(&self) -> bool {
        self.pos == 0 && self.size == 0
    }
    pub fn add_size(&mut self, size: u64) {
        self.size += size;
    }
}

#[derive(Debug, Clone)]
#[binrw]
#[brw(little)]
pub struct TLRecordsInstanceFooter {
    pub id: u128,
    pub prev: MemorySpan,              // 0 as None
    pub last_record_index: u64,        // 0 as None
    pub last_block_header: MemorySpan, // 0 as None . None if is current instance
    pub span: MemorySpan,
}

impl TLRecordsInstanceFooter {
    pub const SIZE: usize =
        (u128::BITS as usize / 8) + MemorySpan::SIZE * 3 + (u64::BITS as usize / 8) * 1;
}

#[derive(Default, Clone, Copy, Debug)]
#[binrw]
#[brw(little)]
pub struct TLRecordInfo {
    memory_span: MemorySpan,
}
impl TLRecordInfo {
    pub const SIZE: usize = MemorySpan::SIZE;
}

#[derive(Debug, Clone)]
#[binrw]
#[brw(little)]
pub struct TLBlockHeader {
    pub span: MemorySpan,
    pub prev: MemorySpan,
    pub records_info: [TLRecordInfo; RECORD_BLOCK_SIZE],
}
impl TLBlockHeader {
    pub const SIZE: usize = MemorySpan::SIZE * 2 + TLRecordInfo::SIZE * RECORD_BLOCK_SIZE;
}

impl Default for TLBlockHeader {
    fn default() -> Self {
        Self {
            span: MemorySpan::NONE,
            prev: MemorySpan::NONE,
            records_info: [TLRecordInfo::default(); RECORD_BLOCK_SIZE],
        }
    }
}

pub const RECORD_BLOCK_SIZE: usize = 1024;
pub const FORMAT_MAGIC: &'static [u8] = b"TLRM";
#[derive(Debug)]
#[binrw]
#[brw(little, magic = b"TLRM")]
pub struct TLRecordsMetadata {
    pub app_run_count: u32,
    pub share_compression_dictionary: MemorySpan,
    pub cur_pos: u64,
    pub current_instance_footer: TLRecordsInstanceFooter,
    pub current_block_header: TLBlockHeader,
}

impl TLRecordsMetadata {
    pub const SIZE: usize = FORMAT_MAGIC.len()
        + (u32::BITS as usize / 8)
        + MemorySpan::SIZE
        + (u64::BITS as usize / 8)
        + TLRecordsInstanceFooter::SIZE
        + TLBlockHeader::SIZE;

    pub fn add_size(&mut self, size: u64) -> MemorySpan {
        let pos = self.cur_pos;
        self.current_instance_footer.span.add_size(size);
        self.cur_pos += size;
        MemorySpan { pos, size }
    }
    pub fn append_instance_header(&mut self, instance_size: usize) -> MemorySpan {
        self.app_run_count += 1;
        self.current_block_header = Default::default();
        self.current_instance_footer.prev = self.add_size(instance_size as _);
        self.current_instance_footer.prev
    }
    pub fn append_block_header(&mut self, block_size: usize) -> MemorySpan {
        self.current_block_header = Default::default();
        self.current_block_header.prev = self.add_size(block_size as _);
        self.current_instance_footer.last_block_header = self.current_block_header.prev;
        self.current_block_header.span.pos = self.cur_pos;
        self.current_block_header.prev
    }
    pub fn append_record(&mut self, record_index: u64, record_size: usize) -> MemorySpan {
        let mut cur_block_index = record_index / RECORD_BLOCK_SIZE as u64;
        let block_record_index = record_index - cur_block_index * RECORD_BLOCK_SIZE as u64;
        let memory_span = self.add_size(record_size as _);
        self.current_block_header.records_info[block_record_index as usize].memory_span =
            memory_span;
        self.current_block_header.span.add_size(record_size as u64);
        self.current_instance_footer.last_record_index = record_index;
        memory_span
    }
}

// 递增压缩可以索引存储文件

pub trait RWS: Write + Read + Seek {}
impl<T> RWS for T where T: Write + Read + Seek {}

impl RecordsPersistenceToFile {
    pub fn reset(
        &mut self,
        mut stream: impl RWS,
        instance_id: u128,
    ) -> BinResult<TLRecordsMetadata> {
        stream.rewind()?;
        // println!("NEW INIT");
        let mut records_metadata = TLRecordsMetadata {
            app_run_count: 1,
            cur_pos: 0,
            current_instance_footer: TLRecordsInstanceFooter {
                id: instance_id,
                prev: MemorySpan::NONE,
                last_record_index: 0,
                last_block_header: MemorySpan::NONE,
                span: MemorySpan::NONE,
            },
            current_block_header: Default::default(),
            share_compression_dictionary: MemorySpan::NONE,
        };
        records_metadata.cur_pos = TLRecordsMetadata::SIZE as _;
        records_metadata.current_block_header.span.pos = records_metadata.cur_pos;
        records_metadata.current_instance_footer.span.pos = records_metadata.cur_pos;

        stream.rewind()?;
        records_metadata.write(&mut stream)?;
        assert_eq!(
            stream.stream_position().unwrap(),
            TLRecordsMetadata::SIZE as u64
        );
        Ok(records_metadata)
    }

    pub fn init_exist(
        &mut self,
        instance_id: u128,
        mut stream: impl RWS,
    ) -> BinResult<TLRecordsMetadata> {
        stream.rewind()?;
        let mut records_metadata: TLRecordsMetadata = TLRecordsMetadata::read(&mut stream)?;
        if !records_metadata.share_compression_dictionary.is_none() {
            let mut decompressor_with_dict = core::mem::take(&mut self.decompressor_with_dict);
            let mut compressor_with_dict = core::mem::take(&mut self.compressor_with_dict);
            let compression_level = self.compression_level;
            let r = self.read_area(
                &mut stream,
                records_metadata.share_compression_dictionary,
                |bytes| {
                    decompressor_with_dict.set_dictionary(bytes.as_slice())?;
                    compressor_with_dict.set_dictionary(compression_level, bytes.as_slice())?;
                    Ok::<(), std::io::Error>(())
                },
            );
            self.decompressor_with_dict = decompressor_with_dict;
            r??;
        }
        if records_metadata.current_instance_footer.id != instance_id {
            stream.seek(SeekFrom::Start(records_metadata.cur_pos))?;
            records_metadata.current_instance_footer.last_block_header =
                self.write_block_footer(&mut stream, &mut records_metadata)?;
            self.write_instance_footer(&mut stream, &mut records_metadata, instance_id)?;

            stream.rewind()?;
            records_metadata.write(&mut stream)?;
            assert_eq!(
                stream.stream_position().unwrap(),
                TLRecordsMetadata::SIZE as u64
            );
        }
        Ok(records_metadata)
    }

    pub fn init(
        &mut self,
        instance_id: u128,
        mut stream: impl RWS,
    ) -> BinResult<TLRecordsMetadata> {
        let mut buf = [0u8; 4];
        stream.rewind()?;
        if (stream.read_exact(&mut buf).is_err() || buf != FORMAT_MAGIC) {
            self.reset(&mut stream, instance_id)
        } else {
            self.init_exist(instance_id, stream)
        }
    }

    pub fn debug(
        &mut self,
        mut stream: impl RWS,
        records_metadata: &TLRecordsMetadata,
    ) -> BinResult<()> {
        if records_metadata.current_instance_footer.span.is_none() {
            return Ok(());
        }
        let instant = Instant::now();
        let instances = self.read_instances(&mut stream)?;
        let mut c = 0;
        for instance in instances.iter().filter(|n| n.span.size != 0) {
            self.iter_from(&mut stream, records_metadata.clone(), &instance, 0, |msg| {
                c += 1;
            })?;
        }

        // println!("debug elapsed: {:?}. count: {c}", instant.elapsed());

        Ok(())
    }

    pub fn blocks_from(
        &mut self,
        mut stream: impl RWS,
        records_metadata: &TLRecordsMetadata,
        instance_footer: &TLRecordsInstanceFooter,
        from_block_index: u64,
        to: &mut Vec<TLBlockHeader>,
    ) -> BinResult<()> {
        stream.rewind()?;
        let mut cur_block_index = instance_footer.last_record_index / RECORD_BLOCK_SIZE as u64;
        let mut header = if instance_footer.id == records_metadata.current_instance_footer.id {
            to.push(records_metadata.current_block_header.clone());
            match cur_block_index.cmp(&from_block_index) {
                Ordering::Less => return Ok(()),
                Ordering::Equal => {
                    return Ok(());
                }
                Ordering::Greater => {}
            }
            records_metadata.current_block_header.prev
        } else {
            instance_footer.last_block_header
        };
        while !header.is_none() {
            let block_header = self.read_block_footer(&mut stream, header)?;
            header = block_header.prev;
            to.push(block_header);
        }
        Ok(())
    }
    pub fn read_instances(&mut self, stream: impl RWS) -> BinResult<Vec<TLRecordsInstanceFooter>> {
        let mut result = vec![];
        for x in self.iter_instances(stream)? {
            result.push(x?);
        }
        Ok(result)
    }
    pub fn read_metadata(&self, mut stream: impl RWS) -> BinResult<TLRecordsMetadata> {
        let pos = stream.stream_position()?;
        stream.rewind()?;
        let records_metadata: TLRecordsMetadata = TLRecordsMetadata::read(&mut stream)?;
        stream.seek(SeekFrom::Start(pos))?;
        Ok(records_metadata)
    }
    pub fn iter_instances<'a>(
        &'a mut self,
        mut stream: impl RWS + 'a,
    ) -> BinResult<impl Iterator<Item = BinResult<TLRecordsInstanceFooter>> + 'a> {
        stream.rewind()?;
        let mut records_metadata: TLRecordsMetadata = TLRecordsMetadata::read(&mut stream)?;
        let mut prev = records_metadata.current_instance_footer.prev;
        Ok(
            core::iter::once(Ok(records_metadata.current_instance_footer)).chain(
                core::iter::from_fn(move || {
                    if prev.is_none() {
                        return None;
                    }
                    let instance = match self.read_instance_footer(&mut stream, prev) {
                        Ok(n) => n,
                        Err(err) => return Some(Err(err)),
                    };
                    prev = instance.prev;
                    Some(Ok(instance))
                }),
            ),
        )
    }

    pub fn blocks(
        &mut self,
        stream: impl RWS,
        records_metadata: &TLRecordsMetadata,
        instance_footer: &TLRecordsInstanceFooter,
    ) -> BinResult<Vec<TLBlockHeader>> {
        let mut result = vec![];
        self.blocks_from(stream, records_metadata, instance_footer, 0, &mut result)?;
        Ok(result)
    }

    pub fn iter_from(
        &mut self,
        mut stream: impl RWS,
        records_metadata: &TLRecordsMetadata,
        instance_footer: &TLRecordsInstanceFooter,
        from_record_index: u64,
        mut records_callback: impl FnMut(TLMsg),
    ) -> BinResult<()> {
        let block_index = from_record_index / RECORD_BLOCK_SIZE as u64;
        let mut blocks = vec![];
        self.blocks_from(
            &mut stream,
            records_metadata,
            instance_footer,
            block_index,
            &mut blocks,
        )?;
        if blocks.is_empty() {
            return Ok(());
        }
        // println!("blocks[0]: {:?}",blocks[0]);
        blocks.reverse();
        for (i, record_info) in blocks[0].records_info
            [(from_record_index as usize - block_index as usize * RECORD_BLOCK_SIZE)..]
            .iter()
            .enumerate()
            .filter(|n| !n.1.memory_span.is_none())
        {
            let _record_index = from_record_index + i as u64;
            // println!("record_info {_record_index} index: {record_info:?}");
            let record = self.read_record(&mut stream, record_info.memory_span)?;
            assert_eq!(_record_index, record.record_index());
            records_callback(record);
        }

        for (current_block_index, block_header) in blocks
            .iter()
            .skip(1)
            .enumerate()
            .map(|n| (n.0 as u64 + block_index + 1, n.1))
        {
            for (i, record_info) in block_header
                .records_info
                .iter()
                .enumerate()
                .filter(|n| !n.1.memory_span.is_none())
            {
                let _record_index = current_block_index * RECORD_BLOCK_SIZE as u64 + i as u64;
                let record = self.read_record(&mut stream, record_info.memory_span)?;
                assert_eq!(_record_index, record.record_index());
                records_callback(record);
            }
        }
        Ok(())
    }

    pub fn read_area<U>(
        &mut self,
        mut stream: impl Read + Seek,
        memory_span: MemorySpan,
        f: impl FnOnce(&mut Vec<u8>) -> U,
    ) -> std::io::Result<U> {
        stream.seek(SeekFrom::Start(memory_span.pos)).unwrap();
        self.cursor_buf.get_mut().reserve(memory_span.size as _);
        self.cursor_buf.get_mut().resize(memory_span.size as _, 0);
        stream.read_exact(&mut self.cursor_buf.get_mut()[..(memory_span.size as usize)])?;
        Ok(f(self.cursor_buf.get_mut()))
    }

    // pub fn decode<U>(&mut self, bytes: &[u8], f: impl FnOnce(&[u8]) -> U) -> std::io::Result<U> {
    //     self.buf.clear();
    //     self.buf.reserve(TLBlockHeader::SIZE);
    //     let len = self
    //         .decompressor_with_dict
    //         .decompress_to_buffer(bytes, &mut self.buf)?;
    //     Ok(f(&self.buf[..len]))
    // }

    pub fn read_and_decode<U>(
        &mut self,
        mut stream: impl RWS,
        memory_span: MemorySpan,
        f: impl FnOnce(&[u8]) -> U,
    ) -> std::io::Result<U> {
        stream.seek(SeekFrom::Start(memory_span.pos))?;
        self.cursor_buf.get_mut().resize(memory_span.size as _, 0);
        stream
            .read_exact(&mut self.cursor_buf.get_mut()[..(memory_span.size as usize)])
            .unwrap();
        self.buf.clear();
        self.buf.reserve(TLBlockHeader::SIZE);
        let len = self.decompressor_with_dict.decompress_to_buffer(
            &self.cursor_buf.get_ref()[..(memory_span.size as usize)],
            &mut self.buf,
        )?;
        Ok(f(&self.buf[..len]))
    }

    pub fn read_record(
        &mut self,
        stream: impl RWS,
        memory_span: MemorySpan,
    ) -> std::io::Result<TLMsg> {
        self.read_and_decode(stream, memory_span, |bytes| {
            rmp_serde::from_slice(bytes).map_err(|n| std::io::Error::other(n))
        })?
    }

    pub fn read_block_footer(
        &mut self,
        stream: impl RWS,
        memory_span: MemorySpan,
    ) -> BinResult<TLBlockHeader> {
        self.read_and_decode(stream, memory_span, |bytes| {
            TLBlockHeader::read(&mut Cursor::new(bytes))
        })?
    }
    pub fn read_instance_footer(
        &mut self,
        stream: impl RWS,
        memory_span: MemorySpan,
    ) -> BinResult<TLRecordsInstanceFooter> {
        self.read_and_decode(stream, memory_span, |bytes| {
            TLRecordsInstanceFooter::read(&mut Cursor::new(bytes))
        })?
    }

    pub fn write_frames<'a>(
        &mut self,
        mut stream: &mut dyn RWS,
        frames: impl Iterator<Item = (&'a [u8], u64)>,
    ) -> BinResult<()> {
        stream.seek(SeekFrom::Start(0))?;
        let mut records_metadata = TLRecordsMetadata::read(&mut stream).unwrap();
        let mut last_record_index = records_metadata.current_instance_footer.last_record_index;
        let mut cur_block_index = last_record_index / RECORD_BLOCK_SIZE as u64;
        stream.seek(SeekFrom::Start(records_metadata.cur_pos))?;

        {
            for (frame, record_index) in frames {
                // println!("record_index {}", record_index);
                let block_index = record_index / RECORD_BLOCK_SIZE as u64;
                if block_index != cur_block_index {
                    // println!("new block {}", block_index);
                    assert_eq!(block_index - 1, cur_block_index);
                    cur_block_index = block_index;
                    if records_metadata.share_compression_dictionary.is_none() {
                        // println!("frame_samples");
                        let mut frame_samples = Vec::with_capacity(RECORD_BLOCK_SIZE);
                        for (record_index, record_info) in records_metadata
                            .current_block_header
                            .records_info
                            .iter()
                            .enumerate()
                            .filter(|n| !n.1.memory_span.is_none())
                        {
                            // println!("record_info: {record_info:?}");
                            self.read_and_decode(&mut stream, record_info.memory_span, |bytes| {
                                frame_samples.push((bytes.to_vec(), record_index))
                            })?;
                        }
                        let dict = zstd::dict::from_sample_iterator(
                            frame_samples.iter().map(|n| Ok(n.0.as_slice())),
                            1024 * 8,
                        )?;

                        self.decompressor_with_dict
                            .set_dictionary(dict.as_slice())?;
                        self.compressor_with_dict
                            .set_dictionary(self.compression_level, dict.as_slice())?;

                        records_metadata =
                            self.reset(&mut stream, records_metadata.current_instance_footer.id)?;
                        last_record_index =
                            records_metadata.current_instance_footer.last_record_index;

                        stream.seek(SeekFrom::Start(records_metadata.cur_pos))?;
                        stream.write_all(dict.as_slice())?;
                        records_metadata.share_compression_dictionary =
                            records_metadata.add_size(dict.len() as _);
                        records_metadata.current_instance_footer.span = MemorySpan {
                            pos: records_metadata.cur_pos,
                            size: 0,
                        };
                        records_metadata.current_block_header.span = MemorySpan {
                            pos: records_metadata.cur_pos,
                            size: 0,
                        };

                        self.update_header(&mut stream, &records_metadata)?;

                        for (frame, record_index) in frame_samples {
                            self.write_frame(
                                &mut stream,
                                &mut records_metadata,
                                frame.as_slice(),
                                record_index as _,
                            )?;
                        }
                    }
                    self.write_block_footer(&mut stream, &mut records_metadata)?;
                }

                self.write_frame(&mut stream, &mut records_metadata, frame, record_index)?;
            }
        }

        self.update_header(&mut stream, &records_metadata)?;
        Ok(())
    }

    fn update_header(
        &mut self,
        mut stream: impl RWS,
        records_metadata: &TLRecordsMetadata,
    ) -> BinResult<()> {
        let prev_pos = stream.stream_position()?;
        stream.seek(SeekFrom::Start(0))?;
        records_metadata.write(&mut stream).unwrap();
        stream.flush()?;
        stream.seek(SeekFrom::Start(prev_pos))?;
        Ok(())
    }

    fn write_instance_footer(
        &mut self,
        mut stream: impl RWS,
        records_metadata: &mut TLRecordsMetadata,
        instance_id: u128,
    ) -> BinResult<MemorySpan> {
        self.cursor_buf.get_mut().clear();
        self.cursor_buf.seek(SeekFrom::Start(0))?;
        records_metadata
            .current_instance_footer
            .write(&mut self.cursor_buf)?;

        self.buf.clear();
        self.buf.reserve(self.cursor_buf.get_ref().len() * 2);
        let len = self
            .compressor
            .compress_to_buffer(self.cursor_buf.get_ref().as_slice(), &mut self.buf)?;

        let start_pos = stream.stream_position()?;
        assert_eq!(records_metadata.cur_pos, start_pos);

        stream.write_all(&self.buf[..len])?;
        let memory_span = records_metadata.append_instance_header(len);

        let end_pos = stream.stream_position()?;
        assert_eq!(records_metadata.cur_pos, end_pos);

        records_metadata.current_instance_footer.id = instance_id;
        records_metadata.current_instance_footer.span = MemorySpan {
            pos: end_pos,
            size: 0,
        };
        records_metadata.current_instance_footer.last_record_index = 0;
        records_metadata.current_instance_footer.last_block_header = MemorySpan::NONE;
        Ok(memory_span)
    }

    fn write_block_footer(
        &mut self,
        mut stream: impl RWS,
        records_metadata: &mut TLRecordsMetadata,
    ) -> BinResult<MemorySpan> {
        self.cursor_buf.get_mut().clear();
        self.cursor_buf.seek(SeekFrom::Start(0))?;
        records_metadata
            .current_block_header
            .write(&mut self.cursor_buf)?;

        self.buf.clear();
        self.buf.reserve(self.cursor_buf.get_ref().len() * 2);
        let len = self
            .compressor
            .compress_to_buffer(self.cursor_buf.get_ref().as_slice(), &mut self.buf)?;

        let start_pos = stream.stream_position()?;
        assert_eq!(records_metadata.cur_pos, start_pos);

        stream.write_all(&self.buf[..len])?;

        let memory_span = records_metadata.append_block_header(len);

        let end_pos = stream.stream_position()?;
        assert_eq!(records_metadata.cur_pos, end_pos);
        Ok(memory_span)
    }

    fn write_frame(
        &mut self,
        mut stream: impl RWS,
        records_metadata: &mut TLRecordsMetadata,
        frame: &[u8],
        record_index: u64,
    ) -> BinResult<()> {
        self.buf.reserve(frame.len() * 2);
        self.buf.clear();
        let len = self
            .compressor_with_dict
            .compress_to_buffer(frame, &mut self.buf)?;
        let stream_pos = stream.stream_position()?;
        assert_eq!(records_metadata.cur_pos, stream_pos);

        stream.write_all(&self.buf[..len])?;

        records_metadata.append_record(record_index, len);

        let stream_pos = stream.stream_position()?;
        assert_eq!(records_metadata.cur_pos, stream_pos);
        Ok(())
    }
}
thread_local! {
    static BUF1: RefCell<Cursor<Vec<u8>>> = RefCell::new(Cursor::new(Vec::new()));
    static BUF2: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

thread_local! {
    static BUF: RefCell<BytesMut> = RefCell::new(BytesMut::new());
}

pub struct EncodeBytesSubscriber {
    // pub total_size: AtomicU64,
    pub sender: flume::Sender<(Bytes, u64)>,
}

/*impl EncodeBytesSubscriber {
    pub fn new() -> Self {
        // let records_io = Arc::new(tokio::sync::Mutex::new(
        //     async_compression::tokio::write::ZstdEncoder::new(setting.records_writer),
        // ));
        /*
        tokio::spawn({
            let records_io = setting.records_writer.clone();
            async move {
                let file_msg_count = 64;
                // let mut len = 0;
                // let mut app_run_data_file = setting.metadata_info_writer;
                // let app_run_start_pos = setting.metadata_info_cur_len;
                // let mut buf = BytesMut::new();
                // let mut app_run_data = AppRunData {
                //     app_run_id,
                //     start_pos: app_run_start_pos,
                //     end_pos: app_run_start_pos,
                //     last_record_index: 0,
                // };
                let mut dir: PathBuf = format!("{app_run_id}").into();
                let max_buf_recv = 64;
                let mut buf_recv = Vec::with_capacity(max_buf_recv);
                let mut first = true;
                while let Ok(n) = receiver.recv_async().await {
                    buf_recv.clear();
                    buf_recv.push(n);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    while let Ok(n) = receiver.try_recv() {
                        buf_recv.push(n);
                        if buf_recv.len() == max_buf_recv {
                            break;
                        }
                    }
                    {
                        (dir, buf_recv) = tokio::task::spawn_blocking({
                            let records_io = records_io.clone();
                            move || {
                                let mut records_io = records_io.lock();
                                let mut records_io = records_io.unwrap();
                                records_io.seek(SeekFrom::Start(0))?;
                                let mut records_io = if first {
                                    zip::ZipWriter::new(&mut *records_io)
                                } else {
                                    zip::ZipWriter::new_append(&mut *records_io)?
                                };
                                records_io.set_flush_on_finish_file(true);
                                let start_record_id = buf_recv[0].1;
                                let start_file = start_record_id / file_msg_count;
                                // let path = dir.join(format!("R-{start_file}.txt"));
                                // println!("buf_recv: {:#?} path: {path:?}", buf_recv.len());

                                for (msg_bytes, record_index) in buf_recv.iter() {
                                    records_io.start_file_from_path(
                                        dir.join(format!("R-{record_index}.txt")),
                                        SimpleFileOptions::default()
                                            .compression_method(CompressionMethod::Zstd),
                                    )?;
                                    records_io.write_all(msg_bytes.as_ref())?;
                                }
                                // let file_end_index = (start_file + 1) * file_msg_count - 1;
                                // if buf_recv.last().unwrap().1 > file_end_index {
                                //     let next_file_count =
                                //         buf_recv.last().unwrap().1 - file_end_index;
                                //     for (msg_bytes, record_index) in
                                //         &buf_recv[..(buf_recv.len() - next_file_count as usize)]
                                //     {
                                //         records_io.write_all(msg_bytes.as_ref())?;
                                //         if *record_index != file_end_index {
                                //             records_io.write_all(b"\r\n,\r\n")?;
                                //         }
                                //     }
                                //     records_io.flush()?;
                                //     records_io.start_file_from_path(
                                //         dir.join(format!("R-{}.txt", start_file + 1)),
                                //         SimpleFileOptions::default()
                                //             .compression_method(CompressionMethod::Zstd),
                                //     )?;
                                //     let next_file_end_index = file_end_index + file_msg_count;
                                //     for (msg_bytes, record_index) in
                                //         &buf_recv[(buf_recv.len() - next_file_count as usize)..]
                                //     {
                                //         records_io.write_all(msg_bytes.as_ref())?;
                                //         if *record_index != next_file_end_index {
                                //             records_io.write_all(b"\r\n,\r\n")?;
                                //         }
                                //     }
                                // } else {
                                //     for (msg_bytes, record_index) in buf_recv.iter() {
                                //         records_io.write_all(msg_bytes.as_ref())?;
                                //         if *record_index != file_end_index {
                                //             records_io.write_all(b"\r\n,\r\n")?;
                                //         }
                                //     }
                                // }
                                // records_io.flush()?;
                                records_io.finish()?;

                                Ok::<_, zip::result::ZipError>((dir, buf_recv))
                            }
                        })
                        .await
                        .unwrap()
                        .unwrap();
                        first = false;
                    }
                }
                // app_run_data.last_record_index = record_index;
                // app_run_data.end_pos = len;
                // let compressed_len = {
                //     let mut records_io = records_io.lock().await;
                //     records_io
                //         .get_mut()
                //         .write_u64(app_run_data.last_record_index)
                //         .await
                //         .unwrap();
                //     records_io
                //         .get_mut()
                //         .write_u64(bytes.chunk().len() as u64)
                //         .await
                //         .unwrap();
                //     records_io.write_all(bytes.chunk()).await.unwrap();
                // };
                // // <num:8><json len:8><record json>
                // len += (u64::BITS / 8) as u64
                //     + (u64::BITS / 8) as u64
                //     + bytes.chunk().len() as u64;
                // serde_json::to_writer_pretty((&mut buf).writer(), &app_run_data).unwrap();
                // app_run_data_file
                //     .seek(SeekFrom::Start(app_run_data.start_pos))
                //     .await
                //     .unwrap();
                // app_run_data_file.write_all(buf.chunk()).await.unwrap();
                // app_run_data_file.flush().await.unwrap();
                // buf.clear();
                // if msg_pos_sender.send(MsgPosInfo { pos: len }).is_err() {
                //     break;
                // }
            }
        });*/
        // tokio::spawn(async move {
        //     let mut buf = Vec::with_capacity(1024 * 8);
        //     while let Ok(msg_pos) = msg_pos_receiver.recv_async().await {
        //         let mut records_io = records_io.lock().await;
        //         let records_io = records_io.get_mut();
        //         records_io.seek(SeekFrom::Start(msg_pos.pos)).await.unwrap();
        //         let _ = records_io.read_u64().await;
        //         let json_len = records_io.read_u64().await.unwrap();
        //         buf.reserve(json_len as _);
        //         records_io.read(&mut buf).await.unwrap();
        //     }
        // });
        Self {
            // total_size: Default::default(),
            sender,
        }
    }
}
*/
impl TracingLiveMsgSubscriber for EncodeBytesSubscriber {
    fn on_msg(&self, msg: TLMsg) {
        // println!("msg: {msg:#?}");

        BUF.with_borrow_mut(|buf| {
            rmp_serde::encode::write(&mut buf.writer(), &msg).unwrap();
            // serde_json::to_writer_pretty(buf.writer(), &msg).unwrap();
            // let total_size = self
            //     .total_size
            //     .fetch_add(buf.len() as _, std::sync::atomic::Ordering::SeqCst);
            // println!("total_size: {total_size:?}");
            if let Err(err) = self
                .sender
                .send((buf.split().freeze(), msg.record_index()))
            {
                eprintln!("send error: {err}. msg: {msg:?}");
            }
        })
    }
}
