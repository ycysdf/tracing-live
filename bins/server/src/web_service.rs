use crate::record::TracingKind;
use crate::running_app::RunMsg;
use crate::tracing_service::TracingTreeRecordDto;
use crate::tracing_service::{
   AppLatestInfoDto, AppNodeFilter, AppNodeRunDto, AppRunDto, BigInt, CursorInfo,
   TracingRecordDto, TracingRecordFieldFilter, TracingRecordFilter, TracingService,
   TracingSpanRunDto,
};
use crate::web_error::AppError;
use axum::extract::rejection::QueryRejection;
use axum::extract::{FromRequest, MatchedPath, Request, State};
use axum::handler::Handler;
use axum::http::{header, StatusCode, Uri};
use axum::response::sse::KeepAlive;
use axum::response::{sse, Html, IntoResponse};
use axum::response::{Response, Sse};
use axum::routing::get;
use axum::serve::IncomingStream;
use axum::{Json, Router};
use chrono::{DateTime, FixedOffset};
use futures_util::{stream, Stream};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tonic::codegen::tokio_stream::StreamExt;
use tower::Service;
use tower_http::classify::ServerErrorsFailureClass;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{error, warn, Span};
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
pub type Result<T, E = AppError> = core::result::Result<T, E>;
pub fn router(tracing_service: TracingService, msg_sender: flume::Sender<RunMsg>) -> Router {
    use crate::tracing_service::TracingRecordScene;

    #[derive(OpenApi)]
    #[openapi(components(schemas(
        TracingRecordFieldFilter,
        NodePageEvent,
        CursorInfo,
        TracingTreeRecordDto,
        TracingRecordScene
    )))]
    struct ApiDoc;

    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .nest("/api/v1/apps", apps::router(tracing_service.clone()))
        .nest("/api/v1/records", records::router(tracing_service.clone()))
        .nest("/api/v1/nodes", nodes::router(tracing_service.clone()))
        .layer(
            TraceLayer::new_for_http()
                // Create our own span for the request and include the matched path. The matched
                // path is useful for figuring out which handler the request was routed to.
                .make_span_with(|req: &Request| {
                    let method = req.method();
                    let uri = req.uri();

                    // axum automatically adds this extension.
                    let matched_path = req
                        .extensions()
                        .get::<MatchedPath>()
                        .map(|matched_path| matched_path.as_str());

                    tracing::info_span!("request", %method, %uri, matched_path)
                })
                .on_failure(
                    |_error: ServerErrorsFailureClass, _latency: Duration, _span: &Span| {
                        error!("{_error:?}")
                    },
                ),
        )
        .split_for_parts();

    let router = router
        .merge(web_router())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api.clone()))
        .route(
            "/node_page_subscribe",
            get(node_page_subscribe).with_state((tracing_service.clone(), msg_sender.clone())),
        )
        .route(
            "/records_subscribe",
            get(records_subscribe).with_state((tracing_service, msg_sender)),
        );

    router.layer(
        CorsLayer::new()
            .allow_origin(AllowOrigin::any())
            .allow_methods(AllowMethods::any())
            .allow_headers(AllowHeaders::any()),
    )
}
fn web_router() -> Router {
    use rust_embed::Embed;
    async fn index_handler() -> impl IntoResponse {
        static_handler("/index.html".parse::<Uri>().unwrap()).await
    }

    async fn static_handler(uri: Uri) -> impl IntoResponse {
        let mut path = uri.path().trim_start_matches('/').to_string();

        if path.starts_with("dist/") {
            path = path.replace("dist/", "");
        }

        StaticFile(path)
    }

    async fn not_found() -> Html<&'static str> {
        Html("<h1>404</h1><p>Not Found</p>")
    }

    #[derive(Embed)]
    #[folder = "../../web/dist/"]
    struct Asset;

    pub struct StaticFile<T>(pub T);

    impl<T> IntoResponse for StaticFile<T>
    where
        T: Into<String>,
    {
        fn into_response(self) -> Response {
            let path = self.0.into();

            match Asset::get(path.as_str()) {
                Some(content) => {
                    let mime = mime_guess::from_path(path).first_or_octet_stream();
                    ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
                }
                None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
            }
        }
    }
    Router::new()
        .route("/", get(index_handler))
        .route("/index.html", get(index_handler))
        .route("/*file", get(static_handler))
        .fallback_service(get(not_found))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
enum NodePageEvent {
    NewNode(AppNodeRunDto),
    NodeAppStart(AppNodeRunDto),
    NodeAppStop(AppNodeRunDto),
}

