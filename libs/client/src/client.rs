#[cfg(feature = "reconnect_and_persistence")]
use crate::reconnect_and_persistence::{
    TLReconnectAndPersistenceSetting, reconnect_and_persistence,
};
use bytes::Bytes;
use chrono::Utc;
use derive_more::{Display, Error, From};
use flume::Receiver;
use futures_util::{FutureExt, StreamExt};
use hyper::Uri;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite};
use tokio::net::ToSocketAddrs;
use tokio::task::yield_now;
use tokio::time::Instant;
use tracing::instrument::{WithDispatch, WithSubscriber};
use tracing::subscriber::NoSubscriber;
use tracing_core::Dispatch;
use tracing_lv_core::proto::{
    AppStartInfo, TLRecordVariant, TracingRecordItem, TracingServiceCaller, TracingServiceSchema,
};
use tracing_lv_core::{MsgReceiverSubscriber, TLAppInfo, TLLayer, TLMsg, TracingLiveMsgSubscriber};
use tracing_subscriber::layer::{Layered, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use uuid::Uuid;
use xy_rpc::tokio::ChannelBuilderTokioExt;
use xy_rpc::{ChannelBuilder, RpcError, XyRpcChannel};

#[derive(Error, From, Display, Debug)]
pub enum TLError {
    Io(std::io::Error),
    Rpc(RpcError),
}

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

pub struct TLGuard {
    msg_sender: flume::Sender<TracingRecordItem>,
    is_normal_drop: bool,
}

impl TLGuard {
    pub fn normal_stop(&mut self) {
        self.is_normal_drop = true;

        let _ = self.msg_sender.send(TracingRecordItem {
            send_time: Utc::now(),
            variant: TLRecordVariant::AppStop {
                exception_end: false,
            },
        });
    }
}

impl Drop for TLGuard {
    fn drop(&mut self) {
        if !self.is_normal_drop {
            // TODO:
            let _ = self.msg_sender.send(TracingRecordItem {
                send_time: Utc::now(),
                variant: TLRecordVariant::AppStop {
                    exception_end: true,
                },
            });
        }
    }
}

pub trait AsyncWriteWithSeek: AsyncWrite + AsyncSeek {}

impl<T> AsyncWriteWithSeek for T where T: AsyncWrite + AsyncSeek {}
pub trait AsyncWriteWithReadAndSeek: AsyncWriteWithSeek + AsyncRead {}

impl<T> AsyncWriteWithReadAndSeek for T where T: AsyncWriteWithSeek + AsyncRead {}

#[derive(Default)]
pub struct TLSetting {
    #[cfg(feature = "reconnect_and_persistence")]
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
        TLError,
    >
    where
        D: ToSocketAddrs;

    async fn tracing_lv_init<D, U, F: Future<Output = U> + 'static>(
        self,
        dst: D,
        app_info: TLAppInfo,
        setting: TLSetting,
        f: impl FnOnce() -> F,
    ) -> Result<U, TLError>
    where
        D: ToSocketAddrs,
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
        _setting: TLSetting,
    ) -> Result<
        (
            Layered<TLLayer<Box<dyn TracingLiveMsgSubscriber>>, Self>,
            impl Future<Output = ()> + Send + 'static,
            TLGuard,
        ),
        TLError,
    >
    where
        D: ToSocketAddrs,
    {
        let run_id = Uuid::new_v4();

        let stream = tokio::net::TcpStream::connect(dst).await?;
        let (channel, channel_fut) = ChannelBuilder::new(tracing_lv_core::proto::FORMAT::default())
            .only_call()
            .build_from_tokio_read_write(stream.into_split());
        // TODO:
        tokio::spawn(channel_fut);

        let (msg_sender, msg_receiver) = flume::unbounded();

        let _ = channel.ping().await?;
        let app_start_info = AppStartInfo::from_app_info(app_info, run_id);

        #[cfg(feature = "reconnect_and_persistence")]
        let (subscriber, mut future) = {
            use futures_util::FutureExt;
            match _setting.reconnect_and_persistence {
                None => (
                    Box::new(MsgReceiverSubscriber::new(msg_sender.clone())) as _,
                    async move {
                        if let Err(err) =
                            tracing_msg_subscriber(app_start_info, channel, msg_receiver).await
                        {
                            eprintln!("error: {err:?}");
                        }
                    }
                    .left_future(),
                ),
                Some(_setting) => {
                    let (subscriber, future) = reconnect_and_persistence(
                        _setting,
                        msg_sender.clone(),
                        msg_receiver,
                        app_start_info,
                        channel,
                    )
                    .await;
                    (subscriber, future.right_future())
                }
            }
        };

        #[cfg(not(feature = "reconnect_and_persistence"))]
        let (subscriber, future) = {
            drop(app_start_info);
            (
                Box::new(MsgReceiverSubscriber::new(msg_sender.clone())) as _,
                crate::client::tracing_msg_subscriber(client, msg_receiver),
            )
        };

        Ok((
            self.with(TLLayer {
                subscriber,
                enable_enter: false,
                record_index: 1.into(),
            }),
            future,
            // async move {
            //     futures_util::select! {
            //         r = future.fuse() => {},
            //         r = channel_fut.fuse() => {}
            //     }
            // },
            TLGuard {
                msg_sender,
                is_normal_drop: false,
            },
        ))
    }
}

fn tracing_msg_subscriber(
    app_start_info: AppStartInfo,
    channel: XyRpcChannel<tracing_lv_core::proto::FORMAT, TracingServiceSchema>,
    msg_receiver: Receiver<TracingRecordItem>,
) -> impl Future<Output = Result<(), RpcError>> + Sized + Send + 'static {
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
                    if matches!(param.variant, TLRecordVariant::AppStop { .. }) {
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
                param.send_time = Utc::now();
                Some((param, (msg_receiver, app_stop, is_end)))
            },
        );
        let stream = channel.app_run(&app_start_info, stream.map(Ok)).await?;
        let mut stream = core::pin::pin!(stream);
        while let Some(item) = stream.next().await {
            item?;
        }
        Ok(())
    }
}
