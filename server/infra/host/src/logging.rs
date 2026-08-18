use axum::{
    Router,
    body::Body,
    extract::{Extension, MatchedPath, Request},
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::Response,
};
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, MakeSpan, TraceLayer};
use tracing::{debug, error, info, info_span, trace, warn, Level, Span};

use crate::ip_extract::OriginatorIp;

async fn trace_layer(Extension(ip): Extension<OriginatorIp>, request: Request, next: Next) -> Response {
    let method: Method = request.method().clone();
    let path: String = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or(request.uri().path())
        .to_owned();
    let content_type: String = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("_")
        .to_owned();
    let accept: String = request
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("_")
        .to_owned();
    let user_agent: String = request
        .headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("_")
        .to_owned();

    trace!(
        ip = %ip.ip(),
        method = %method,
        path = %path,
        content_type = %content_type,
        accept = %accept,
        user_agent = %user_agent,
        "HTTP request accepted by host trace layer"
    );
    let started_at: std::time::Instant = std::time::Instant::now();
    let response: Response = next.run(request).await;
    let latency_ms: u128 = started_at.elapsed().as_millis();
    let status: StatusCode = response.status();

    info!(
        ip = %ip.ip(),
        method = %method,
        path = %path,
        content_type = %content_type,
        accept = %accept,
        user_agent = %user_agent,
        status = status.as_u16(),
        latency_ms,
        "HTTP request completed"
    );
    if status.is_server_error() {
        error!(
            ip = %ip.ip(),
            method = %method,
            path = %path,
            status = status.as_u16(),
            latency_ms,
            "HTTP request completed with server error"
        );
    } else if status.is_client_error() {
        warn!(
            ip = %ip.ip(),
            method = %method,
            path = %path,
            status = status.as_u16(),
            latency_ms,
            "HTTP request completed with client error"
        );
    } else {
        debug!(
            ip = %ip.ip(),
            method = %method,
            path = %path,
            status = status.as_u16(),
            latency_ms,
            "HTTP request response classification completed"
        );
    }

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
        .to_owned();
    trace!(path = %path, "Host matched route before handler execution");
    next.run(request).await
}

pub fn layer(router: Router) -> Router {
    info!("Applying host HTTP trace layer");
    let host_router: Router = router.route_layer(middleware::from_fn(trace_layer));
    host_router
}

#[derive(Clone)]
pub struct CustomMakeSpan;

impl<B> MakeSpan<B> for CustomMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let path: &str = request.uri().path();
        let method: &str = request.method().as_str();
        info_span!(
            "http_request",
            %method,
            %path,
            request_id = tracing::field::Empty,
        )
    }
}
