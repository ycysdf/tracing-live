use crate::{SpanAttrs, SpanInfo, TLLayer, TLMsg, TLValue, TracingLiveMsgSubscriber, VxMetadata};
use chrono::Utc;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::future::Future;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::task::yield_now;
use tokio::time::Instant;
use tonic::codegen::{CompressionEncoding, Service};
use tracing::instrument::{WithDispatch, WithSubscriber};
use tracing::subscriber::NoSubscriber;
use tracing_core::Dispatch;
use tracing_lv_proto::tracing_service_client::TracingServiceClient;
use tracing_lv_proto::{
    field_value, record_param, AppStart, AppStop, Event, FieldValue, Ping, PosInfo, RecordParam,
    SpanClose, SpanCreate, SpanEnter, SpanLeave, SpanRecordField,
};
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
            Layered<TLLayer<GrpcTracingLiveMsgSubscriber>, Self>,
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
        Layered<TLLayer<GrpcTracingLiveMsgSubscriber>, Self>: Into<Dispatch>,
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

impl From<TLValue> for field_value::Variant {
    fn from(value: TLValue) -> Self {
        match value {
            TLValue::F64(n) => field_value::Variant::F64(n),
            TLValue::I64(n) => field_value::Variant::I64(n),
            TLValue::U64(n) => field_value::Variant::U64(n),
            TLValue::Bool(n) => field_value::Variant::Bool(n),
            TLValue::Str(n) => field_value::Variant::String(n.into()),
        }
    }
}
impl From<TLValue> for FieldValue {
    fn from(value: TLValue) -> Self {
        FieldValue {
            variant: Some(value.into()),
        }
    }
}

impl<'a> From<&'a VxMetadata> for PosInfo {
    fn from(value: &'a VxMetadata) -> Self {
        Self {
            module_path: value.module_path.unwrap_or("unknown").into(),
            file_line: format!(
                "{}:{}",
                value.file.unwrap_or("unknown"),
                value.line.unwrap_or(0)
            ),
        }
    }
}
impl<'a> From<&'a SpanInfo> for tracing_lv_proto::SpanInfo {
    fn from(value: &'a SpanInfo) -> Self {
        Self {
            t_id: value.id,
            name: value.metadata.name.into(),
            file_line: format!(
                "{}:{}",
                value.metadata.file.unwrap_or("unknown"),
                value.metadata.line.unwrap_or(0)
            ),
        }
    }
}

impl From<SpanAttrs> for HashMap<String, FieldValue> {
    fn from(value: SpanAttrs) -> Self {
        value
            .0
            .into_iter()
            .map(|n| (n.0.into(), n.1.into()))
            .collect()
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

#[derive(Clone, Debug)]
pub struct TLAppInfo {
    pub app_id: Uuid,
    pub app_name: String,
    pub app_version: String,
    pub node_id: String,
    pub static_data: HashMap<String, FieldValueVariant>,
    pub data: HashMap<String, FieldValueVariant>,
}

impl TLAppInfo {
    pub fn new(
        app_id: impl Into<Uuid>,
        app_name: impl Into<String>,
        app_version: impl Into<String>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            app_name: app_name.into(),
            app_version: app_version.into(),
            node_id: "default_node".to_string(),
            static_data: Default::default(),
            data: Default::default(),
        }
    }

    pub fn node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = node_id.into();
        self
    }

    pub fn with_data(
        mut self,
        name: impl Into<String>,
        value: impl Into<FieldValueVariant>,
    ) -> Self {
        self.data.insert(name.into(), value.into());
        self
    }

    pub fn with_static_data(
        mut self,
        name: impl Into<String>,
        value: impl Into<FieldValueVariant>,
    ) -> Self {
        self.static_data.insert(name.into(), value.into());
        self
    }
}

pub type FieldValueVariant = field_value::Variant;

pub struct GrpcTracingLiveMsgSubscriber {
    sender: flume::Sender<RecordParam>,
}

