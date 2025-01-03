use axum_server::tls_rustls::RustlsConfig;
use sea_orm::{ConnectOptions, Database};
use std::env;
use std::future::IntoFuture;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::TcpListener;
use tonic::codec::CompressionEncoding;
use tonic::transport::Server;
use tonic::Status;
use tower_http::compression::CompressionLayer;
use tracing::{info, info_span, instrument, warn, Instrument, Span};
use tracing_lv_core::{
    proto::tracing_service_server::TracingServiceServer,
    proto::{record_param, RecordParam},
    MsgReceiverSubscriber, TLAppInfo, TLAppInfoExt, TLLayer,
};
use tracing_lv_server::{
    build,
    grpc_service::{AppRunLifetime, TracingServiceImpl},
    running_app::RunMsg,
    running_app::RunningApps,
    tracing_service::TracingService,
    web_service,
};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use uuid::{uuid, Uuid};

#[instrument]
fn program_panic_catch() {
    let prev_hook = std::panic::take_hook();
    let span = Span::current();
    std::panic::set_hook(Box::new(move |panic_info| {
        span.in_scope(|| {
            tracing_panic::panic_hook(panic_info);
            prev_hook(panic_info);
        })
    }));
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (self_record_sender, self_record_receiver) = flume::unbounded::<RecordParam>();
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            format!(
            "{}=debug,tower_http=debug,axum::rejection=trace,tracing_lv_server::running_app=warn",
            env!("CARGO_CRATE_NAME")
         )
            .into()
        }))
        .with(TLLayer {
            subscriber: MsgReceiverSubscriber::new(self_record_sender),
            enable_enter: false,
            record_index: 1.into(),
        })
        .with(tracing_subscriber::fmt::layer().pretty())
        .init();
    program_panic_catch();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or("postgresql://postgres:123456@127.0.0.1:5432/tracing-dev".into());
    let dc = Database::connect(ConnectOptions::new(database_url.as_str()))
        .instrument(info_span!("connect db", database_url))
        .await
        .expect("Fail to initialize database connection");

    let tracing_service = TracingService::new(dc.clone());

    let (msg_sender, msg_receiver) = flume::unbounded::<RunMsg>();
    tracing_service.init().await?;
    {
        let tracing_service = tracing_service.clone();
        let msg_sender = msg_sender.clone();
        tokio::spawn(async move {
            let fut = async move {
                let app_info = TLAppInfo::new(
                    uuid!("51E5297E-949F-DABC-76B1-F34E5FCEA32F"),
                    "Tracing Live Server",
                    build::PKG_VERSION,
                )
                .node_name("Server");
                let mut self_lifetime = AppRunLifetime::new(
                    app_info.into_app_start(Uuid::new_v4(), Duration::default()),
                    tracing_service,
                    msg_sender,
                )
                .await?;
                while let Ok(msg) = self_record_receiver.recv_async().await {
                    let variant = msg.variant.unwrap();
                    let record = if let record_param::Variant::AppStop(_) = variant {
                        unreachable!("AppStop should not be sent to self_record_receiver");
                        // break;
                    } else {
                        self_lifetime.record(variant).await?
                    };
                    if let Err(err) = self_lifetime.record_sender.send(RunMsg::Record {
                        record_index: msg.record_index,
                        variant: record,
                    }) {
                        info!(?err, "record_sender send failed. exit!");
                        break;
                    }
                }
                anyhow::Ok(())
            };
            fut.await.inspect_err(|err| {
                warn!(?err, "Self tracing record task error");
            })
        });
    }


    let handle_records_future = tokio::spawn({
        let tracing_service = tracing_service.clone();
        let id = tracing_service
            .query_last_record_id()
            .await
            .map_err(|err| Status::internal(format!("query_last_record_id error. {err}")))?
            .unwrap_or(0);
        let max_buf_count = env::var("RECORD_MAX_BUF_COUNT")
            .ok()
            .map(|n| n.parse::<usize>().ok())
            .flatten();
        async move {
            let mut running_apps = RunningApps::new(tracing_service, id, max_buf_count);
            running_apps.handle_records(msg_receiver).await
        }
        .instrument(info_span!("records handle task", max_buf_count))
    });
    let web_router = info_span!("web").in_scope(|| {
        web_service::router(tracing_service.clone(), msg_sender.clone())
            .layer(CompressionLayer::new())
    });
    let https_web_serve_future = tokio::spawn({
        let addr = SocketAddr::from((
            Ipv4Addr::UNSPECIFIED,
            std::env::var("WEB_PORT")
                .ok()
                .map(|n| n.parse().ok())
                .flatten()
                .unwrap_or(443),
        ));
        axum_server::bind_rustls(
            addr,
            RustlsConfig::from_pem(
                include_bytes!("../cert/server.pem").into(),
                include_bytes!("../cert/server.key").into(),
            )
            .await?,
        )
        .serve(web_router.clone().into_make_service())
        .instrument(info_span!("axum https web server", ?addr))
    });

    let http_web_serve_future = tokio::spawn({
        let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 80));
        axum::serve(
            TcpListener::bind(addr).await?,
            web_router.clone().into_make_service(),
        )
        .into_future()
        .instrument(info_span!("axum http web server", ?addr))
    });

    let grpc_serve_future = tokio::spawn({
        let addr = SocketAddr::from((
            Ipv4Addr::UNSPECIFIED,
            std::env::var("GRPC_PORT")
                .ok()
                .map(|n| n.parse().ok())
                .flatten()
                .unwrap_or(8080),
        ));
        let span = info_span!("tonic grpc server", ?addr);
        Server::builder()
            .accept_http1(false)
            .add_service(
                TracingServiceServer::new(TracingServiceImpl::new(
                    tracing_service,
                    msg_sender,
                    span.clone(),
                ))
                .accept_compressed(CompressionEncoding::Zstd)
                .send_compressed(CompressionEncoding::Zstd),
            )
            .serve(addr)
            .instrument(span)
    });

    Ok(tokio::select! {
        r = https_web_serve_future => {
            r??
        }
        r = http_web_serve_future => {
            r??
        }
        r = grpc_serve_future => {
            r??
        }
        r = handle_records_future => {
            r??
        }
    })
}
