//! `network::server` — route specs and OpenAPI generation.
//!
//! Auto-generates REST route descriptors and an OpenAPI 3.0
//! specification from an `ObjectRegistry`. Each registered entity
//! type produces a standard CRUD route set; edge types produce
//! relationship routes. The output is a data structure — actual
//! HTTP wiring (Axum, Actix, etc.) lives in host crates.

pub mod openapi;
pub mod route_spec;

pub use openapi::{generate_openapi, OpenApiSpec};
pub use route_spec::{generate_route_specs, HttpMethod, RouteParam, RouteSpec};
