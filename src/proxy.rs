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

//프록시 요청 핸들러
#[instrument(skip(config, backend))]
pub async fn proxy_request(
    config: &ProxyConfig,
    backend: &BackendService,
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<http_body_util::Full<hyper::body::Bytes>>, ProxyError> {
    // --- 부수 효과가 포함된 임페리티브 처리 영역 ---
    // UUID 생성 및 트레이싱 설정
    let request_id = Uuid::new_v4().to_string();
    let _span = tracing::span!(Level::INFO, "request", request_id = %request_id);
    let _enter = _span.enter();
    let start_time = std::time::Instant::now();
    
    // 요청 정보 로깅
    let mut log = RequestLog::new(request_id);
    log.with_request(&req);

    // 백엔드 주소 획득
    let address = backend.get_next_address().map_err(|e| {
        let err = ProxyError::BackendRequestFailed {
            backend: "unknown".to_string(),
            error: e.to_string(),
        };
        error!(error = %err, "백엔드 주소 획득 실패");
        err
    })?;
    log.with_backend(address);
    info!(backend = %address, "백엔드로 요청 프록시");

    // --- 순수 함수 호출 영역 ---
    // 원래 요청을 분리하여 순수 함수로 요청 빌드
    let (parts, body) = req.into_parts();
    let proxied_req = pure_build_proxied_request(address, parts.method, parts.uri.path(), body)
        .map_err(|e| {
            let err = ProxyError::RequestBuildError { reason: e };
            error!(error = %err, "요청 빌드 실패");
            err
        })?;

    // --- 부수 효과: 네트워크 요청 및 응답 처리 ---
    let response = config.client.request(proxied_req).await.map_err(|e| {
        let err = ProxyError::BackendRequestFailed {
            backend: address.to_string(),
            error: e.to_string(),
        };
        error!(error = %err, "백엔드 요청 실패");
        err
    })?;

    let status = response.status();
    log.with_response(status);

    let (parts, body) = response.into_parts();
    let collected = body.collect().await.map_err(|e| {
        let err = ProxyError::ResponseError {
            backend: address.to_string(),
            error: e.to_string(),
        };
        error!(error = %err, "응답 처리 실패");
        err
    })?;
    let bytes = collected.to_bytes();
    info!(bytes_size = bytes.len(), "응답 바디 수집 완료");

    log.duration_ms = start_time.elapsed().as_millis() as u64;
    log_request(&log);

    Ok(hyper::Response::from_parts(parts, http_body_util::Full::new(bytes)))
}

// 에러 응답 생성 헬퍼 함수
pub fn error_response(error: &ProxyError) -> Response<Full<Bytes>> {
    let (status, message) = match error {
        ProxyError::RequestBuildError { .. } => 
            (StatusCode::BAD_REQUEST, error.to_string()),
        ProxyError::BackendRequestFailed { .. } | 
        ProxyError::ResponseError { .. } => 
            (StatusCode::BAD_GATEWAY, error.to_string()),
    };

    Response::builder()
        .status(status)
        .body(Full::new(Bytes::from(message)))
        .unwrap_or_else(|e| {
            error!(error = %e, "에러 응답 생성 실패");
            Response::new(Full::new(Bytes::from("Internal Server Error")))
        })
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

impl std::error::Error for ProxyError {}

// 순수 함수로 분리한 요청 빌드 함수
pub fn pure_build_proxied_request(
    address: std::net::SocketAddr,
    method: hyper::Method,
    path: &str,
    body: hyper::body::Incoming,
) -> Result<hyper::Request<hyper::body::Incoming>, String> {
    let uri: hyper::Uri = format!("http://{}{}", address, path)
        .parse()
        .map_err(|e| format!("URI 파싱 실패: {}", e))?;
    hyper::Request::builder()
        .method(method)
        .uri(uri)
        .body(body)
        .map_err(|e| format!("요청 빌드 실패: {}", e))
}

