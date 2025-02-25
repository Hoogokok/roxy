use std::fmt;

/// 라우팅 관련 에러를 표현하는 열거형입니다.
#[derive(Debug, PartialEq)]
pub enum RoutingError {
    /// 유효하지 않은 호스트 이름
    InvalidHost {
        host: String,
        reason: String,
    },
    /// 유효하지 않은 포트 번호
    InvalidPort {
        port: String,
        reason: String,
    },
    /// Host 헤더 누락
    MissingHost,
    /// 백엔드 서비스를 찾을 수 없음
    BackendNotFound {
        host: String,
        available_routes: Vec<String>,
    },
    /// 헤더 파싱 에러
    HeaderParseError {
        header_name: String,
        error: String,
    },
    /// 잘못된 경로 패턴
    InvalidPathPattern {
        pattern: String,
        reason: String,
    },
}

impl fmt::Display for RoutingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RoutingError::InvalidHost { host, reason } => 
                write!(f, "유효하지 않은 호스트 {}: {}", host, reason),
            RoutingError::InvalidPort { port, reason } => 
                write!(f, "유효하지 않은 포트 {}: {}", port, reason),
            RoutingError::MissingHost => 
                write!(f, "Host 헤더가 누락됨"),
            RoutingError::BackendNotFound { host, available_routes } => 
                write!(f, "호스트 {}에 대한 백엔드를 찾을 수 없음 (사용 가능한 라우트: {:?})", host, available_routes),
            RoutingError::HeaderParseError { header_name, error } => 
                write!(f, "{} 헤더 파싱 실패: {}", header_name, error),
            RoutingError::InvalidPathPattern { pattern, reason } => 
                write!(f, "잘못된 경로 패턴: {} ({})", pattern, reason),
        }
    }
}

impl std::error::Error for RoutingError {}

#[derive(Debug)]
pub enum BackendError {
    NoAddresses,
    IndexOutOfBounds { index: usize, len: usize },
    LoadBalancerNotEnabled,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::NoAddresses => write!(f, "백엔드 주소가 없음"),
            BackendError::IndexOutOfBounds { index, len } => 
                write!(f, "백엔드 주소 인덱스 범위 초과: index={}, len={}", index, len),
            BackendError::LoadBalancerNotEnabled =>
                write!(f, "로드밸런서가 활성화되지 않음"),
        }
    }
}

impl std::error::Error for BackendError {} 