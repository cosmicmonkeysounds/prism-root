//! gRPC transport — a hand-rolled tonic 0.12 service that wraps
//! [`DaemonKernel::invoke`] in a single unary RPC.
//!
//! ```text
//! service DaemonService {
//!     rpc Invoke (InvokeRequest) returns (InvokeResponse);
//! }
//! message InvokeRequest  { string command = 1; bytes payload_json = 2; }
//! message InvokeResponse { bytes  result_json = 1; }
//! ```
//!
//! The wire format is JSON-bytes-inside-protobuf-bytes: every payload
//! that the kernel speaks is JSON, so we just shove the bytes through
//! the protobuf field instead of inventing a parallel proto schema for
//! every command. This means polyglot clients only need a tiny `.proto`
//! file (the one above) plus a JSON encoder for whichever language they
//! use, and the daemon's full command surface is reachable from any
//! tonic-, grpc-go-, grpc-node-, etc.-style client.
//!
//! ### No tonic-build / no protoc
//!
//! We deliberately hand-write both the prost messages and the tonic
//! `Service` glue. The alternative — running `tonic-build` from a
//! `build.rs` — would force every dev environment to install `protoc`
//! and turn the `transport-grpc` feature into a system-deps minefield.
//! For one unary RPC the hand-rolled version is ~150 lines and stays
//! pinned to the public `tonic::server` API.

use crate::kernel::DaemonKernel;
use std::sync::Arc;
use std::task::{Context, Poll};

use tonic::codegen::{http, Body, BoxFuture, Service, StdError};
use tonic::server::NamedService;

// ── Prost messages ─────────────────────────────────────────────────────

/// Wire-level request: which command to invoke + the JSON payload bytes.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InvokeRequest {
    /// Fully-qualified command name, e.g. `crdt.write`, `vfs.put`.
    #[prost(string, tag = "1")]
    pub command: ::prost::alloc::string::String,
    /// UTF-8 JSON bytes — passed straight through to the kernel handler.
    #[prost(bytes = "vec", tag = "2")]
    pub payload_json: ::prost::alloc::vec::Vec<u8>,
}

/// Wire-level response: the JSON bytes the handler returned, untouched.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InvokeResponse {
    #[prost(bytes = "vec", tag = "1")]
    pub result_json: ::prost::alloc::vec::Vec<u8>,
}

// ── Service trait ──────────────────────────────────────────────────────

/// Backing trait every tonic service impl must satisfy. We avoid
/// `#[async_trait]` (and the proc-macro dep that goes with it) by
/// returning [`BoxFuture`] directly — the same shape tonic-build emits
/// internally.
pub trait DaemonGrpcService: Send + Sync + 'static {
    fn invoke(
        &self,
        request: tonic::Request<InvokeRequest>,
    ) -> BoxFuture<tonic::Response<InvokeResponse>, tonic::Status>;
}

// ── Server wrapper ─────────────────────────────────────────────────────

/// Tonic-shaped server that you hand to
/// `tonic::transport::Server::builder().add_service(...)`.
#[derive(Debug)]
pub struct DaemonServiceServer<T: DaemonGrpcService> {
    inner: Arc<T>,
    max_decoding_message_size: Option<usize>,
    max_encoding_message_size: Option<usize>,
}

impl<T: DaemonGrpcService> DaemonServiceServer<T> {
    pub fn new(inner: T) -> Self {
        Self::from_arc(Arc::new(inner))
    }

    pub fn from_arc(inner: Arc<T>) -> Self {
        Self {
            inner,
            max_decoding_message_size: None,
            max_encoding_message_size: None,
        }
    }

    pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
        self.max_decoding_message_size = Some(limit);
        self
    }

    pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
        self.max_encoding_message_size = Some(limit);
        self
    }
}

impl<T: DaemonGrpcService> Clone for DaemonServiceServer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            max_decoding_message_size: self.max_decoding_message_size,
            max_encoding_message_size: self.max_encoding_message_size,
        }
    }
}

