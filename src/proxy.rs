use hyper::{Request, Response, StatusCode, Uri};
use hyper::body::{Incoming, Bytes};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use crate::logging::{RequestLog, log_request};
use crate::routing::BackendService;
use uuid::Uuid;
use std::time::Instant;
use tracing::{info, error, instrument, Level};

// 프록시 요청을 위한 불변 설정 구조체
#[derive(Clone)]
pub struct ProxyConfig {
    client: legacy::Client<HttpConnector, hyper::body::Incoming>,
}

impl ProxyConfig {
    pub fn new() -> Self {
        let connector = HttpConnector::new();
        let client = legacy::Client::builder(TokioExecutor::new())
            .build::<_, hyper::body::Incoming>(connector);
        
        Self { client }
    }
}

// 순수 함수들: 입력을 받아서 새로운 값을 반환
#[instrument(skip(config, backend))]
pub async fn proxy_request(
    config: &ProxyConfig,
    backend: &BackendService,
    req: Request<Incoming>,
) -> Response<Full<Bytes>> {
    let request_id = Uuid::new_v4().to_string();
    let _span = tracing::span!(Level::INFO, "request", request_id = %request_id);
    let _enter = _span.enter();
    let start_time = Instant::now();
    let mut log = RequestLog::new(request_id);
    log.with_request(&req);

    let address = backend.get_next_address();
    log.with_backend(address);
    
    info!(backend = %address, "Proxying request to backend");

    let response = match build_proxied_request(address, req) {
        Ok(proxied_req) => {
            match config.client.request(proxied_req).await {
                Ok(res) => {
                    let status = res.status();
                    log.with_response(status);
                    
                    let (parts, body) = res.into_parts();
                    match body.collect().await {
                        Ok(collected) => {
                            let bytes = collected.to_bytes();
                            info!(bytes_size = bytes.len(), "Response body collected");
                            Response::from_parts(parts, Full::new(bytes))
                        }
                        Err(e) => {
                            log.with_error(&e);
                            build_error_response(
                                StatusCode::BAD_GATEWAY,
                                format!("Failed to collect response body: {}", e)
                            )
                        }
                    }
                }
                Err(e) => {
                    log.with_error(&e);
                    build_error_response(
                        StatusCode::BAD_GATEWAY,
                        format!("Backend request failed: {}", e)
                    )
                }
            }
        }
        Err(e) => {
            log.with_error(&e);
            build_error_response(
                StatusCode::BAD_REQUEST,
                format!("Failed to build request: {}", e)
            )
        }
    };

    log.duration_ms = start_time.elapsed().as_millis() as u64;
    log_request(&log);

    response
}

fn build_proxied_request(
    address: std::net::SocketAddr,
    req: Request<Incoming>,
) -> Result<Request<Incoming>, hyper::http::Error> {
    let uri: Uri = format!("http://{}{}", address, req.uri().path())
        .parse()
        .unwrap();

    let (parts, body) = req.into_parts();
    Request::builder()
        .method(parts.method)
        .uri(uri)
        .body(body)
}

fn build_error_response(status: StatusCode, message: String) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::from(message)))
        .unwrap()
}

#[derive(Debug)]
pub enum ProxyError {
    /// 백엔드 요청 실패
    BackendRequestFailed {
        backend: String,
        error: String,
    },
    /// 응답 처리 실패
    ResponseError {
        backend: String,
        error: String,
    },
    /// 요청 빌드 실패
    RequestBuildError {
        reason: String,
    },
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyError::BackendRequestFailed { backend, error } => 
                write!(f, "백엔드 {} 요청 실패: {}", backend, error),
            ProxyError::ResponseError { backend, error } => 
                write!(f, "백엔드 {} 응답 처리 실패: {}", backend, error),
            ProxyError::RequestBuildError { reason } => 
                write!(f, "요청 빌드 실패: {}", reason),
        }
    }
} 