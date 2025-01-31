use std::collections::HashMap;
use std::net::SocketAddr;
use hyper::header;
use std::fmt;

// 호스트 정보를 담는 불변 데이터 구조
#[derive(Clone, Debug)]
pub struct HostInfo {
    pub name: String,
    pub port: Option<u16>,
}

// 커스텀 에러 타입 정의
#[derive(Debug)]
pub enum RoutingError {
    InvalidHost(String),
    InvalidPort(String),
    MissingHost,
    BackendNotFound(String),
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
    // Result를 반환하도록 수정
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

// 백엔드 서비스 정보를 담는 구조체
#[derive(Clone, Debug)]
pub struct BackendService {
    pub address: SocketAddr,
}

// 라우팅 테이블 구조체
#[derive(Clone)]
pub struct RoutingTable {
    routes: HashMap<String, BackendService>,
}

impl RoutingTable {
    pub fn new() -> Self {
        RoutingTable {
            routes: HashMap::new(),
        }
    }

    pub fn add_route(&mut self, host: String, service: BackendService) {
        self.routes.insert(host, service);
    }

    pub fn get_backend(&self, host: &str) -> Result<&BackendService, RoutingError> {
        self.routes
            .get(host)
            .ok_or_else(|| RoutingError::BackendNotFound(host.to_string()))
    }

    pub fn extract_host(req: &hyper::Request<hyper::body::Incoming>) -> Result<HostInfo, RoutingError> {
        let host = req.headers()
            .get(header::HOST)
            .ok_or(RoutingError::MissingHost)?
            .to_str()
            .map_err(|e| RoutingError::HeaderParseError(e.to_string()))?;
        
        HostInfo::from_header_value(host)
    }

    pub fn find_backend(&self, host_info: &HostInfo) -> Result<&BackendService, RoutingError> {
        self.get_backend(&host_info.name)
    }
} 