/// Fully-qualified service name. Matches what `protoc` would emit for
/// `package prism.daemon; service DaemonService { … }`.
pub const SERVICE_NAME: &str = "prism.daemon.DaemonService";

impl<T: DaemonGrpcService> NamedService for DaemonServiceServer<T> {
    const NAME: &'static str = SERVICE_NAME;
}

impl<T, B> Service<http::Request<B>> for DaemonServiceServer<T>
where
    T: DaemonGrpcService,
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::BoxBody>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        match req.uri().path() {
            "/prism.daemon.DaemonService/Invoke" => {
                struct InvokeSvc<T: DaemonGrpcService>(pub Arc<T>);

                impl<T: DaemonGrpcService> tonic::server::UnaryService<InvokeRequest> for InvokeSvc<T> {
                    type Response = InvokeResponse;
                    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

                    fn call(&mut self, request: tonic::Request<InvokeRequest>) -> Self::Future {
                        let inner = Arc::clone(&self.0);
                        Box::pin(async move { inner.invoke(request).await })
                    }
                }

                let max_decoding_message_size = self.max_decoding_message_size;
                let max_encoding_message_size = self.max_encoding_message_size;
                let inner = Arc::clone(&self.inner);

                let fut = async move {
                    let method = InvokeSvc(inner);
                    // ProstCodec<Encode, Decode> — Encode first, Decode second.
                    let codec =
                        tonic::codec::ProstCodec::<InvokeResponse, InvokeRequest>::default();
                    let mut grpc = tonic::server::Grpc::new(codec).apply_max_message_size_config(
                        max_decoding_message_size,
                        max_encoding_message_size,
                    );
                    let res = grpc.unary(method, req).await;
                    Ok(res)
                };
                Box::pin(fut)
            }
            _ => Box::pin(async move {
                let mut response = http::Response::new(tonic::body::empty_body());
                let headers = response.headers_mut();
                headers.insert(
                    tonic::Status::GRPC_STATUS,
                    (tonic::Code::Unimplemented as i32).into(),
                );
                headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/grpc"),
                );
                Ok(response)
            }),
        }
    }
}

// ── Kernel-backed implementation ──────────────────────────────────────

/// The default [`DaemonGrpcService`] impl. Wraps a [`DaemonKernel`] and
/// hops the sync `invoke` onto the blocking pool so the tokio reactor
/// keeps spinning even if a handler does real work.
pub struct KernelGrpcService {
    kernel: DaemonKernel,
}

impl KernelGrpcService {
    pub fn new(kernel: DaemonKernel) -> Self {
        Self { kernel }
    }
}

impl DaemonGrpcService for KernelGrpcService {
    fn invoke(
        &self,
        request: tonic::Request<InvokeRequest>,
    ) -> BoxFuture<tonic::Response<InvokeResponse>, tonic::Status> {
        let kernel = self.kernel.clone();
        Box::pin(async move {
            let req = request.into_inner();
            let payload: serde_json::Value = if req.payload_json.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&req.payload_json).map_err(|e| {
                    tonic::Status::invalid_argument(format!("payload_json is not valid JSON: {e}"))
                })?
            };

            let cmd = req.command.clone();
            let result = tokio::task::spawn_blocking(move || kernel.invoke(&cmd, payload))
                .await
                .map_err(|e| tonic::Status::internal(format!("blocking pool join error: {e}")))?;

            let value = match result {
                Ok(v) => v,
                Err(err) => {
                    use crate::registry::CommandError;
                    return Err(match err {
                        CommandError::NotFound(name) => {
                            tonic::Status::not_found(format!("command not found: {name}"))
                        }
                        CommandError::AlreadyRegistered { command } => {
                            tonic::Status::already_exists(format!(
                                "command already registered: {command}"
                            ))
                        }
                        CommandError::Handler { command, message } => {
                            tonic::Status::internal(format!("{command}: {message}"))
                        }
                        CommandError::LockPoisoned => {
                            tonic::Status::internal("registry lock poisoned")
                        }
                    });
                }
            };

            let result_json = serde_json::to_vec(&value)
                .map_err(|e| tonic::Status::internal(format!("encode JSON: {e}")))?;
            Ok(tonic::Response::new(InvokeResponse { result_json }))
        })
    }
}

