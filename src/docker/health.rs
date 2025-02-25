use std::time::SystemTime;
use async_trait::async_trait;
use hyper::{Method, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use http_body_util::Empty;
use bytes::Bytes;
use tokio::time::timeout;
use tracing::debug;
use tokio::net::TcpStream;
use std::fmt;

use crate::settings::docker::HealthCheckType;
use super::{DockerError, events_types::HealthStatus};

#[async_trait]
pub trait HealthChecker: Send + Sync {
    /// 헬스 체크 수행
    async fn check(&self) -> Result<HealthCheckResult, DockerError>;
}

#[derive(Debug)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    pub message: String,
    pub timestamp: SystemTime,
}

impl HealthCheckResult {
    fn healthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Healthy,
            message: message.into(),
            timestamp: SystemTime::now(),
        }
    }

    fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            message: message.into(),
            timestamp: SystemTime::now(),
        }
    }
}

/// HTTP 헬스 체커
pub struct HttpHealthChecker {
    addr: String,
    path: String,
    method: String,
    expected_status: u16,
    timeout_secs: u64,
}

impl HttpHealthChecker {
    pub fn new(addr: String, check_type: &HealthCheckType, timeout_secs: u64) -> Option<Self> {
        match check_type {
            HealthCheckType::Http { path, method, expected_status } => Some(Self {
                addr,
                path: path.clone(),
                method: method.clone(),
                expected_status: *expected_status,
                timeout_secs,
            }),
            _ => None,
        }
    }
}

#[async_trait]
impl HealthChecker for HttpHealthChecker {
    async fn check(&self) -> Result<HealthCheckResult, DockerError> {
        let url = format!("http://{}{}", self.addr, self.path);
        debug!("HTTP 헬스 체크 시작: {}", url);

        let client = Client::builder(TokioExecutor::new())
            .build::<_, Empty<Bytes>>(HttpConnector::new());
            
        let request = hyper::Request::builder()
            .method(Method::from_bytes(self.method.as_bytes()).map_err(|e| DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: format!("잘못된 HTTP 메서드: {}", e),
                context: None,
            })?)
            .uri(&url)
            .body(Empty::<Bytes>::new())
            .map_err(|e| DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: format!("요청 생성 실패: {}", e),
                context: None,
            })?;

        match timeout(std::time::Duration::from_secs(self.timeout_secs), client.request(request)).await {
            Ok(Ok(response)) => {
                let status = response.status();
                if status == StatusCode::from_u16(self.expected_status).unwrap() {
                    Ok(HealthCheckResult::healthy(format!("HTTP {} 응답 성공", status)))
                } else {
                    Ok(HealthCheckResult::unhealthy(format!("예상 상태 코드 불일치: {} (expected {})", 
                        status, self.expected_status)))
                }
            }
            Ok(Err(e)) => Ok(HealthCheckResult::unhealthy(format!("요청 실패: {}", e))),
            Err(_) => Ok(HealthCheckResult::unhealthy(format!("타임아웃 ({}초)", self.timeout_secs))),
        }
    }
}

/// TCP 헬스 체커
pub struct TcpHealthChecker {
    addr: String,
    port: u16,
    timeout_secs: u64,
}

impl TcpHealthChecker {
    pub fn new(addr: String, check_type: &HealthCheckType, timeout_secs: u64) -> Option<Self> {
        match check_type {
            HealthCheckType::Tcp { port } => Some(Self {
                addr,
                port: *port,
                timeout_secs,
            }),
            _ => None,
        }
    }
}

#[async_trait]
impl HealthChecker for TcpHealthChecker {
    async fn check(&self) -> Result<HealthCheckResult, DockerError> {
        let addr = format!("{}:{}", self.addr, self.port);
        debug!("TCP 헬스 체크 시작: {}", addr);

        match timeout(
            std::time::Duration::from_secs(self.timeout_secs),
            TcpStream::connect(&addr)
        ).await {
            Ok(Ok(_)) => Ok(HealthCheckResult::healthy(format!("TCP 연결 성공: {}", addr))),
            Ok(Err(e)) => Ok(HealthCheckResult::unhealthy(format!("TCP 연결 실패: {}", e))),
            Err(_) => Ok(HealthCheckResult::unhealthy(format!("타임아웃 ({}초)", self.timeout_secs))),
        }
    }
}

/// 헬스 체커 팩토리
pub struct HealthCheckerFactory;

impl HealthCheckerFactory {
    pub fn create(addr: String, check_type: &HealthCheckType, timeout_secs: u64) -> Option<Box<dyn HealthChecker>> {
        match check_type {
            HealthCheckType::Http { .. } => {
                HttpHealthChecker::new(addr, check_type, timeout_secs)
                    .map(|checker| Box::new(checker) as Box<dyn HealthChecker>)
            }
            HealthCheckType::Tcp { .. } => {
                TcpHealthChecker::new(addr, check_type, timeout_secs)
                    .map(|checker| Box::new(checker) as Box<dyn HealthChecker>)
            }
        }
    }
}

/// 컨테이너 헬스 체크 상태 관리
pub struct ContainerHealth {
    pub container_id: String,
    pub checker: Box<dyn HealthChecker>,
    pub last_check: Option<HealthCheckResult>,
    pub check_count: u64,
    pub consecutive_failures: u64,
}

impl fmt::Debug for ContainerHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContainerHealth")
            .field("container_id", &self.container_id)
            .field("checker", &"<dyn HealthChecker>")  // checker는 간단히 표시
            .field("last_check", &self.last_check)
            .field("check_count", &self.check_count)
            .field("consecutive_failures", &self.consecutive_failures)
            .finish()
    }
}

impl ContainerHealth {
    pub fn new(container_id: String, checker: Box<dyn HealthChecker>) -> Self {
        Self {
            container_id,
            checker,
            last_check: None,
            check_count: 0,
            consecutive_failures: 0,
        }
    }

    pub async fn check(&mut self) -> Result<&HealthCheckResult, DockerError> {
        let result = self.checker.check().await?;
        self.check_count += 1;
        
        match result.status {
            HealthStatus::Healthy => self.consecutive_failures = 0,
            HealthStatus::Unhealthy => self.consecutive_failures += 1,
            _ => {}
        }

        self.last_check = Some(result);
        Ok(self.last_check.as_ref().unwrap())
    }
} 