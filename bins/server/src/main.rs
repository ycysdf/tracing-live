#![feature(duration_millis_float)]
#![allow(unused_imports)]

mod dyn_query;
pub mod event_service;
mod global_data;
pub mod grpc_service;
pub mod record;
pub mod running_app;
mod setting_service;
mod tracing_service;
mod web_error;
mod web_service;

use crate::grpc_service::TracingServiceImpl;
use crate::record::TracingRecordVariant;
use crate::running_app::RunningApps;
use crate::tracing_service::{AppLatestInfoDto, TracingRecordDto, TracingService};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::serve::Serve;
use axum::{Json, Router};
use axum_server::tls_rustls::RustlsConfig;
use event_service::EventService;
use futures_util::future::Either;
use futures_util::{SinkExt, StreamExt};
use running_app::RunMsg;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ConnectOptions, Database};
use serde::{Deserialize, Serialize};
use std::env;
use std::future::{Future, IntoFuture};
use std::net::{Ipv4Addr, SocketAddr};
use std::pin::pin;
use std::str::FromStr;
use tokio::net::TcpListener;
use tonic::codec::CompressionEncoding;
use tonic::transport::{Error, Server};
use tonic::Status;
use tower_http::compression::{Compression, CompressionLayer};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tracing::info;
use tracing_lv_proto::tracing_service_server::TracingServiceServer;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            format!(
                "{}=debug,tower_http=debug,axum::rejection=trace",
                env!("CARGO_CRATE_NAME")
            )
            .into()
        }))
        .with(tracing_subscriber::fmt::layer().pretty())
        .init();
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        println!("panic: {:?}", panic_info);
        tracing_panic::panic_hook(panic_info);
        prev_hook(panic_info);
    }));

    let web_addr = SocketAddr::from((
        Ipv4Addr::UNSPECIFIED,
        std::env::var("WEB_PORT")
            .ok()
            .map(|n| n.parse().ok())
            .flatten()
            .unwrap_or(443),
    ));
    let grpc_addr = SocketAddr::from((
        Ipv4Addr::UNSPECIFIED,
        std::env::var("GRPC_PORT")
            .ok()
            .map(|n| n.parse().ok())
            .flatten()
            .unwrap_or(8080),
    ));

    info!("start connect db");
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or("postgresql://postgres:123456@127.0.0.1:5432/postgres".into());
    info!("database_url: {}", database_url);
    let dc = Database::connect(ConnectOptions::new(database_url))
        .await
        .expect("Fail to initialize database connection");

    info!("end connect db");

    let tracing_service = tracing_service::TracingService::new(dc.clone());

    tracing_service.init().await?;

    let (msg_sender, msg_receiver) = flume::unbounded::<RunMsg>();

    let handle_records_future = tokio::spawn({
        let tracing_service = tracing_service.clone();
        let id = tracing_service
            .query_last_record_id()
            .await
            .map_err(|err| Status::internal(format!("query_last_record_id error. {err}")))?
            .unwrap_or(0);
        async move {
            let mut running_apps = RunningApps::new(
                tracing_service,
                id,
                env::var("RECORD_MAX_BUF_COUNT")
                    .ok()
                    .map(|n| n.parse::<usize>().ok())
                    .flatten(),
            );
            running_apps.handle_records(msg_receiver).await
        }
    });
    let web_router = web_service::router(tracing_service.clone(), msg_sender.clone())
        .layer(CompressionLayer::new());
    let https_web_serve_future = tokio::spawn(
        axum_server::bind_rustls(
            web_addr,
            RustlsConfig::from_pem(
                include_bytes!("../cert/server.pem").into(),
                include_bytes!("../cert/server.key").into(),
            )
            .await?,
        )
        .serve(web_router.clone().into_make_service()),
    );

    let http_web_serve_future = tokio::spawn(
        axum::serve(
            TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 80))).await?,
            web_router.clone().into_make_service(),
        )
        .into_future(),
    );

    let grpc_serve_future = tokio::spawn(
        Server::builder()
            .accept_http1(false)
            .add_service(
                TracingServiceServer::new(TracingServiceImpl::new(tracing_service, msg_sender))
                    .accept_compressed(CompressionEncoding::Zstd)
                    .send_compressed(CompressionEncoding::Zstd),
            )
            .serve(grpc_addr),
    );
    info!("web serve: {:?}", web_addr);
    info!("grpc serve: {:?}", grpc_addr);

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
