use bytes::{BufMut, Bytes, BytesMut};
use chrono::Utc;
use hyper::Uri;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::future::Future;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};
use tokio::task::yield_now;
use tokio::time::Instant;
use tonic::codegen::{CompressionEncoding, Service, StdError};
use tonic::transport::{Channel, Endpoint, Error};
use tracing::instrument::{WithDispatch, WithSubscriber};
use tracing::subscriber::NoSubscriber;
use tracing::{error, warn};
use tracing_core::Dispatch;
use tracing_lv_core::proto::tracing_service_client::TracingServiceClient;
use tracing_lv_core::proto::{record_param, AppStop, PingParam, RecordParam};
use tracing_lv_core::{MsgReceiverSubscriber, TLAppInfo, TLLayer, TracingLiveMsgSubscriber};
use tracing_lv_core::{TLAppInfoExt, TLMsg};
use tracing_subscriber::layer::{Layered, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;
use crate::persistence::{PersistenceSubscriber, RecordsPersistenceToFile, RWS};

struct NoSubscriberService<T>(T);
struct NoSubscriberExecutor;

impl<F> hyper::rt::Executor<F> for NoSubscriberExecutor
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, fut: F) {
        tokio::spawn(fut.with_subscriber(NoSubscriber::new()));
    }
}

impl<T, P> Service<P> for NoSubscriberService<T>
where
    T: Service<P>,
{
    type Response = T::Response;
    type Error = T::Error;
    type Future = WithDispatch<T::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _guard = tracing::subscriber::set_default(NoSubscriber::new());
        self.0.poll_ready(cx)
    }

    fn call(&mut self, request: P) -> Self::Future {
        let _guard = tracing::subscriber::set_default(NoSubscriber::new());
        self.0.call(request).with_subscriber(NoSubscriber::new())
    }
}

pub struct TLGuard {
    msg_sender: flume::Sender<RecordParam>,
    is_normal_drop: bool,
}

impl TLGuard {
    pub fn normal_stop(&mut self) {
        self.is_normal_drop = true;

        let _ = self.msg_sender.send(RecordParam {
            send_time: Utc::now().timestamp_nanos_opt().unwrap(),
            record_index: 0,
            variant: Some(record_param::Variant::AppStop(AppStop {})),
        });
    }
}

impl Drop for TLGuard {
    fn drop(&mut self) {
        if !self.is_normal_drop {
            // TODO:
            let _ = self.msg_sender.send(RecordParam {
                send_time: Utc::now().timestamp_nanos_opt().unwrap(),
                record_index: 0,
                variant: Some(record_param::Variant::AppStop(AppStop {})),
            });
        }
    }
}

pub async fn default_connect(
    endpoint: impl TryInto<Endpoint, Error: Into<StdError>>,
) -> Result<Channel, tonic::transport::Error> {
    Endpoint::new(endpoint)?
        .executor(NoSubscriberExecutor)
        // .tcp_nodelay(false)
        .buffer_size(1024 * 1024 * 8)
        .keep_alive_while_idle(true)
        .keep_alive_timeout(Duration::from_secs(120))
        .connect()
        .await
}

pub trait TonicTryConnect {
    async fn try_connect(self) -> Result<Channel, tonic::transport::Error>;
}

impl TonicTryConnect for String {
    async fn try_connect(self) -> Result<Channel, tonic::transport::Error> {
        default_connect(self).await
    }
}
impl TonicTryConnect for &'static str {
    async fn try_connect(self) -> Result<Channel, tonic::transport::Error> {
        default_connect(self).await
    }
}
impl TonicTryConnect for Bytes {
    async fn try_connect(self) -> Result<Channel, tonic::transport::Error> {
        default_connect(self).await
    }
}
impl TonicTryConnect for Uri {
    async fn try_connect(self) -> Result<Channel, tonic::transport::Error> {
        let endpoint: Endpoint = self.into();
        default_connect(endpoint).await
    }
}
impl TonicTryConnect for Channel {
    async fn try_connect(self) -> Result<Channel, tonic::transport::Error> {
        Ok(self)
    }
}
impl<F, FO> TonicTryConnect for F
where
    FO: Future<Output = Result<Channel, tonic::transport::Error>>,
    F: FnOnce() -> FO,
{
    async fn try_connect(self) -> Result<Channel, tonic::transport::Error> {
        self().await
    }
}