async fn node_page_subscribe(
    State((tracing_service, msg_sender)): State<(TracingService, flume::Sender<RunMsg>)>,
    // Query(input): Query<NodesPageInput>,
) -> Sse<impl Stream<Item = Result<sse::Event, Infallible>>> {
    let (sender, receiver) = flume::bounded(1024 * 4);

    let _ = msg_sender.send(RunMsg::AddRecordWatcher {
        sender,
        filter: TracingRecordFilter {
            // app_build_ids: input.app_build_ids,
            kinds: Some(smallvec![TracingKind::AppStart, TracingKind::AppStop]),
            ..Default::default()
        },
    });

    Sse::new(futures_util::stream::unfold(
        (receiver, tracing_service),
        move |(receiver, tracing_service)| async move {
            let item = receiver.recv_async().await.ok()?;
            let event = match item.record.kind {
                TracingKind::AppStart => {
                    let run_count = tracing_service
                        .app_node_run_count(item.record.app_id, item.record.node_id.as_str())
                        .await
                        .unwrap();
                    let run_dto = AppNodeRunDto {
                        app_run_id: item.record.app_run_id,
                        node_id: item.record.node_id.clone(),
                        data: item.record.fields.clone(),
                        creation_time: item.record.creation_time,
                        record_id: item.record.id,
                        stop_record_id: None,
                        start_time: item.record.record_time,
                        stop_time: None,
                        exception_end: false,
                        app_build_ids: smallvec![], // remove
                    };
                    if run_count > 1 {
                        NodePageEvent::NodeAppStart(run_dto)
                    } else {
                        NodePageEvent::NewNode(run_dto)
                    }
                }
                TracingKind::AppStop => {
                    let run_dto = AppNodeRunDto {
                        app_run_id: item.record.app_run_id,
                        node_id: item.record.node_id.clone(),
                        data: item.record.fields.clone(),
                        creation_time: item.record.creation_time, // remove
                        record_id: item.record.id,
                        stop_record_id: Some(item.record.id),
                        start_time: item.record.record_time, // remove
                        stop_time: Some(item.record.record_time),
                        exception_end: false,
                        app_build_ids: smallvec![], // remove
                    };
                    NodePageEvent::NodeAppStop(run_dto)
                }
                _ => return None,
            };
            Some((
                Ok(sse::Event::default().json_data(event).unwrap()),
                (receiver, tracing_service),
            ))
        },
    ))
    .keep_alive(KeepAlive::default())
}

async fn records_subscribe(
    State((tracing_service, msg_sender)): State<(TracingService, flume::Sender<RunMsg>)>,
    Query(filter): Query<TracingRecordFilter>,
) -> Sse<impl Stream<Item = Result<sse::Event, Infallible>>> {
    let mut records: Vec<_> = tracing_service
        .list_tree_records(filter.clone())
        .await
        .unwrap()
        .collect();
    records.sort_by_key(|n| n.record.id);
    let (sender, receiver) = flume::unbounded();
    // filter.is_tree = Some(false);
    msg_sender
        .send(RunMsg::AddRecordWatcher { sender, filter })
        .inspect_err(|err| {
            warn!("Failed to add record watcher: {}", err);
        })
        .unwrap();

    let stream = futures_util::stream::iter(
        records
            .into_iter()
            .map(|n| sse::Event::default().json_data(n).unwrap())
            .map(Ok),
    )
    .chain(futures_util::stream::unfold(
        receiver,
        move |receiver| async move {
            let item = receiver.recv_async().await;
            // println!("notify item: {item:#?}");
            item.inspect_err(|err| {
                warn!(?err, "recv error");
            })
            .ok()
            .map(|n| (Ok(sse::Event::default().json_data(n).unwrap()), receiver))
        },
    ));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

mod apps {
    use super::{Query, Result};
    use crate::tracing_service::{AppLatestInfoDto, AppRunDto, TracingService};
    use axum::extract::State;
    use axum::Json;
    use utoipa_axum::router::OpenApiRouter;
    use utoipa_axum::routes;

    pub(super) fn router(tracing_service: TracingService) -> OpenApiRouter {
        OpenApiRouter::new()
            .routes(routes!(list_apps))
            .with_state(tracing_service)
    }

    #[utoipa::path(
      get,
      path = "",
      responses(
            (status = 200, description = "List all todos successfully", body = [AppLatestInfoDto])
      )
   )]
    pub async fn list_apps(
        State(tracing_service): State<TracingService>,
    ) -> Result<Json<Vec<AppLatestInfoDto>>> {
        Ok(Json(tracing_service.list_latest_apps().await?))
    }
}

#[derive(Default, Clone, Debug, Deserialize, IntoParams, ToSchema)]
pub struct NodesPageInput {
    after_record_id: Option<BigInt>,
    app_build_ids: Option<SmallVec<[(Uuid, Option<String>); 2]>>,
}

