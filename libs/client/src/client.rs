use bytes::{Buf, BufMut, Bytes, BytesMut};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::io::SeekFrom;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::task::yield_now;
use tokio::time::Instant;
use tonic::codegen::{CompressionEncoding, Service};
use tracing::instrument::{WithDispatch, WithSubscriber};
use tracing::subscriber::NoSubscriber;
use tracing_core::Dispatch;
use tracing_lv_proto::proto::tracing_service_client::TracingServiceClient;
use tracing_lv_proto::proto::{record_param, AppStop, Ping, RecordParam};
use tracing_lv_proto::{MsgReceiverSubscriber, TLAppInfo, TLLayer, TLMsg};
use tracing_subscriber::layer::{Layered, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;

pub trait TLSubscriberExt: Sized {
    async fn with_tracing_lv<D>(
        self,
        dst: D,
        app_info: TLAppInfo,
    ) -> Result<
        (
            Layered<
                TLLayer<(
                    MsgReceiverSubscriber,
                    Box<dyn Fn(&TLMsg) + Send + Sync + 'static>,
                )>,
                Self,
            >,
            impl Future<Output = ()> + Send + 'static,
            TLGuard,
        ),
        tonic::transport::Error,
    >
    where
        D: TryInto<tonic::transport::Endpoint>,
        D::Error: Into<tonic::codegen::StdError>;

    async fn tracing_lv_init<D, U, F: Future<Output = U> + 'static>(
        self,
        dst: D,
        app_info: TLAppInfo,
        f: impl FnOnce() -> F,
    ) -> Result<U, tonic::transport::Error>
    where
        D: TryInto<tonic::transport::Endpoint>,
        D::Error: Into<tonic::codegen::StdError>,
        Layered<
            TLLayer<(
                MsgReceiverSubscriber,
                Box<dyn Fn(&TLMsg) + Send + Sync + 'static>,
            )>,
            Self,
        >: Into<Dispatch>,
    {
        use tracing_subscriber::util::SubscriberInitExt;
        let (layered, future, mut _guard) = self.with_tracing_lv(dst, app_info).await?;

        let handle = tokio::spawn(future);
        layered.init();
        let r = f().await;
        _guard.normal_stop();
        drop(_guard);
        handle.await.unwrap();
        Ok(r)
    }
}

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
                variant: Some(record_param::Variant::AppStop(AppStop {})),
            });
        }
    }
}

impl<T> TLSubscriberExt for T
where
    T: SubscriberExt + for<'a> LookupSpan<'a>,
{
    async fn with_tracing_lv<D>(
        self,
        dst: D,
        app_info: TLAppInfo,
    ) -> Result<
        (
            Layered<
                TLLayer<(
                    MsgReceiverSubscriber,
                    Box<dyn Fn(&TLMsg) + Send + Sync + 'static>,
                )>,
                Self,
            >,
            impl Future<Output = ()> + 'static,
            TLGuard,
        ),
        tonic::transport::Error,
    >
    where
        D: TryInto<tonic::transport::Endpoint>,
        D::Error: Into<tonic::codegen::StdError>,
    {
        let run_id = Uuid::new_v4();

        let channel = tonic::transport::Endpoint::new(dst)?
            .executor(NoSubscriberExecutor)
            // .tcp_nodelay(false)
            .keep_alive_timeout(Duration::from_secs(120))
            .connect()
            .await?;

        let mut client = TracingServiceClient::new(NoSubscriberService(channel))
            .send_compressed(CompressionEncoding::Zstd)
            .accept_compressed(CompressionEncoding::Zstd);
        let (msg_sender, msg_receiver) = flume::unbounded();
        {
            let instant = Instant::now();
            let _ = client.ping(Ping {}).await.unwrap();
            let rtt = instant.elapsed();
            let app_start = app_info.into_app_start(run_id, rtt);
            msg_sender
                .send(RecordParam {
                    send_time: app_start.record_time.clone(),
                    variant: Some(record_param::Variant::AppStart(app_start)),
                })
                .unwrap();
        }
        /*
        <app_run><num><is_upload>
        */
        /*
        <num ><msg>
         */
        Ok((
            self.with(TLLayer {
                subscriber: GrpcTracingLiveMsgSubscriber {
                    sender: msg_sender.clone(),
                },
                enable_enter: false,
            }),
            {
                async move {
                    client
                        .app_run(futures_util::stream::unfold(
                            (msg_receiver, None, false),
                            move |(msg_receiver, mut app_stop, is_end)| async move {
                                if is_end {
                                    return None;
                                }
                                let (mut param, app_stop, is_en) = if app_stop.is_some() {
                                    yield_now().await;
                                    let mut param = msg_receiver
                                        .try_recv()
                                        .ok()
                                        .or_else(|| app_stop.take())
                                        .unwrap();
                                    let is_end = app_stop.is_none();
                                    (param, app_stop, is_end)
                                } else {
                                    let mut param = msg_receiver.recv_async().await.ok()?;
                                    if matches!(
                                        param.variant.as_ref().unwrap(),
                                        record_param::Variant::AppStop(_)
                                    ) {
                                        let mut app_stop = Some(param);
                                        yield_now().await;
                                        let mut param = msg_receiver
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
                        ))
                        .await
                        .unwrap();
                }
            },
            TLGuard {
                msg_sender,
                is_normal_drop: false,
            },
        ))
    }
}
