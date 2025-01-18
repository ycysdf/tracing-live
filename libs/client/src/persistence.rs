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
use tracing_lv_core::{TLMsg, TracingLiveMsgSubscriber};
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

#[derive(Debug)]
#[binrw]
pub struct TLRecordsInstanceFooter {
    pub prev_instance_footer_pos: u64, // 0 as None
    pub last_record_index: u64,
    // pub header_pos: u64, // 0 as None
    pub last_block_header_pos: u64, // 0 as None
    pub start_pos: u64,
    pub size: u64,
}

#[derive(Default, Clone, Copy, Debug)]
#[binrw]
#[brw(little)]
pub struct TLRecordInfo {
    pos: u64,
    size: u32,
}

#[derive(Debug)]
#[binrw]
#[brw(little)]
pub struct TLBlockHeader {
    pub size: u64,
    pub prev_pos: u64,
    pub prev_size: u64,
    pub records_info: [TLRecordInfo; RECORD_BLOCK_SIZE],
}
impl TLBlockHeader {
    pub const SIZE: usize = u64::BITS as usize / 8 * (3 + RECORD_BLOCK_SIZE)
        + u32::BITS as usize / 8 * RECORD_BLOCK_SIZE;
}

impl Default for TLBlockHeader {
    fn default() -> Self {
        Self {
            size: 0,
            prev_pos: 0,
            prev_size: 0,
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
    pub share_compression_dictionary_pos: u64,
    pub share_compression_dictionary_size: u32,
    pub cur_pos: u64,
    pub current_instance_header: TLRecordsInstanceFooter,
    pub current_block_header: TLBlockHeader,
}

impl TLRecordsMetadata {
    pub const SIZE: usize = 4
        + (u32::BITS as usize / 8) * 2
        + (u64::BITS as usize / 8) * 2
        + (u64::BITS as usize / 8) * 5
        + TLBlockHeader::SIZE;

    pub fn append_block(&mut self, block_size: usize) {
        self.current_block_header = Default::default();
        self.current_block_header.prev_pos = self.cur_pos;
        self.current_block_header.prev_size = block_size as _;
        self.current_instance_header.size += block_size as u64;
        self.cur_pos += block_size as u64;
    }
    pub fn append_record(&mut self, record_index: u64, record_size: usize) {
        let mut cur_block_index = record_index / RECORD_BLOCK_SIZE as u64;
        let block_record_index = record_index - cur_block_index * RECORD_BLOCK_SIZE as u64;
        self.current_block_header.records_info[block_record_index as usize].pos = self.cur_pos;
        self.current_block_header.records_info[block_record_index as usize].size = record_size as _;
        self.current_block_header.size += record_size as u64;
        self.current_instance_header.size += record_size as u64;
        self.cur_pos += record_size as u64;
        self.current_instance_header.last_record_index = record_index;
    }
}

// 递增压缩可以索引存储文件

pub trait RWS: Write + Read + Seek {}
impl<T> RWS for T where T: Write + Read + Seek {}

impl RecordsPersistenceToFile {
    pub fn init(&mut self, mut stream: impl RWS, reset: bool) -> BinResult<TLRecordsMetadata> {
        let mut buf = [0u8; 4];
        stream.rewind()?;
        if (stream.read_exact(&mut buf).is_err() || buf != FORMAT_MAGIC) || reset {
            stream.rewind()?;
            println!("NEW INIT");
            let mut records_metadata = TLRecordsMetadata {
                app_run_count: 1,
                share_compression_dictionary_pos: 0,
                share_compression_dictionary_size: 0,
                cur_pos: 0,
                current_instance_header: TLRecordsInstanceFooter {
                    prev_instance_footer_pos: 0,
                    last_record_index: 0,
                    last_block_header_pos: 0,
                    start_pos: 0,
                    size: 0,
                },
                current_block_header: Default::default(),
            };
            // records_metadata.current_instance_header.start_pos = TLRecordsMetadata::SIZE as _;
            records_metadata.cur_pos = TLRecordsMetadata::SIZE as _;
            println!("records_metadata: {records_metadata:#?}");
            records_metadata.write(&mut stream)?;
            assert_eq!(stream.stream_position().unwrap(), records_metadata.cur_pos);
            Ok(records_metadata)
        } else {
            stream.rewind()?;
            let mut records_metadata: TLRecordsMetadata = TLRecordsMetadata::read(&mut stream)?;
            println!("records_metadata: {records_metadata:#?}");
            if records_metadata.share_compression_dictionary_size != 0 {
                let mut decompressor_with_dict = core::mem::take(&mut self.decompressor_with_dict);
                let r = self.read_area(
                    &mut stream,
                    records_metadata.share_compression_dictionary_pos,
                    records_metadata.share_compression_dictionary_size as _,
                    |bytes| decompressor_with_dict.set_dictionary(bytes.as_slice()),
                );
                self.decompressor_with_dict = decompressor_with_dict;
                r??;
            }
            Ok(records_metadata)
        }
    }

    pub fn debug(&mut self, mut stream: impl RWS) -> BinResult<()> {
        let records_metadata = self.init(&mut stream, false)?;
        if records_metadata.current_instance_header.size == 0 {
            return Ok(());
        }
        let instant = Instant::now();
        let mut c = 0;
        self.iter_from(stream, 0, |msg| {
            c += 1;
        })?;

        println!("debug elapsed: {:?}. count: {c}", instant.elapsed());

        Ok(())
    }

    pub fn blocks_from(
        &mut self,
        mut stream: impl RWS,
        from_block_index: u64,
        to: &mut Vec<TLBlockHeader>,
    ) -> BinResult<()> {
        stream.rewind()?;
        let mut records_metadata: TLRecordsMetadata = TLRecordsMetadata::read(&mut stream)?;
        let mut cur_block_index =
            records_metadata.current_instance_header.last_record_index / RECORD_BLOCK_SIZE as u64;
        match cur_block_index.cmp(&from_block_index) {
            Ordering::Less => return Ok(()),
            Ordering::Equal => {
                to.push(records_metadata.current_block_header);
                return Ok(());
            }
            Ordering::Greater => {}
        }
        let mut header_pos = records_metadata.current_block_header.prev_pos;
        let mut header_size = records_metadata.current_block_header.prev_size;
        to.push(records_metadata.current_block_header);
        for i in (0..cur_block_index).rev() {
            let block_header = self.read_block_header(&mut stream, header_pos, header_size)?;
            header_pos = block_header.prev_pos;
            header_size = block_header.prev_size;
            if i >= from_block_index {
                to.push(block_header);
            } else {
                break;
            }
        }
        Ok(())
    }
    pub fn blocks(&mut self, mut stream: impl RWS) -> BinResult<Vec<TLBlockHeader>> {
        let mut result = vec![];
        self.blocks_from(stream, 0, &mut result)?;
        Ok(result)
    }

    pub fn iter_from(
        &mut self,
        mut stream: impl RWS,
        from_record_index: u64,
        mut records_callback: impl FnMut(TLMsg),
    ) -> BinResult<()> {
        let block_index = from_record_index / RECORD_BLOCK_SIZE as u64;
        let mut blocks = vec![];
        self.blocks_from(&mut stream, block_index, &mut blocks)?;
        if blocks.is_empty() {
            return Ok(());
        }
        blocks.reverse();

        for (i, record_info) in blocks[0].records_info[(from_record_index as usize)..]
            .iter()
            .enumerate()
            .filter(|n| n.1.size > 0)
        {
            let _record_index = block_index * RECORD_BLOCK_SIZE as u64 + i as u64;
            let record = self.read_record(&mut stream, record_info.pos, record_info.size)?;
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
                .filter(|n| n.1.size > 0)
            {
                let _record_index = current_block_index * RECORD_BLOCK_SIZE as u64 + i as u64;
                let record = self.read_record(&mut stream, record_info.pos, record_info.size)?;
                assert_eq!(_record_index, record.record_index());
                records_callback(record);
            }
        }
        Ok(())
    }

    pub fn read_area<U>(
        &mut self,
        mut stream: impl Read + Seek,
        pos: u64,
        size: u64,
        f: impl FnOnce(&mut Vec<u8>) -> U,
    ) -> std::io::Result<U> {
        stream.seek(SeekFrom::Start(pos)).unwrap();
        self.cursor_buf.get_mut().reserve(size as _);
        self.cursor_buf.get_mut().resize(size as _, 0);
        stream.read_exact(&mut self.cursor_buf.get_mut()[..(size as usize)])?;
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
        pos: u64,
        size: u64,
        f: impl FnOnce(&[u8]) -> U,
    ) -> std::io::Result<U> {
        stream.seek(SeekFrom::Start(pos)).unwrap();
        self.cursor_buf.get_mut().resize(size as _, 0);
        stream
            .read_exact(&mut self.cursor_buf.get_mut()[..(size as usize)])
            .unwrap();
        self.buf.clear();
        self.buf.reserve(TLBlockHeader::SIZE);
        let len = self
            .decompressor_with_dict
            .decompress_to_buffer(&self.cursor_buf.get_ref()[..(size as usize)], &mut self.buf)?;
        Ok(f(&self.buf[..len]))
    }

    pub fn read_record(&mut self, stream: impl RWS, pos: u64, size: u32) -> std::io::Result<TLMsg> {
        self.read_and_decode(stream, pos, size as _, |bytes| {
            rmp_serde::from_slice(bytes).map_err(|n| std::io::Error::other(n))
        })?
    }

    pub fn read_block_header(
        &mut self,
        stream: impl RWS,
        pos: u64,
        size: u64,
    ) -> BinResult<TLBlockHeader> {
        self.read_and_decode(stream, pos, size, |bytes| {
            TLBlockHeader::read(&mut Cursor::new(bytes))
        })?
    }

    pub fn write_frames<'a>(
        &mut self,
        mut stream: &mut dyn RWS,
        frames: impl Iterator<Item = (&'a [u8], u64)>,
    ) -> BinResult<()> {
        stream.seek(SeekFrom::Start(0))?;
        let mut records_metadata = TLRecordsMetadata::read(&mut stream).unwrap();
        let mut last_record_index = records_metadata.current_instance_header.last_record_index;
        let mut cur_block_index = last_record_index / RECORD_BLOCK_SIZE as u64;
        stream.seek(SeekFrom::Start(records_metadata.cur_pos))?;

        {
            for (frame, record_index) in frames {
                let block_index = record_index / RECORD_BLOCK_SIZE as u64;
                println!("frame: record_index = {record_index}. block_index = {block_index}. cur_block_index = {cur_block_index}. ");
                if block_index != cur_block_index {
                    assert_eq!(block_index - 1, cur_block_index);
                    // 1.09 1.21  1.85
                    if records_metadata.share_compression_dictionary_size == 0 {
                        let mut frame_samples = Vec::with_capacity(RECORD_BLOCK_SIZE);
                        // let mut msgs = Vec::with_capacity(RECORD_BLOCK_SIZE);
                        for (record_index, record_info) in records_metadata
                            .current_block_header
                            .records_info
                            .iter()
                            .enumerate()
                            .filter(|n| n.1.size > 0)
                        {
                            // println!("RDS {record_index} {record_info:?}");
                            // let msg = self.read_record(
                            //     &mut stream,
                            //     record_info.pos,
                            //     record_info.size as _,
                            // )?;
                            self.read_and_decode(
                                &mut stream,
                                record_info.pos,
                                record_info.size as _,
                                |bytes| frame_samples.push((bytes.to_vec(), record_index)),
                            )?;
                            // msgs.push(msg);
                        }
                        let dict = zstd::dict::from_sample_iterator(
                            frame_samples.iter().map(|n| Ok(n.0.as_slice())),
                            1024 * 8,
                        )?;

                        self.decompressor_with_dict
                            .set_dictionary(dict.as_slice())?;
                        self.compressor_with_dict
                            .set_dictionary(self.compression_level, dict.as_slice())?;

                        records_metadata = self.init(&mut stream, true)?;
                        last_record_index =
                            records_metadata.current_instance_header.last_record_index;
                        cur_block_index = last_record_index / RECORD_BLOCK_SIZE as u64;

                        stream.seek(SeekFrom::Start(records_metadata.cur_pos))?;
                        stream.write_all(dict.as_slice())?;

                        records_metadata.share_compression_dictionary_pos =
                            records_metadata.cur_pos;
                        records_metadata.share_compression_dictionary_size = dict.len() as u32;
                        records_metadata.current_instance_header.size += dict.len() as u64;
                        records_metadata.cur_pos += dict.len() as u64;

                        stream.seek(SeekFrom::Start(records_metadata.cur_pos))?;

                        for (frame, record_index) in frame_samples {
                            self.buf.clear();
                            self.buf.reserve(frame.len() * 2);
                            let len = self
                                .compressor_with_dict
                                .compress_to_buffer(frame.as_slice(), &mut self.buf)?;
                            stream.write_all(&self.buf[..len])?;

                            records_metadata.append_record(record_index as _, len);
                        }
                    }

                    cur_block_index = block_index;

                    self.cursor_buf.get_mut().clear();
                    self.cursor_buf.seek(SeekFrom::Start(0))?;
                    records_metadata
                        .current_block_header
                        .write(&mut self.cursor_buf)
                        .unwrap();

                    self.buf.clear();
                    self.buf.reserve(self.cursor_buf.get_ref().len() * 2);
                    let len = self
                        .compressor
                        .compress_to_buffer(self.cursor_buf.get_ref().as_slice(), &mut self.buf)?;

                    let stream_pos = stream.stream_position().unwrap();
                    assert_eq!(records_metadata.cur_pos, stream_pos);

                    stream.write_all(&self.buf[..len])?;

                    records_metadata.append_block(len);

                    let stream_pos = stream.stream_position().unwrap();
                    assert_eq!(records_metadata.cur_pos, stream_pos);
                }

                self.buf.reserve(frame.len() * 2);
                self.buf.clear();
                let len = self
                    .compressor_with_dict
                    .compress_to_buffer(frame, &mut self.buf)?;
                let stream_pos = stream.stream_position().unwrap();
                assert_eq!(records_metadata.cur_pos, stream_pos);

                stream.write_all(&self.buf[..len])?;

                records_metadata.append_record(record_index, len);

                let stream_pos = stream.stream_position().unwrap();
                assert_eq!(records_metadata.cur_pos, stream_pos);
            }
        }

        stream.seek(SeekFrom::Start(0))?;
        records_metadata.write(&mut stream).unwrap();
        stream.flush()?;
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

pub struct PersistenceSubscriber {
    // pub total_size: AtomicU64,
    pub sender: flume::Sender<(Bytes, u64)>,
    pub setting: TLReconnectAndPersistenceSetting,
}

impl PersistenceSubscriber {
    pub fn new(
        app_run_id: Uuid,
        setting: TLReconnectAndPersistenceSetting,
    ) -> BinResult<(Self, JoinHandle<BinResult<()>>)> {
        use bytes::Buf;
        struct MsgPosInfo {
            pos: u64,
        }
        let (sender, receiver) = flume::unbounded::<(Bytes, u64)>();
        let (msg_pos_sender, msg_pos_receiver) = flume::unbounded::<MsgPosInfo>();

        let task_handle = tokio::spawn({
            let records_io = setting.records_writer.clone();
            async move {
                let max_buf_recv = RECORD_BLOCK_SIZE;
                let mut buf_recv = Vec::with_capacity(max_buf_recv);
                let mut file = RecordsPersistenceToFile {
                    compressor: zstd::bulk::Compressor::new(setting.compression_level)?,
                    compressor_with_dict: zstd::bulk::Compressor::new(setting.compression_level)?,
                    decompressor_with_dict: zstd::bulk::Decompressor::new()?,
                    decompressor: zstd::bulk::Decompressor::new()?,
                    cursor_buf: Default::default(),
                    buf: vec![],
                    compression_level: setting.compression_level,
                };
                {
                    let mut records_io = records_io.lock().unwrap();
                    file.debug(&mut *records_io)?;
                    return Ok(());
                }
                {
                    let mut records_io = records_io.lock().unwrap();
                    file.init(&mut *records_io, false)?;
                }
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
                    let mut records_io = records_io.lock().unwrap();
                    file.write_frames(
                        &mut *records_io,
                        buf_recv.iter().map(|n| (n.0.chunk(), n.1)),
                    )
                    .unwrap();
                }
                Ok(())
            }
        });

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
        Ok((
            Self {
                setting,
                // total_size: Default::default(),
                sender,
            },
            task_handle,
        ))
    }
}

impl TracingLiveMsgSubscriber for PersistenceSubscriber {
    fn on_msg(&self, msg: TLMsg) {
        // println!("msg: {msg:#?}");

        BUF.with_borrow_mut(|buf| {
            rmp_serde::encode::write(&mut buf.writer(), &msg).unwrap();
            // serde_json::to_writer_pretty(buf.writer(), &msg).unwrap();
            // let total_size = self
            //     .total_size
            //     .fetch_add(buf.len() as _, std::sync::atomic::Ordering::SeqCst);
            // println!("total_size: {total_size:?}");
            let _ = self.sender.send((buf.split().freeze(), msg.record_index()));
        })
    }
}