impl<T> TonicTryConnect for Vec<T>
where
    T: TryInto<Endpoint>,
    T::Error: Into<StdError>,
{
    async fn try_connect(self) -> Result<Channel, tonic::transport::Error> {
        let mut prev_err = None;
        for endpoint in self.into_iter() {
            match default_connect(endpoint).await {
                Ok(n) => return Ok(n),
                Err(err) => {
                    prev_err = Some(err);
                    continue;
                }
            }
        }
        Err(prev_err.expect("no found invalid endpoint"))
    }
}

pub trait AsyncWriteWithSeek: AsyncWrite + AsyncSeek {}

impl<T> AsyncWriteWithSeek for T where T: AsyncWrite + AsyncSeek {}
pub trait AsyncWriteWithReadAndSeek: AsyncWriteWithSeek + AsyncRead {}

impl<T> AsyncWriteWithReadAndSeek for T where T: AsyncWriteWithSeek + AsyncRead {}

pub struct TLReconnectAndPersistenceSetting {
    pub records_writer: Arc<Mutex<dyn RWS + Send>>,
    pub reconnect_interval: Vec<Duration>,
    pub compression_level: i32
}

impl TLReconnectAndPersistenceSetting {
    pub fn from_file(records_file: std::fs::File) -> Result<Self, std::io::Error> {
        // let metadata_info_cur_len = metadata_info_file.metadata().await?.len();
        Ok(Self {
            // metadata_info_writer: Box::pin(metadata_info_file),
            // metadata_info_cur_len,
            records_writer: Arc::new(Mutex::new(records_file)),
            reconnect_interval: vec![
                Duration::from_secs(0),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(8),
                Duration::from_secs(8),
                Duration::from_secs(8),
                Duration::from_secs(8),
                Duration::from_secs(32),
                Duration::from_secs(64),
                Duration::from_secs(256),
                Duration::from_secs(1024),
            ],
            compression_level: 8,
        })
    }
}

#[derive(Default)]
pub struct TLSetting {
    pub reconnect_and_persistence: Option<TLReconnectAndPersistenceSetting>,
}

pub trait TLSubscriberExt: Sized {
    async fn with_tracing_lv<D>(
        self,
        dst: D,
        app_info: TLAppInfo,
        setting: TLSetting,
    ) -> Result<
        (
            Layered<TLLayer<PersistenceSubscriber>, Self>,
            impl Future<Output = ()> + Send + 'static,
            TLGuard,
        ),
        Error,
    >
    where
        D: TonicTryConnect;

