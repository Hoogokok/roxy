use std::fmt;

use crate::routing_v2::BackendError;

#[derive(Debug)]
pub enum DockerError {
    /// Docker 데몬 연결 실패
    ConnectionError {
        source: bollard::errors::Error,
        context: String,
    },
    /// 컨테이너 목록 조회 실패
    ListContainersError {
        source: bollard::errors::Error,
        context: String,
    },
    /// 컨테이너 설정 오류
    ContainerConfigError {
        container_id: String,
        reason: String,
        context: Option<String>,
    },
    /// 주소 파싱 오류
    AddressParseError {
        container_id: String,
        address: String,
        network: String,
        context: Option<String>,
    },
    /// 네트워크 설정 오류
    NetworkError {
        container_id: String,
        network: String,
        reason: String,
        context: Option<String>,
    },
    BackendError {
        container_id: String,
        error: String,
    },
}

impl fmt::Display for DockerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DockerError::ConnectionError { source, context } => 
                write!(f, "Docker 데몬 연결 실패 ({}): {}", context, source),
            DockerError::ListContainersError { source, context } => 
                write!(f, "컨테이너 목록 조회 실패 ({}): {}", context, source),
            DockerError::ContainerConfigError { container_id, reason, context } => 
                if let Some(ctx) = context {
                    write!(f, "컨테이너 {} 설정 오류 ({}): {}", container_id, ctx, reason)
                } else {
                    write!(f, "컨테이너 {} 설정 오류: {}", container_id, reason)
                },
            DockerError::AddressParseError { container_id, address, network, context } => 
                write!(f, "컨테이너 {}의 네트워크 {} 주소 {} 파싱 실패 ({})", 
                    container_id, network, address, context.as_deref().unwrap_or("No context provided")),
            DockerError::NetworkError { container_id, network, reason, context } =>
                write!(f, "컨테이너 {}의 네트워크 {} 설정 오류 ({}): {}", 
                    container_id, network, context.as_deref().unwrap_or("No context provided"), reason),
            DockerError::BackendError { container_id, error } => 
                write!(f, "백엔드 서비스 오류 (컨테이너 {}): {}", container_id, error),
        }
    }
}

impl std::error::Error for DockerError {}

impl From<bollard::errors::Error> for DockerError {
    fn from(err: bollard::errors::Error) -> Self {
        DockerError::ConnectionError {
            source: err,
            context: "Docker 데몬 연결 실패".to_string(),
        }
    }
}

impl From<BackendError> for DockerError {
    fn from(err: BackendError) -> Self {
        DockerError::ContainerConfigError {
            container_id: "unknown".to_string(),
            reason: "백엔드 서비스 설정 실패".to_string(),
            context: Some(err.to_string()),
        }
    }
}

/// 재시도 가능한 오류 타입
#[derive(Debug, Clone)]
pub enum RetryableError {
    /// 연결 실패
    ConnectionFailed,
    /// 타임아웃
    Timeout,
    /// 서비스 일시적 사용 불가
    ServiceUnavailable,
}

impl DockerError {
    /// 재시도 가능한 오류인지 확인
    pub fn is_retryable(&self) -> bool {
        match self {
            DockerError::ConnectionError { .. } => true,
            DockerError::NetworkError { reason, .. } => {
                // 일시적인 네트워크 오류만 재시도
                reason.contains("timeout") || 
                reason.contains("connection refused") ||
                reason.contains("service unavailable")
            }
            _ => false
        }
    }
}