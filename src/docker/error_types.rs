use std::fmt;

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