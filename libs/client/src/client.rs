use crate::reconnect_and_persistence::{
    reconnect_and_persistence, TLReconnectAndPersistenceSetting,
};
use bytes::Bytes;
use chrono::Utc;
use futures_util::{FutureExt, SinkExt, StreamExt};
use hyper::Uri;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::io::Read;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncWrite};
use tokio::task::yield_now;
use tokio::time::Instant;
use tonic::codec::CompressionEncoding;
use tonic::codegen::{Service, StdError};
use tonic::transport::{Channel, Endpoint, Error};
use tracing::instrument::{WithDispatch, WithSubscriber};
use tracing::subscriber::NoSubscriber;
use tracing_core::Dispatch;
use tracing_lv_core::proto::tracing_service_client::TracingServiceClient;
use tracing_lv_core::proto::{record_param, AppStop, PingParam, RecordParam};
use tracing_lv_core::{MsgReceiverSubscriber, TLAppInfoExt};
use tracing_lv_core::{TLAppInfo, TLLayer, TracingLiveMsgSubscriber};
use tracing_subscriber::layer::{Layered, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;

pub struct NoSubscriberService<T>(T);
pub struct NoSubscriberExecutor;

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
) -> Result<Channel, Error> {
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
    async fn try_connect(self) -> Result<Channel, Error>;
}

impl TonicTryConnect for String {
    async fn try_connect(self) -> Result<Channel, Error> {
        default_connect(self).await
    }
}
impl TonicTryConnect for &'static str {
    async fn try_connect(self) -> Result<Channel, Error> {
        default_connect(self).await
    }
}
impl TonicTryConnect for Bytes {
    async fn try_connect(self) -> Result<Channel, Error> {
        default_connect(self).await
    }
}
impl TonicTryConnect for Uri {
    async fn try_connect(self) -> Result<Channel, Error> {
        let endpoint: Endpoint = self.into();
        default_connect(endpoint).await
    }
}
impl TonicTryConnect for Channel {
    async fn try_connect(self) -> Result<Channel, Error> {
        Ok(self)
    }
}
impl<F, FO> TonicTryConnect for F
where
    FO: Future<Output = Result<Channel, Error>>,
    F: FnOnce() -> FO,
{
    async fn try_connect(self) -> Result<Channel, Error> {
        self().await
    }
}

impl<T> TonicTryConnect for Vec<T>
where
    T: TryInto<Endpoint>,
    T::Error: Into<StdError>,
{
    async fn try_connect(self) -> Result<Channel, Error> {
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
            Layered<TLLayer<Box<dyn TracingLiveMsgSubscriber>>, Self>,
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
        Layered<TLLayer<Box<dyn TracingLiveMsgSubscriber>>, Self>: Into<Dispatch>,
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
            Layered<TLLayer<Box<dyn TracingLiveMsgSubscriber>>, Self>,
            impl Future<Output = ()> + Send + 'static,
            TLGuard,
        ),
        Error,
    >
    where
        D: TonicTryConnect,
    {
        let run_id = Uuid::new_v4();

        let channel = dst.try_connect().await?;

        let mut client = TracingServiceClient::new(NoSubscriberService(channel))
            .send_compressed(CompressionEncoding::Zstd)
            .max_decoding_message_size(usize::MAX)
            .accept_compressed(CompressionEncoding::Zstd);
        let (mut msg_sender, mut msg_receiver) = flume::unbounded();

        let app_start = {
            let instant = Instant::now();
            let _ = client.ping(PingParam {}).await.unwrap();
            let rtt = instant.elapsed();
            let app_start = app_info.into_app_start(run_id, rtt);
            msg_sender
                .send(RecordParam {
                    send_time: app_start.record_time.clone(),
                    record_index: 0,
                    variant: Some(record_param::Variant::AppStart(app_start.clone())),
                })
                .unwrap();
            app_start
        };

        let (subscriber, future) = match setting.reconnect_and_persistence {
            None => (
                Box::new(MsgReceiverSubscriber::new(msg_sender.clone())) as _,
                async move {
                    let app_run =
                        |mut client: TracingServiceClient<NoSubscriberService<Channel>>| {
                            let msg_receiver = msg_receiver.clone();
                            async move {
                                let stream = futures_util::stream::unfold(
                                    (msg_receiver, None, false),
                                    move |(msg_receiver, mut app_stop, is_end)| async move {
                                        if is_end {
                                            return None;
                                        }
                                        let (mut param, app_stop, is_end) = if app_stop.is_some() {
                                            yield_now().await;
                                            let param = msg_receiver
                                                .try_recv()
                                                .ok()
                                                .or_else(|| app_stop.take())
                                                .unwrap();
                                            let is_end = app_stop.is_none();
                                            (param, app_stop, is_end)
                                        } else {
                                            let param = msg_receiver.recv_async().await.ok()?;
                                            if matches!(
                                                param.variant.as_ref().unwrap(),
                                                record_param::Variant::AppStop(_)
                                            ) {
                                                let mut app_stop = Some(param);
                                                yield_now().await;
                                                tokio::time::sleep(Duration::from_secs(1)).await;
                                                let param = msg_receiver
                                                    .try_recv()
                                                    .ok()
                                                    .or_else(|| app_stop.take())
                                                    .unwrap();
                                                let is_end = app_stop.is_none();
                                                (param, app_stop, is_end)
                                            } else {
                                                (param, None, false)
                                            }
                                        };
                                        param.send_time = Utc::now().timestamp_nanos_opt().unwrap();
                                        Some((param, (msg_receiver, app_stop, is_end)))
                                    },
                                );
                                (client.app_run(stream).await, client)
                            }
                        };
                    let (r, _client) = app_run(client).await;
                    use futures_util::StreamExt;
                    match r {
                        Ok(mut response) => {
                            while let Some(item) = response.get_mut().next().await {
                                match item {
                                    Ok(_) => {}
                                    Err(err) => {
                                        eprintln!("tracing live stream error. {err:}");
                                        break;
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("tracing live connect error. {err:}")
                        }
                    }
                }
                .left_future(),
            ),
            Some(setting) => {
                let (subscriber, future) = reconnect_and_persistence(
                    setting,
                    msg_sender.clone(),
                    msg_receiver,
                    app_start,
                    client,
                )
                .await;
                (subscriber, future.right_future())
            }
        };

        Ok((
            self.with(TLLayer {
                subscriber,
                enable_enter: false,
                record_index: 1.into(),
            }),
            future,
            TLGuard {
                msg_sender,
                is_normal_drop: false,
            },
        ))
    }
}
