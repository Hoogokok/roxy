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
use tracing::{debug, error, info, warn};

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
    pub fn from_header_value(value: &str) -> Result<Self, RoutingError> {
        if value.is_empty() {
            return Err(RoutingError::InvalidHost {
                host: value.to_string(),
                reason: "Invalid format".to_string(),
            });
        }

        // 호스트와 포트 분리
        let parts: Vec<&str> = value.split(':').collect();
        match parts.len() {
            1 => Ok(HostInfo {
                name: value.to_string(),
                port: None,
            }),
            2 => {
                // 호스트 이름이 비어있는지 확인
                if parts[0].is_empty() {
                    return Err(RoutingError::InvalidHost {
                        host: value.to_string(),
                        reason: "Invalid format".to_string(),
                    });
                }

                let port = parts[1].parse::<u16>().map_err(|_| {
                    RoutingError::InvalidPort {
                        port: parts[1].to_string(),
                        reason: "Invalid format".to_string(),
                    }
                })?;

                if port == 0 {
                    return Err(RoutingError::InvalidPort {
                        port: parts[1].to_string(),
                        reason: "Port must be greater than 0".to_string(),
                    });
                }

                Ok(HostInfo {
                    name: parts[0].to_string(),
                    port: Some(port),
                })
            }
            _ => Err(RoutingError::InvalidHost {
                host: value.to_string(),
                reason: "Invalid format".to_string(),
            }),
        }
    }
}

/// 백엔드 서비스 정보를 담는 구조체입니다.
#[derive(Debug)]
pub struct BackendService {
    pub addresses: Vec<SocketAddr>,
    current_index: std::sync::atomic::AtomicUsize,
}

impl Clone for BackendService {
    fn clone(&self) -> Self {
        Self {
            addresses: self.addresses.clone(),
            current_index: std::sync::atomic::AtomicUsize::new(
                self.current_index.load(std::sync::atomic::Ordering::Relaxed)
            ),
        }
    }
}

#[derive(Debug)]
pub enum BackendError {
    NoAddresses,
    IndexOutOfBounds { index: usize, len: usize },
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::NoAddresses => write!(f, "백엔드 주소가 없음"),
            BackendError::IndexOutOfBounds { index, len } => 
                write!(f, "백엔드 주소 인덱스 범위 초과: index={}, len={}", index, len),
        }
    }
}

impl std::error::Error for BackendError {}

impl BackendService {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addresses: vec![addr],
            current_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn get_next_address(&self) -> Result<SocketAddr, BackendError> {
        let len = self.addresses.len();
        if len == 0 {
            return Err(BackendError::NoAddresses);
        }
        
        let index = self.current_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % len;
        self.addresses.get(index)
            .copied()
            .ok_or_else(|| BackendError::IndexOutOfBounds { index, len })
    }
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
        if self.routes.remove(host).is_some() {
            info!(host = %host, "라우트 제거됨");
        } else {
            debug!(host = %host, "존재하지 않는 라우트 제거 시도");
        }
    }

    /// 라우팅 테이블에 새로운 라우트를 추가합니다.
    /// 
    /// # 인자
    /// 
    /// * `host` - 호스트 이름
    /// * `service` - 백엔드 서비스 정보
    pub fn add_route(&mut self, host: String, service: BackendService) {
        match self.routes.get_mut(&host) {
            Some(existing) => {
                let new_addresses = service.addresses.clone();
                existing.addresses.extend(service.addresses);
                info!(
                    host = %host,
                    existing_addresses = ?existing.addresses,
                    new_addresses = ?new_addresses,
                    "기존 서비스에 주소 추가"
                );
            }
            None => {
                info!(
                    host = %host,
                    addresses = ?service.addresses,
                    "새 라우트 추가"
                );
                self.routes.insert(host, service);
            }
        }
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
            .ok_or_else(|| RoutingError::BackendNotFound {
                host: host.to_string(),
                available_routes: self.routes.keys().cloned().collect(),
            })
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
        debug!(host = %host_info.name, "라우팅 요청 처리");
        
        let backend = self.find_backend(&host_info);
        match &backend {
            Ok(_) => info!(host = %host_info.name, "백엔드 서비스 찾음"),
            Err(e) => warn!(
                host = %host_info.name,
                error = %e,
                available_routes = ?self.routes.keys().collect::<Vec<_>>(),
                "백엔드 서비스를 찾을 수 없음"
            ),
        }
        backend
    }

    /// HTTP 요청에서 호스트 정보를 추출합니다.
    pub fn extract_host<B>(req: &hyper::Request<B>) -> Result<HostInfo, RoutingError> {
        let host = match req.headers().get(header::HOST) {
            Some(value) => {
                debug!("Host 헤더 발견");
                value
            }
            None => {
                warn!("Host 헤더 누락");
                return Err(RoutingError::MissingHost);
            }
        };

        match host.to_str() {
            Ok(host_str) => {
                debug!(host = %host_str, "Host 헤더 파싱 성공");
                HostInfo::from_header_value(host_str)
            }
            Err(e) => {
                error!(error = %e, "Host 헤더 파싱 실패");
                Err(RoutingError::HeaderParseError { 
                    header_name: "Host".to_string(), 
                    error: e.to_string() 
                })
            }
        }
    }

    /// 호스트 정보를 기반으로 백엔드 서비스를 찾습니다.
    pub fn find_backend(&self, host_info: &HostInfo) -> Result<&BackendService, RoutingError> {
        self.routes
            .get(&host_info.name)
            .ok_or_else(|| RoutingError::BackendNotFound {
                host: host_info.name.clone(),
                available_routes: self.routes.keys().cloned().collect(),
            })
    }

    /// Docker 컨테이너로부터 라우팅 규칙을 업데이트합니다.
    pub fn sync_docker_routes(&mut self, routes: HashMap<String, BackendService>) {
        info!(
            route_count = routes.len(),
            routes = ?routes.keys().collect::<Vec<_>>(),
            "Docker 라우트 동기화"
        );
        self.routes = routes;
    }
} 