/// Convenience constructor: build a tonic server wired to a kernel in
/// one shot.
pub fn server_for_kernel(kernel: DaemonKernel) -> DaemonServiceServer<KernelGrpcService> {
    DaemonServiceServer::new(KernelGrpcService::new(kernel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;
    use crate::module::DaemonModule;
    use crate::registry::CommandError;
    use serde_json::json;

    struct EchoModule;
    impl DaemonModule for EchoModule {
        fn id(&self) -> &str {
            "echo"
        }
        fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
            builder
                .registry()
                .register("echo.ping", |payload| Ok(json!({ "echoed": payload })))?;
            builder.registry().register("echo.boom", |_| {
                Err(CommandError::handler("echo.boom", "kaboom"))
            })?;
            Ok(())
        }
    }

    fn build_kernel() -> DaemonKernel {
        DaemonBuilder::new()
            .with_module(EchoModule)
            .build()
            .unwrap()
    }

    #[test]
    fn server_implements_named_service_with_proto_path() {
        // Compile-time assertion that the const is what tonic-build would emit.
        assert_eq!(
            <DaemonServiceServer<KernelGrpcService> as NamedService>::NAME,
            "prism.daemon.DaemonService"
        );
    }

    #[test]
    fn server_for_kernel_constructs_a_clonable_service() {
        let svc = server_for_kernel(build_kernel());
        let _clone = svc.clone();
    }

    #[tokio::test]
    async fn invoke_roundtrips_json_payload_through_the_service_trait() {
        let svc = KernelGrpcService::new(build_kernel());
        let payload = serde_json::to_vec(&json!({ "hello": "world" })).unwrap();
        let req = tonic::Request::new(InvokeRequest {
            command: "echo.ping".into(),
            payload_json: payload,
        });
        let resp = svc.invoke(req).await.unwrap();
        let body = resp.into_inner();
        let value: serde_json::Value = serde_json::from_slice(&body.result_json).unwrap();
        assert_eq!(value, json!({ "echoed": { "hello": "world" } }));
    }

    #[tokio::test]
    async fn invoke_unknown_command_maps_to_grpc_not_found() {
        let svc = KernelGrpcService::new(build_kernel());
        let req = tonic::Request::new(InvokeRequest {
            command: "nope.nada".into(),
            payload_json: b"null".to_vec(),
        });
        let err = svc.invoke(req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::NotFound);
        assert!(err.message().contains("nope.nada"));
    }

    #[tokio::test]
    async fn invoke_handler_error_maps_to_grpc_internal() {
        let svc = KernelGrpcService::new(build_kernel());
        let req = tonic::Request::new(InvokeRequest {
            command: "echo.boom".into(),
            payload_json: b"{}".to_vec(),
        });
        let err = svc.invoke(req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::Internal);
        assert!(err.message().contains("kaboom"));
    }

    #[tokio::test]
    async fn invoke_invalid_json_payload_maps_to_invalid_argument() {
        let svc = KernelGrpcService::new(build_kernel());
        let req = tonic::Request::new(InvokeRequest {
            command: "echo.ping".into(),
            payload_json: b"not json".to_vec(),
        });
        let err = svc.invoke(req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn empty_payload_is_treated_as_null() {
        let svc = KernelGrpcService::new(build_kernel());
        let req = tonic::Request::new(InvokeRequest {
            command: "echo.ping".into(),
            payload_json: Vec::new(),
        });
        let resp = svc.invoke(req).await.unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&resp.into_inner().result_json).unwrap();
        assert_eq!(value, json!({ "echoed": null }));
    }
}
