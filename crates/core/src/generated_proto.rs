
// This file is @generated by prost-build.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PosInfo {
    #[prost(string, tag = "1")]
    pub module_path: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub file_line: ::prost::alloc::string::String,
}
#[derive(Hash, Eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SpanInfo {
    #[prost(uint64, tag = "1")]
    pub t_id: u64,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub file_line: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FieldValue {
    #[prost(oneof = "field_value::Variant", tags = "1, 2, 3, 4, 5, 6")]
    pub variant: ::core::option::Option<field_value::Variant>,
}
/// Nested message and enum types in `FieldValue`.
pub mod field_value {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Variant {
        #[prost(double, tag = "1")]
        F64(f64),
        #[prost(sint64, tag = "2")]
        I64(i64),
        #[prost(uint64, tag = "3")]
        U64(u64),
        #[prost(bool, tag = "4")]
        Bool(bool),
        #[prost(string, tag = "5")]
        String(::prost::alloc::string::String),
        #[prost(bool, tag = "6")]
        Null(bool),
    }
}
#[derive(Clone, Copy, PartialEq, ::prost::Message)]
pub struct TracingRecordResult {
}
#[derive(Clone, Copy, PartialEq, ::prost::Message)]
pub struct Ping {}
#[derive(Clone, Copy, PartialEq, ::prost::Message)]
pub struct PingResult {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SpanCreate {
    #[prost(int64, tag = "1")]
    pub record_time: i64,
    #[prost(message, optional, tag = "2")]
    pub span_info: ::core::option::Option<SpanInfo>,
    #[prost(message, optional, tag = "3")]
    pub pos_info: ::core::option::Option<PosInfo>,
    #[prost(message, optional, tag = "4")]
    pub parent_span_info: ::core::option::Option<SpanInfo>,
    #[prost(map = "string, message", tag = "5")]
    pub fields: ::std::collections::HashMap<::prost::alloc::string::String, FieldValue>,
    #[prost(string, tag = "6")]
    pub target: ::prost::alloc::string::String,
    #[prost(enumeration = "Level", tag = "7")]
    pub level: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SpanEnter {
    #[prost(int64, tag = "1")]
    pub record_time: i64,
    #[prost(message, optional, tag = "2")]
    pub span_info: ::core::option::Option<SpanInfo>,
    #[prost(message, optional, tag = "3")]
    pub pos_info: ::core::option::Option<PosInfo>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SpanLeave {
    #[prost(int64, tag = "1")]
    pub record_time: i64,
    #[prost(message, optional, tag = "2")]
    pub span_info: ::core::option::Option<SpanInfo>,
    #[prost(message, optional, tag = "3")]
    pub pos_info: ::core::option::Option<PosInfo>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SpanClose {
    #[prost(int64, tag = "1")]
    pub record_time: i64,
    #[prost(message, optional, tag = "2")]
    pub span_info: ::core::option::Option<SpanInfo>,
    #[prost(message, optional, tag = "3")]
    pub pos_info: ::core::option::Option<PosInfo>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SpanRecordField {
    #[prost(int64, tag = "1")]
    pub record_time: i64,
    #[prost(message, optional, tag = "2")]
    pub span_info: ::core::option::Option<SpanInfo>,
    #[prost(message, optional, tag = "3")]
    pub pos_info: ::core::option::Option<PosInfo>,
    #[prost(map = "string, message", tag = "4")]
    pub fields: ::std::collections::HashMap<::prost::alloc::string::String, FieldValue>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Event {
    #[prost(int64, tag = "1")]
    pub record_time: i64,
    #[prost(message, optional, tag = "2")]
    pub span_info: ::core::option::Option<SpanInfo>,
    #[prost(message, optional, tag = "3")]
    pub pos_info: ::core::option::Option<PosInfo>,
    #[prost(map = "string, message", tag = "4")]
    pub fields: ::std::collections::HashMap<::prost::alloc::string::String, FieldValue>,
    #[prost(string, tag = "5")]
    pub target: ::prost::alloc::string::String,
    #[prost(enumeration = "Level", tag = "6")]
    pub level: i32,
    #[prost(string, tag = "7")]
    pub message: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AppStart {
    #[prost(int64, tag = "1")]
    pub record_time: i64,
    #[prost(bytes = "vec", tag = "2")]
    pub id: ::prost::alloc::vec::Vec<u8>,
    #[prost(string, tag = "3")]
    pub node_id: ::prost::alloc::string::String,
    #[prost(bytes = "vec", tag = "4")]
    pub run_id: ::prost::alloc::vec::Vec<u8>,
    #[prost(string, tag = "5")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "6")]
    pub version: ::prost::alloc::string::String,
    #[prost(map = "string, message", tag = "8")]
    pub data: ::std::collections::HashMap<::prost::alloc::string::String, FieldValue>,
    #[prost(double, tag = "9")]
    pub rtt: f64,
    #[prost(bool, tag = "10")]
    pub reconnect: bool,
}
#[derive(Clone, Copy, PartialEq, ::prost::Message)]
pub struct AppStop {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RecordParam {
    #[prost(int64, tag = "30")]
    pub send_time: i64,
    #[prost(uint64, tag = "31")]
    pub record_index: u64,
    #[prost(oneof = "record_param::Variant", tags = "1, 2, 3, 4, 5, 6, 7, 8")]
    pub variant: ::core::option::Option<record_param::Variant>,
}
/// Nested message and enum types in `RecordParam`.
pub mod record_param {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Variant {
        #[prost(message, tag = "1")]
        AppStart(super::AppStart),
        #[prost(message, tag = "2")]
        SpanCreate(super::SpanCreate),
        #[prost(message, tag = "3")]
        SpanEnter(super::SpanEnter),
        #[prost(message, tag = "4")]
        SpanLeave(super::SpanLeave),
        #[prost(message, tag = "5")]
        SpanClose(super::SpanClose),
        #[prost(message, tag = "6")]
        SpanRecordField(super::SpanRecordField),
        #[prost(message, tag = "7")]
        Event(super::Event),
        #[prost(message, tag = "8")]
        AppStop(super::AppStop),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}
impl Level {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRACE" => Some(Self::Trace),
            "DEBUG" => Some(Self::Debug),
            "INFO" => Some(Self::Info),
            "WARN" => Some(Self::Warn),
            "ERROR" => Some(Self::Error),
            _ => None,
        }
    }
}
/// Generated client implementations.
#[cfg(feature = "client")]
pub mod tracing_service_client {
    #![allow(
        unused_variables,
        dead_code,
        missing_docs,
        clippy::wildcard_imports,
        clippy::let_unit_value,
    )]
    use tonic::codegen::*;
    use tonic::codegen::http::Uri;
    #[derive(Debug, Clone)]
    pub struct TracingServiceClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl TracingServiceClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> TracingServiceClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + std::marker::Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + std::marker::Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_origin(inner: T, origin: Uri) -> Self {
            let inner = tonic::client::Grpc::with_origin(inner, origin);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> TracingServiceClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + std::marker::Send + std::marker::Sync,
        {
            TracingServiceClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with the given encoding.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.send_compressed(encoding);
            self
        }
        /// Enable decompressing responses.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.accept_compressed(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_decoding_message_size(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_encoding_message_size(limit);
            self
        }
        pub async fn app_run(
            &mut self,
            request: impl tonic::IntoStreamingRequest<Message = super::RecordParam>,
        ) -> std::result::Result<
            tonic::Response<super::TracingRecordResult>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        alloc::format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/tracing.TracingService/app_run",
            );
            let mut req = request.into_streaming_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("tracing.TracingService", "app_run"));
            self.inner.client_streaming(req, path, codec).await
        }
        pub async fn ping(
            &mut self,
            request: impl tonic::IntoRequest<super::Ping>,
        ) -> std::result::Result<tonic::Response<super::PingResult>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        alloc::format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/tracing.TracingService/ping",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("tracing.TracingService", "ping"));
            self.inner.unary(req, path, codec).await
        }
    }
}
/// Generated server implementations.
#[cfg(feature = "server")]
pub mod tracing_service_server {
    #![allow(
        unused_variables,
        dead_code,
        missing_docs,
        clippy::wildcard_imports,
        clippy::let_unit_value,
    )]
    use tonic::codegen::*;
    use std::boxed::Box;
    /// Generated trait containing gRPC methods that should be implemented for use with TracingServiceServer.
    #[async_trait]
    pub trait TracingService: std::marker::Send + std::marker::Sync + 'static {
        async fn app_run(
            &self,
            request: tonic::Request<tonic::Streaming<super::RecordParam>>,
        ) -> std::result::Result<
            tonic::Response<super::TracingRecordResult>,
            tonic::Status,
        >;
        async fn ping(
            &self,
            request: tonic::Request<super::Ping>,
        ) -> std::result::Result<tonic::Response<super::PingResult>, tonic::Status>;
    }
    #[derive(Debug)]
    pub struct TracingServiceServer<T> {
        inner: Arc<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    impl<T> TracingServiceServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
                max_decoding_message_size: None,
                max_encoding_message_size: None,
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
        /// Enable decompressing requests with the given encoding.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.accept_compression_encodings.enable(encoding);
            self
        }
        /// Compress responses with the given encoding, if the client supports it.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.send_compression_encodings.enable(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.max_decoding_message_size = Some(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.max_encoding_message_size = Some(limit);
            self
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for TracingServiceServer<T>
    where
        T: TracingService,
        B: Body + std::marker::Send + 'static,
        B::Error: Into<StdError> + std::marker::Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            match req.uri().path() {
                "/tracing.TracingService/app_run" => {
                    #[allow(non_camel_case_types)]
                    struct app_runSvc<T: TracingService>(pub Arc<T>);
                    impl<
                        T: TracingService,
                    > tonic::server::ClientStreamingService<super::RecordParam>
                    for app_runSvc<T> {
                        type Response = super::TracingRecordResult;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<tonic::Streaming<super::RecordParam>>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as TracingService>::app_run(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = app_runSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.client_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/tracing.TracingService/ping" => {
                    #[allow(non_camel_case_types)]
                    struct pingSvc<T: TracingService>(pub Arc<T>);
                    impl<T: TracingService> tonic::server::UnaryService<super::Ping>
                    for pingSvc<T> {
                        type Response = super::PingResult;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::Ping>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as TracingService>::ping(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = pingSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        let mut response = http::Response::new(empty_body());
                        let headers = response.headers_mut();
                        headers
                            .insert(
                                tonic::Status::GRPC_STATUS,
                                (tonic::Code::Unimplemented as i32).into(),
                            );
                        headers
                            .insert(
                                http::header::CONTENT_TYPE,
                                tonic::metadata::GRPC_CONTENT_TYPE,
                            );
                        Ok(response)
                    })
                }
            }
        }
    }
    impl<T> Clone for TracingServiceServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
                max_decoding_message_size: self.max_decoding_message_size,
                max_encoding_message_size: self.max_encoding_message_size,
            }
        }
    }
    /// Generated gRPC service name
    pub const SERVICE_NAME: &str = "tracing.TracingService";
    impl<T> tonic::server::NamedService for TracingServiceServer<T> {
        const NAME: &'static str = SERVICE_NAME;
    }
}
