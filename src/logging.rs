use std::fs;
use std::path::Path;
use tracing::{info, warn, error, Level};
use tracing_subscriber::{fmt, EnvFilter};
use tracing_appender::rolling::Rotation;
use crate::settings::LogSettings;
use crate::settings::logging::{LogFormat, LogOutput};

fn ensure_log_directory(path: &str) -> std::io::Result<()> {
    let log_dir = Path::new(path);
    if !log_dir.exists() {
        fs::create_dir_all(log_dir)?;
    }
    Ok(())
}

pub fn init_logging(settings: &LogSettings) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::new("")
        .add_directive(settings.level.into())
        .add_directive("bollard=warn".parse()?)
        .add_directive("hyper=warn".parse()?);

    let subscriber = fmt::Subscriber::builder()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .with_ansi(matches!(settings.output, LogOutput::Stdout))
        .with_env_filter(env_filter);

    // 출력 대상 설정
    match &settings.output {
        LogOutput::Stdout => {
            match settings.format {
                LogFormat::Json => subscriber.json().init(),
                LogFormat::Text => subscriber.init(),
            }
        }
        LogOutput::File(path) => {
            ensure_log_directory("logs")?;
            let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
                .rotation(Rotation::NEVER)
                .filename_prefix(path)
                .build("logs")?;

            match settings.format {
                LogFormat::Json => subscriber.json().with_writer(file_appender).init(),
                LogFormat::Text => subscriber.with_writer(file_appender).init(),
            }
        }
    }

    info!("로깅 초기화 완료: format={:?}, level={:?}, output={:?}", 
        settings.format, settings.level, settings.output);

    Ok(())
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
            if let Ok(host_str) = host.to_str() {
                self.host = host_str.to_string();
            } else {
                warn!(
                    request_id = %self.request_id,
                    "Invalid UTF-8 in Host header"
                );
            }
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

    // 간소화된 로그 포맷
    match level {
        Level::ERROR => error!(
            "{} {} {} -> {} ({}ms) [{}]{}",
            log.method,
            log.host,
            log.path,
            log.status_code,
            log.duration_ms,
            log.backend_address.as_deref().unwrap_or("-"),
            log.error.as_ref().map(|e| format!(" error: {}", e)).unwrap_or_default()
        ),
        Level::WARN => warn!(
            "{} {} {} -> {} ({}ms) [{}]",
            log.method,
            log.host,
            log.path,
            log.status_code,
            log.duration_ms,
            log.backend_address.as_deref().unwrap_or("-")
        ),
        _ => info!(
            "{} {} {} -> {} ({}ms) [{}]",
            log.method,
            log.host,
            log.path,
            log.status_code,
            log.duration_ms,
            log.backend_address.as_deref().unwrap_or("-")
        ),
    }
} 