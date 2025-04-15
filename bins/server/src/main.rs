use axum_server::tls_rustls::RustlsConfig;
use sea_orm::{ConnectOptions, Database};
use std::env;
use std::future::IntoFuture;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::{TcpListener, TcpSocket};
use tower_http::compression::CompressionLayer;
use tracing::{Instrument, error, info, info_span, warn};
use tracing_lv_core::catch_panic::program_panic_catch;
use tracing_lv_core::proto::{AppStartInfo, TLRecordVariant, TracingRecordItem};
use tracing_lv_core::{MsgReceiverSubscriber, TLAppInfo, TLLayer};
use tracing_lv_server::rpc_service::{AppRunLifetime, TracingServiceImpl};
use tracing_lv_server::running_app::{
    AppRunMsg, AppRunRecord, POSTGRESQL_MAX_BIND_PARAM_COUNT, TLConfig,
};
use tracing_lv_server::tracing_service::TracingRecordBatchInserter;
use tracing_lv_server::{
    RECORD_ID_GENERATOR, SELF_APP_ID, build, running_app::RunMsg, running_app::RunningApps,
    tracing_service::TracingService, web_service,
};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use uuid::Uuid;
use xy_rpc::formats::MessagePackFormat;
use xy_rpc::tokio::ChannelBuilderTokioExt;
use xy_rpc::{ChannelBuilder, XyRpcChannel};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (self_record_sender, self_record_receiver) = flume::unbounded::<TracingRecordItem>();
    tracing_subscriber::registry()
      .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
         format!(
            "warn,tracing_lv_core::catch_panic=info,{}=info,tower_http=debug,axum::rejection=trace,tracing_lv_server::running_app=warn",
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
                let app_info =
                    TLAppInfo::new(SELF_APP_ID, "Tracing Live Server", build::PKG_VERSION)
                        .node_name("Server");
                let mut self_lifetime = AppRunLifetime::new(
                    AppStartInfo::from_app_info(app_info, Uuid::new_v4()),
                    tracing_service,
                    msg_sender,
                )
                .await?;
                while let Ok(msg) = self_record_receiver.recv_async().await {
                    let record_index = msg.variant.record_index();
                    let record = match msg.variant {
                        TLRecordVariant::TLMsg(msg) => self_lifetime.record(msg).await?,
                        TLRecordVariant::AppStop { .. } => {
                            unreachable!("AppStop should not be sent to self_record_receiver");
                        }
                    };
                    if let Err(err) =
                        self_lifetime
                            .app_run_record_sender
                            .send(AppRunMsg::Record(AppRunRecord {
                                id: RECORD_ID_GENERATOR.next(),
                                record_index: record_index as _,
                                variant: record,
                            }))
                    {
                        error!(?err, "record_sender send failed. exit!");
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
        let max_buf_count = env::var("RECORD_MAX_BUF_COUNT")
            .ok()
            .map(|n| n.parse::<usize>().ok())
            .flatten();
        async move {
            let max_buf_count_max_value =
                POSTGRESQL_MAX_BIND_PARAM_COUNT / TracingRecordBatchInserter::BIND_COL_COUNT;
            let record_max_buf_count = max_buf_count
                .unwrap_or(max_buf_count_max_value)
                .min(max_buf_count_max_value);

            let mut running_apps = RunningApps::new(
                tracing_service,
                TLConfig {
                    record_max_delay: Duration::from_millis(
                        env::var("RECORD_MAX_DELAY")
                            .ok()
                            .map(|n| n.parse::<u64>().ok())
                            .flatten()
                            .unwrap_or(200),
                    ),
                    record_max_buf_count,
                },
            );
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
            env::var("WEB_PORT")
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

    let rpc_serve_future = tokio::spawn(async move {
        let addr = SocketAddr::from((
            Ipv4Addr::UNSPECIFIED,
            env::var("GRPC_PORT")
                .ok()
                .map(|n| n.parse().ok())
                .flatten()
                .unwrap_or(8080),
        ));
        let span = info_span!("tonic grpc server", ?addr);
        let socket = TcpSocket::new_v4()?;
        socket.set_keepalive(true)?;
        socket.bind(addr)?;
        let mut tcp_listener = socket.listen(1024)?;
        // let tcp_listener = TcpListener::bind(addr).await?;

        while let Ok((stream, addr)) = tcp_listener.accept().await {
            let (_channel, fut) = ChannelBuilder::new(tracing_lv_core::proto::FORMAT::default())
                .only_serve({
                    {
                        let n = TracingServiceImpl {
                            tracing_service: tracing_service.clone(),
                            span: span.clone(),
                            record_sender: msg_sender.clone(),
                        };
                        move |_channel: XyRpcChannel<MessagePackFormat>| n
                    }
                })
                .build_from_tokio_read_write(stream.into_split());
            tokio::spawn(async move {
                if let Err(e) = fut.await {
                    eprintln!("rpc error: {:?}", e);
                };
                drop(_channel);
            });
        }
        anyhow::Ok(())
        // Server::builder()
        //     .accept_http1(false)
        //     .add_service(
        //         TracingServiceServer::new(TracingServiceImpl::new(
        //             tracing_service,
        //             msg_sender,
        //             span.clone(),
        //         ))
        //         .accept_compressed(CompressionEncoding::Zstd)
        //         .send_compressed(CompressionEncoding::Zstd),
        //     )
        //     .serve(addr)
        //     .instrument(span)
    });

    Ok(tokio::select! {
        r = https_web_serve_future => {
            r??
        }
        r = http_web_serve_future => {
            r??
        }
        r = rpc_serve_future => {
            r??
        }
        r = handle_records_future => {
            r?
        }
    })
}
