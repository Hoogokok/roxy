use tracing::{info, warn, error, Level, span};
use tracing_subscriber::{fmt, EnvFilter};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive(Level::INFO.into())
            .add_directive("reverse_proxy_traefik=debug".parse().unwrap()))
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();
}

#[derive(Debug)]
pub struct RequestLog {
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub host: String,
    pub status_code: u16,
    pub duration_ms: u64,
    pub backend_address: Option<String>,
    pub error: Option<String>,
}

impl RequestLog {
    pub fn new(request_id: String) -> Self {
        info!(request_id = %request_id, "Creating new request log");
        Self {
            request_id,
            method: String::new(),
            path: String::new(),
            host: String::new(),
            status_code: 0,
            duration_ms: 0,
            backend_address: None,
            error: None,
        }
    }

    pub fn with_request<B>(&mut self, req: &hyper::Request<B>) {
        self.method = req.method().to_string();
        self.path = req.uri().path().to_string();
        if let Some(host) = req.headers().get(hyper::header::HOST) {
            self.host = host.to_str().unwrap_or_default().to_string();
        }

        info!(
            request_id = %self.request_id,
            method = %self.method,
            path = %self.path,
            host = %self.host,
            "Received request"
        );
    }

    pub fn with_response(&mut self, status: hyper::StatusCode) {
        self.status_code = status.as_u16();
        info!(
            request_id = %self.request_id,
            status = %self.status_code,
            "Response status set"
        );
    }

    pub fn with_backend(&mut self, addr: std::net::SocketAddr) {
        self.backend_address = Some(addr.to_string());
        info!(
            request_id = %self.request_id,
            backend = %addr,
            "Selected backend"
        );
    }

    pub fn with_error(&mut self, error: impl std::fmt::Display) {
        let error_msg = error.to_string();
        error!(
            request_id = %self.request_id,
            error = %error_msg,
            "Request error occurred"
        );
        self.error = Some(error_msg);
    }
}

pub fn log_request(log: &RequestLog) {
    let level = if log.error.is_some() {
        Level::ERROR
    } else if log.status_code >= 400 {
        Level::WARN
    } else {
        Level::INFO
    };

    let span = span!(
        level,
        "request",
        request_id = %log.request_id,
        method = %log.method,
        path = %log.path,
        host = %log.host,
        status = %log.status_code,
        duration_ms = %log.duration_ms
    );
    let _enter = span.enter();

    match level {
        Level::ERROR => error!(
            backend = ?log.backend_address,
            error = ?log.error,
            "Request failed"
        ),
        Level::WARN => warn!(
            backend = ?log.backend_address,
            "Request completed with warning"
        ),
        _ => info!(
            backend = ?log.backend_address,
            "Request completed successfully"
        ),
    }
} 