mod nodes {
    use super::NodesPageInput;
    use crate::tracing_service::{
        AppLatestInfoDto, AppNodeFilter, AppNodeRunDto, TracingService, TracingTreeRecordDto,
    };
    use crate::web_service::Query;
    use axum::extract::State;
    use axum::Json;
    use chrono::{DateTime, FixedOffset, Local};
    use serde::Serialize;
    use std::iter::Iterator;
   use std::time::Duration;
   use utoipa::ToSchema;
    use utoipa_axum::router::OpenApiRouter;
    use utoipa_axum::routes;

    #[utoipa::path(
      get,
      path = "",
      params(AppNodeFilter),
      responses(
            (status = 200, body = [AppNodeRunDto])
      )
   )]
    pub async fn list_nodes(
        State(tracing_service): State<TracingService>,
        Query(filter): Query<AppNodeFilter>,
    ) -> crate::web_service::Result<Json<Vec<AppNodeRunDto>>> {
        Ok(Json(tracing_service.list_node(filter).await?))
    }

    pub(super) fn router(tracing_service: TracingService) -> OpenApiRouter {
        OpenApiRouter::new()
            .routes(routes!(nodes_page))
            .routes(routes!(list_nodes))
            .with_state(tracing_service)
    }

    #[derive(Serialize, Debug, ToSchema)]
    pub struct NodesPageDto {
        date: DateTime<FixedOffset>,
        apps: Vec<AppLatestInfoDto>,
        nodes: Vec<AppNodeRunDto>,
    }

    #[utoipa::path(
      get,
      path = "/page",
      params(NodesPageInput),
      responses(
            (status = 200, body = NodesPageDto)
      )
   )]
    pub async fn nodes_page(
        State(tracing_service): State<TracingService>,
        Query(input): Query<NodesPageInput>,
    ) -> crate::web_service::Result<Json<NodesPageDto>> {
        let main_app_id = input
            .app_build_ids
            .as_ref()
            .map(|n| n.iter().next())
            .flatten()
            .map(|n| n.0.clone());
        let nodes = tracing_service
            .list_node(AppNodeFilter {
                main_app_id,
                app_build_ids: input.app_build_ids,
                after_record_id: input.after_record_id,
            })
            .await?;
        let apps = tracing_service.list_latest_apps().await?;

        Ok(Json(NodesPageDto {
            date: Local::now().fixed_offset(),
            apps,
            nodes,
        }))
    }
}

mod records {
    use super::{Query, Result};
    use crate::tracing_service::{
        TracingRecordDto, TracingRecordFilter, TracingService, TracingTreeRecordDto,
    };
    use axum::extract::State;
    use axum::Json;
    use serde::Deserialize;
    use utoipa::IntoParams;
    use utoipa_axum::router::OpenApiRouter;
    use utoipa_axum::routes;

    #[utoipa::path(
      get,
      path = "",
      params(TracingRecordFilter),
      responses(
            (status = 200, description = "List all record successfully", body = [TracingRecordDto])
      )
   )]
    pub async fn list_records(
        State(tracing_service): State<TracingService>,
        Query(filter): Query<TracingRecordFilter>,
    ) -> Result<Json<Vec<TracingRecordDto>>> {
        Ok(Json(tracing_service.list_records(filter).await?.collect()))
    }

    #[utoipa::path(
      get,
      path = "/tree",
      params(TracingRecordFilter),
      responses(
            (status = 200, description = "List all record successfully", body = [TracingTreeRecordDto])
      )
   )]
    pub async fn list_tree_records(
        State(tracing_service): State<TracingService>,
        Query(filter): Query<TracingRecordFilter>,
    ) -> Result<Json<Vec<TracingTreeRecordDto>>> {
        let mut r: Vec<_> = tracing_service.list_tree_records(filter).await?.collect();
        r.sort_by_key(|n| n.record.id);
        Ok(Json(r))
    }

    pub(super) fn router(tracing_service: TracingService) -> OpenApiRouter {
        OpenApiRouter::new()
            .routes(routes!(list_records))
            .routes(routes!(list_tree_records))
            .with_state(tracing_service)
    }
}

struct Query<T>(T);

#[axum::async_trait]
impl<S, T> FromRequest<S> for Query<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    type Rejection = String;

    fn from_request<'life0, 'async_trait>(
        req: Request,
        _state: &'life0 S,
    ) -> Pin<
        Box<dyn Future<Output = std::result::Result<Self, Self::Rejection>> + Send + 'async_trait>,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let Some(query) = req.uri().query() else {
                return Ok(Self(Default::default()));
            };
            Ok(Self(
                serde_qs::Config::new(5, false)
                    .deserialize_str(query)
                    .map_err(|e| {
                        tracing::error!("Failed to parse query: {}", e);
                        format!("{e:?}")
                    })?,
            ))
        })
    }
}
