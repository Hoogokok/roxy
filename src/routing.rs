//! 호스트 기반 라우팅을 위한 핵심 기능을 제공하는 모듈입니다.
//! 
//! 이 모듈은 다음과 같은 주요 구성 요소를 포함합니다:
//! - `HostInfo`: 호스트 정보를 표현하는 구조체
//! - `BackendService`: 백엔드 서비스 정보를 표현하는 구조체
//! - `RoutingTable`: 라우팅 규칙을 관리하는 구조체

use std::collections::HashMap;
use std::net::SocketAddr;
use hyper::header;
use std::fmt;

/// 호스트 정보를 담는 불변 데이터 구조입니다.
/// 
/// # 필드
/// 
/// * `name` - 호스트 이름 (예: "example.com")
/// * `port` - 선택적 포트 번호
#[derive(Clone, Debug)]
pub struct HostInfo {
    pub name: String,
    pub port: Option<u16>,
}

/// 라우팅 관련 에러를 표현하는 열거형입니다.
#[derive(Debug)]
pub enum RoutingError {
    /// 유효하지 않은 호스트 이름
    InvalidHost(String),
    /// 유효하지 않은 포트 번호
    InvalidPort(String),
    /// Host 헤더 누락
    MissingHost,
    /// 백엔드 서비스를 찾을 수 없음
    BackendNotFound(String),
    /// 헤더 파싱 에러
    HeaderParseError(String),
}

impl fmt::Display for RoutingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RoutingError::InvalidHost(host) => write!(f, "Invalid host: {}", host),
            RoutingError::InvalidPort(port) => write!(f, "Invalid port: {}", port),
            RoutingError::MissingHost => write!(f, "Missing Host header"),
            RoutingError::BackendNotFound(host) => write!(f, "Backend not found for host: {}", host),
            RoutingError::HeaderParseError(err) => write!(f, "Failed to parse header: {}", err),
        }
    }
}

impl std::error::Error for RoutingError {}

impl HostInfo {
    /// 호스트 헤더 값에서 HostInfo를 생성합니다.
    /// 
    /// # 인자
    /// 
    /// * `host` - 파싱할 호스트 문자열 (예: "example.com" 또는 "example.com:8080")
    /// 
    /// # 반환
    /// 
    /// 성공 시 `HostInfo` 인스턴스를 포함한 `Ok`를 반환하고,
    /// 실패 시 적절한 `RoutingError`를 포함한 `Err`를 반환합니다.
    /// 
    /// # 예제
    /// 
    /// ```
    /// use reverse_proxy_traefik::routing::HostInfo;
    /// 
    /// let host_info = HostInfo::from_header_value("example.com:8080").unwrap();
    /// assert_eq!(host_info.name, "example.com");
    /// assert_eq!(host_info.port, Some(8080));
    /// ```
    pub fn from_header_value(host: &str) -> Result<Self, RoutingError> {
        let parse_host_parts = |s: &str| -> Result<(String, Option<u16>), RoutingError> {
            let parts: Vec<&str> = s.split(':').collect();
            match parts.as_slice() {
                [name] if !name.is_empty() => 
                    Ok((name.to_string(), None)),
                [name, port] if !name.is_empty() => {
                    port.parse::<u16>()
                        .map_err(|_| RoutingError::InvalidPort(port.to_string()))
                        .and_then(|p| {
                            if p > 0 {
                                Ok((name.to_string(), Some(p)))
                            } else {
                                Err(RoutingError::InvalidPort(p.to_string()))
                            }
                        })
                }
                _ => Err(RoutingError::InvalidHost(s.to_string())),
            }
        };

        parse_host_parts(host).map(|(name, port)| HostInfo { name, port })
    }
}

/// 백엔드 서비스 정보를 담는 구조체입니다.
#[derive(Clone, Debug)]
pub struct BackendService {
    /// 백엔드 서비스의 주소 (IP:Port)
    pub address: SocketAddr,
}

/// 라우팅 테이블을 관리하는 구조체입니다.
#[derive(Clone)]
pub struct RoutingTable {
    routes: HashMap<String, BackendService>,
}

impl RoutingTable {
    /// 새로운 라우팅 테이블을 생성합니다.
    pub fn new() -> Self {
        RoutingTable {
            routes: HashMap::new(),
        }
    }

    /// 라우팅 테이블에서 호스트를 제거합니다.
    pub fn remove_route(&mut self, host: &str) {
        self.routes.remove(host);
    }

    /// 라우팅 테이블에 새로운 라우트를 추가합니다.
    /// 
    /// # 인자
    /// 
    /// * `host` - 호스트 이름
    /// * `service` - 백엔드 서비스 정보
    pub fn add_route(&mut self, host: String, service: BackendService) {
        self.routes.insert(host, service);
    }

    /// 주어진 호스트에 대한 백엔드 서비스를 찾습니다.
    /// 
    /// # 인자
    /// 
    /// * `host` - 찾을 호스트 이름
    /// 
    /// # 반환
    /// 
    /// 성공 시 `BackendService`에 대한 참조를 포함한 `Ok`를 반환하고,
    /// 실패 시 `BackendNotFound` 에러를 포함한 `Err`를 반환합니다.
    pub fn get_backend(&self, host: &str) -> Result<&BackendService, RoutingError> {
        self.routes
            .get(host)
            .ok_or_else(|| RoutingError::BackendNotFound(host.to_string()))
    }

    /// HTTP 요청에서 호스트 정보를 추출하고 해당하는 백엔드 서비스를 찾습니다.
    /// 
    /// # 인자
    /// 
    /// * `req` - HTTP 요청
    /// 
    /// # 반환
    /// 
    /// 성공 시 `BackendService`에 대한 참조를 포함한 `Ok`를 반환하고,
    /// 실패 시 적절한 `RoutingError`를 포함한 `Err`를 반환합니다.
    pub fn route_request<B>(&self, req: &hyper::Request<B>) -> Result<&BackendService, RoutingError> {
        let host_info = Self::extract_host(req)?;
        println!("Attempting to route request for host: {}", host_info.name);
        println!("Available routes: {:?}", self.routes);
        self.find_backend(&host_info)
    }

    /// HTTP 요청에서 호스트 정보를 추출합니다.
    pub fn extract_host<B>(req: &hyper::Request<B>) -> Result<HostInfo, RoutingError> {
        let host = req.headers()
            .get(header::HOST)
            .ok_or(RoutingError::MissingHost)?
            .to_str()
            .map_err(|e| RoutingError::HeaderParseError(e.to_string()))?;
        
        HostInfo::from_header_value(host)
    }

    /// 호스트 정보를 기반으로 백엔드 서비스를 찾습니다.
    pub fn find_backend(&self, host_info: &HostInfo) -> Result<&BackendService, RoutingError> {
        self.get_backend(&host_info.name)
    }

    /// Docker 컨테이너로부터 라우팅 규칙을 업데이트합니다.
    pub fn sync_docker_routes(&mut self, routes: HashMap<String, BackendService>) {
        println!("Syncing routes: {:?}", routes);
        self.routes = routes;
    }
} 