impl TracingLiveMsgSubscriber for GrpcTracingLiveMsgSubscriber {
    fn on_msg(&self, msg: TLMsg) {
        if self.sender.is_disconnected() {
            return;
        }
        if let Err(err) = self.sender.send(msg.into()) {
            if let Some(variant) = &err.0.variant {
                if let record_param::Variant::Event(e) = variant {
                    if e.target == "tower::buffer::worker" {
                        return;
                    }
                }
            }
            eprintln!("failed to send msg. {:?}", err.0.variant);
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
            Layered<TLLayer<GrpcTracingLiveMsgSubscriber>, Self>,
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
            let record_time = Utc::now().timestamp_nanos_opt().unwrap();
            msg_sender
                .send(RecordParam {
                    send_time: record_time.clone(),
                    variant: Some(record_param::Variant::AppStart(AppStart {
                        record_time,
                        id: app_info.app_id.into(),
                        run_id: run_id.into(),
                        node_id: app_info.node_id,
                        name: app_info.app_name,
                        version: app_info.app_version,
                        data: app_info
                            .data
                            .into_iter()
                            .map(|n| (n.0, FieldValue { variant: Some(n.1) }))
                            .collect(),
                        rtt: rtt.as_secs_f64(),
                    })),
                })
                .unwrap();
        }
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
                                if app_stop.is_some() {
                                    yield_now().await;
                                    let param = msg_receiver
                                        .try_recv()
                                        .ok()
                                        .or_else(|| app_stop.take())
                                        .unwrap();
                                    let is_end = app_stop.is_none();
                                    return Some((param, (msg_receiver, app_stop, is_end)));
                                }
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
                                    param.send_time = Utc::now().timestamp_nanos_opt().unwrap();
                                    Some((param, (msg_receiver, app_stop, is_end)))
                                } else {
                                    param.send_time = Utc::now().timestamp_nanos_opt().unwrap();
                                    Some((param, (msg_receiver, None, false)))
                                }
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

impl From<TLMsg> for RecordParam {
    fn from(value: TLMsg) -> Self {
        Self {
            send_time: 0,
            variant: Some(value.into()),
        }
    }
}

impl From<TLMsg> for record_param::Variant {
    fn from(value: TLMsg) -> Self {
        match value {
            TLMsg::SpanCreate {
                date,
                span,
                attributes,
                parent_span,
            } => record_param::Variant::SpanCreate(SpanCreate {
                record_time: date.timestamp_nanos_opt().unwrap(),
                span_info: Some(span.borrow().into()),
                pos_info: Some(span.metadata.borrow().into()),
                parent_span_info: parent_span.map(|n| n.borrow().into()),
                fields: attributes.into(),
                target: span.metadata.target.into(),
                level: tracing_lv_proto::Level::from_str_name(span.metadata.level)
                    .unwrap()
                    .into(),
            }),
            TLMsg::SpanRecordAttr {
                span,
                date,
                attributes,
            } => record_param::Variant::SpanRecordField(SpanRecordField {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some(span.metadata.borrow().into()),
                span_info: Some(span.borrow().into()),
                fields: attributes.into(),
            }),
            TLMsg::SpanEnter { span, date } => record_param::Variant::SpanEnter(SpanEnter {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some(span.metadata.borrow().into()),
                span_info: Some(span.borrow().into()),
            }),
            TLMsg::SpanLeave { span, date } => record_param::Variant::SpanLeave(SpanLeave {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some(span.metadata.borrow().into()),
                span_info: Some(span.borrow().into()),
            }),
            TLMsg::SpanClose { span, date } => record_param::Variant::SpanClose(SpanClose {
                record_time: date.timestamp_nanos_opt().unwrap(),
                pos_info: Some(span.metadata.borrow().into()),
                span_info: Some(span.borrow().into()),
            }),
            TLMsg::Event {
                message,
                metadata,
                attributes,
                span,
                date,
            } => record_param::Variant::Event(Event {
                record_time: date.timestamp_nanos_opt().unwrap(),
                message: message.into(),
                pos_info: Some(metadata.borrow().into()),
                fields: attributes.into(),
                target: metadata.target.into(),
                span_info: span.map(|n| n.borrow().into()),
                level: tracing_lv_proto::Level::from_str_name(metadata.level)
                    .unwrap()
                    .into(),
            }),
        }
    }
}