    async fn tracing_lv_init<D, U, F: Future<Output = U> + 'static>(
        self,
        dst: D,
        app_info: TLAppInfo,
        setting: TLSetting,
        f: impl FnOnce() -> F,
    ) -> Result<U, Error>
    where
        D: TonicTryConnect,
        Layered<TLLayer<PersistenceSubscriber>, Self>: Into<Dispatch>,
    {
        use tracing_subscriber::util::SubscriberInitExt;
        let (layered, future, mut _guard) = self.with_tracing_lv(dst, app_info, setting).await?;

        let handle = tokio::spawn(future);
        layered.init();
        let r = f().await;
        _guard.normal_stop();
        drop(_guard);
        handle.await.unwrap();
        Ok(r)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppRunData {
    pub app_run_id: Uuid,
    pub start_pos: u64,
    pub end_pos: u64,
    pub last_record_index: u64,
}

#[allow(refining_impl_trait)]
impl<T> TLSubscriberExt for T
where
    T: SubscriberExt + for<'a> LookupSpan<'a>,
{
    async fn with_tracing_lv<D>(
        self,
        dst: D,
        app_info: TLAppInfo,
        setting: TLSetting,
    ) -> Result<
        (
            Layered<TLLayer<PersistenceSubscriber>, Self>,
            impl Future<Output = ()> + Send + 'static,
            TLGuard,
        ),
        Error,
    >
    where
        D: TonicTryConnect,
    {
        let run_id = Uuid::new_v4();

        // let channel = dst.try_connect().await?;
        //
        // let mut client = TracingServiceClient::new(NoSubscriberService(channel))
        //     .send_compressed(CompressionEncoding::Zstd)
        //     .max_decoding_message_size(usize::MAX)
        //     .accept_compressed(CompressionEncoding::Zstd);
        let (msg_sender, msg_receiver) = flume::unbounded();
        // let _app_start = {
        //     let instant = Instant::now();
        //     let _ = client.ping(PingParam {}).await.unwrap();
        //     let rtt = instant.elapsed();
        //     let app_start = app_info.into_app_start(run_id, rtt);
        //     msg_sender
        //         .send(RecordParam {
        //             send_time: app_start.record_time.clone(),
        //             record_index: 0,
        //             variant: Some(record_param::Variant::AppStart(app_start.clone())),
        //         })
        //         .unwrap();
        //     app_start
        // };
        let write_sync_semaphore = Arc::new(tokio::sync::Semaphore::new(1024));
       let (subscriber,subscriber_task_handler) = PersistenceSubscriber::new(
          run_id,
          TLReconnectAndPersistenceSetting::from_file(
             std::fs::OpenOptions::new()
                .write(true)
                .read(true)
                .create(true)
                .open("./test.zip")
                .unwrap(),
          )
             .unwrap(),
       ).unwrap();
        Ok((
            self.with(TLLayer {
                /*                subscriber: (
                    MsgReceiverSubscriber::new(msg_sender.clone()),
                    setting.reconnect_and_persistence.map(|setting| {
                        use bytes::Buf;
                        let (sender, receiver) = flume::unbounded::<(Bytes, u64)>();
                        let write_sync_semaphore = write_sync_semaphore.clone();
                        // tokio::spawn(async move {
                        //     let mut len = 0;
                        //     let mut app_run_data_file = setting.metadata_info_writer;
                        //     let app_run_start_pos = setting.metadata_info_cur_len;
                        //     let mut encoder = async_compression::tokio::write::ZstdEncoder::new(
                        //         setting.records_writer,
                        //     );
                        //     let mut buf = BytesMut::new();
                        //     let mut app_run_data = AppRunData {
                        //         app_run_id: run_id,
                        //         start_pos: app_run_start_pos,
                        //         end_pos: app_run_start_pos,
                        //         last_record_index: 0,
                        //     };
                        //     while let Ok((bytes, record_index)) = receiver.recv_async().await {
                        //         app_run_data.last_record_index = record_index;
                        //         app_run_data.end_pos = len;
                        //         encoder
                        //             .get_mut()
                        //             .write_u64(app_run_data.last_record_index)
                        //             .await
                        //             .unwrap();
                        //         encoder
                        //             .get_mut()
                        //             .write_u64(bytes.chunk().len() as u64)
                        //             .await
                        //             .unwrap();
                        //         encoder.write_all(bytes.chunk()).await.unwrap();
                        //         // <num:8><json len:8><record json>
                        //         len += 8 + 8 + bytes.chunk().len() as u64;
                        //         serde_json::to_writer_pretty((&mut buf).writer(), &app_run_data)
                        //             .unwrap();
                        //         app_run_data_file
                        //             .seek(SeekFrom::Start(app_run_data.start_pos))
                        //             .await
                        //             .unwrap();
                        //         app_run_data_file.write_all(buf.chunk()).await.unwrap();
                        //         app_run_data_file.flush().await.unwrap();
                        //         write_sync_semaphore.add_permits(1);
                        //         buf.clear();
                        //     }
                        // });
                        thread_local! {
                            static BUF: RefCell<BytesMut> = RefCell::new(BytesMut::new());
                        }
                        Box::new(move |msg: &TLMsg| {
                            BUF.with_borrow_mut(|buf| {
                                serde_json::to_writer_pretty(buf.writer(), msg).unwrap();
                                let _ = sender.send((buf.split().freeze(), msg.record_index()));
                            })
                        }) as _
                    }),
                ),*/
                subscriber,
                enable_enter: false,
                record_index: 1.into(),
            }),
            {
                async move {
                   subscriber_task_handler.await.unwrap().unwrap();
                    // let app_run =
                    //     |mut client: TracingServiceClient<NoSubscriberService<Channel>>| {
                    //         let msg_receiver = msg_receiver.clone();
                    //         async move {
                    //             let stream = futures_util::stream::unfold(
                    //                 (msg_receiver, None, false),
                    //                 move |(msg_receiver, mut app_stop, is_end)| async move {
                    //                     if is_end {
                    //                         return None;
                    //                     }
                    //                     let (mut param, app_stop, is_end) = if app_stop.is_some() {
                    //                         yield_now().await;
                    //                         let param = msg_receiver
                    //                             .try_recv()
                    //                             .ok()
                    //                             .or_else(|| app_stop.take())
                    //                             .unwrap();
                    //                         let is_end = app_stop.is_none();
                    //                         (param, app_stop, is_end)
                    //                     } else {
                    //                         let param = msg_receiver.recv_async().await.ok()?;
                    //                         if matches!(
                    //                             param.variant.as_ref().unwrap(),
                    //                             record_param::Variant::AppStop(_)
                    //                         ) {
                    //                             let mut app_stop = Some(param);
                    //                             yield_now().await;
                    //                             let param = msg_receiver
                    //                                 .try_recv()
                    //                                 .ok()
                    //                                 .or_else(|| app_stop.take())
                    //                                 .unwrap();
                    //                             let is_end = app_stop.is_none();
                    //                             (param, app_stop, is_end)
                    //                         } else {
                    //                             (param, None, false)
                    //                         }
                    //                     };
                    //                     param.send_time = Utc::now().timestamp_nanos_opt().unwrap();
                    //                     Some((param, (msg_receiver, app_stop, is_end)))
                    //                 },
                    //             );
                    //             (client.app_run(stream).await, client)
                    //         }
                    //     };
                    // loop {
                    //     let (r, _client) = { app_run(client).await };
                    //     client = _client;
                    //     match r {
                    //         Ok(record_param) => {
                    //             warn!("not expected app run end. {record_param:?}");
                    //         }
                    //         Err(err) => {
                    //             error!("app run error end. {err:?}");
                    //             eprintln!("app run error end. {err:?}");
                    //             tokio::time::sleep(Duration::from_secs(10)).await;
                    //             eprintln!("reconnect");
                    //         }
                    //     }
                    // }
                }
            },
            TLGuard {
                msg_sender,
                is_normal_drop: false,
            },
        ))
    }
}


#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn ff() {
        let x = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .read(true)
            .open("./test.txt")
            .await
            .unwrap();
        let mut x = async_compression::tokio::write::ZstdEncoder::new(x);
        x.write_all(b"{FFDSFNFSJFS{SFADFSNSDFKFDKJ}").await.unwrap();
        x.flush().await.unwrap();
        let len = x.get_mut().metadata().await.unwrap().len();
        println!("len: {len:?}");
        x.write_all(b"{FFDSFNFSJFS{SFADFSNSDFKFDKJ}").await.unwrap();
        x.flush().await.unwrap();
        let len = x.get_mut().metadata().await.unwrap().len();
        println!("len: {len:?}");
        for _ in 0..100 {
            x.write_all(b"{FFDSFNFSJFS{SFADFSNSDFKFDKJ}").await.unwrap();
            x.flush().await.unwrap();
            let len = x.get_mut().metadata().await.unwrap().len();
            println!("len: {len:?}");
        }
    }
}
