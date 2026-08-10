use std::sync::Arc;

use axum::{
    Json, Router,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{MethodRouter, get, post},
    extract::{Extension, State},
};
use serde::{Deserialize, Serialize};

use axum::body::Body;
use axum::extract::{MatchedPath, Request};
use axum::http::{Method, header};
// use axum::http::Request;
use axum::{middleware::Next, response::Response};
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, MakeSpan, TraceLayer};
use tracing::{Level, Span, info_span};
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

use crate::ip_extract::OriginatorIp;

async fn trace_layer(Extension(ip): Extension<OriginatorIp>, request: Request, next: Next) -> Response {
    let method: Method = request.method().clone();
    let path: String = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or(request.uri().path())
        .to_string();

    // Headers
    let content_type: String = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("_")
        .to_string();

    let accept: String = request
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("_")
        .to_string();

    let user_agent: String = request
        .headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("_")
        .to_string();

    let start: std::time::Instant = std::time::Instant::now();
    let response: Response = next.run(request).await;
    let latency: u128 = start.elapsed().as_millis();

    tracing::info!(
        ip = %ip.ip(),
        method = %method,
        path = %path,
        content_type = %content_type,
        accept = %accept,
        user_agent = %user_agent,
        status = response.status().as_u16(),
        latency_ms = latency,
        "http request",
    );

    response
}

fn make_trace_layer() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().include_headers(false).level(Level::INFO))
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(tower_http::LatencyUnit::Millis),
        )
}

async fn default_trace_layer(request: Request, next: Next) -> Response {
    let path: String = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or(request.uri().path())
        .to_string();

    tracing::info!(path = %path, "matched route");
    next.run(request).await
}

pub fn layer(router: Router) -> Router {
    let host_router: Router = router.route_layer(middleware::from_fn(trace_layer));
    host_router
}

#[derive(Clone)]
pub struct CustomMakeSpan;

impl<B> MakeSpan<B> for CustomMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let path = request.uri().path();
        let method = request.method().as_str();

        info_span!(
            "http_request",
            %method,
            %path,
            request_id = tracing::field::Empty,
        )
    }
}
