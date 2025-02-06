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
use tracing::{debug, info, warn};
use regex_lite as regex;

/// 호스트 정보를 담는 불변 데이터 구조입니다.
/// 
/// # 필드
/// 
/// * `name` - 호스트 이름 (예: "example.com")
/// * `port` - 선택적 포트 번호
/// * `path` - 선택적 경로 정보
#[derive(Clone, Debug)]
pub struct HostInfo {
    pub name: String,
    pub port: Option<u16>,
    pub path: Option<String>,
}

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
                path: None,
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
                    path: None,
                })
            }
            _ => Err(RoutingError::InvalidHost {
                host: value.to_string(),
                reason: "Invalid format".to_string(),
            }),
        }
    }

    // 새로운 메서드: path 설정
    pub fn with_path(mut self, path: String) -> Self {
        self.path = Some(path);
        self
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
    // (host, PathMatcher)를 키로 사용
    pub routes: HashMap<(String, PathMatcher), BackendService>,
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
        self.routes.retain(|k, _| k.0 != host);
    }

    /// 라우팅 테이블에 새로운 라우트를 추가합니다.
    pub fn add_route(&mut self, host: String, service: BackendService, path_matcher: Option<PathMatcher>) {
        let matcher = path_matcher.unwrap_or_else(|| PathMatcher::from_str("/").unwrap());
        let key = (host, matcher);
        match self.routes.get_mut(&key) {
            Some(existing_service) => {
                existing_service.addresses.extend(service.addresses);
            }
            None => {
                self.routes.insert(key, service);
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
        // 기본 경로 "/"에 대한 PathMatcher 생성
        let default_matcher = PathMatcher::from_str("/").unwrap();
        let key = (host.to_string(), default_matcher);
        
        self.routes.get(&key).ok_or_else(|| RoutingError::BackendNotFound {
            host: host.to_string(),
            available_routes: self.routes.keys()
                .map(|(host, matcher)| format!("{}:{:?}", host, matcher))
                .collect(),
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
            Some(value) => value,
            None => return Err(RoutingError::MissingHost),
        };

        let host_str = host.to_str().map_err(|e| RoutingError::HeaderParseError { 
            header_name: "Host".to_string(), 
            error: e.to_string() 
        })?;

        // 호스트에서 경로 부분 제거하고 포트 파싱
        let host_name = host_str.split('/').next().unwrap_or(host_str);
        let mut host_info = HostInfo::from_header_value(host_name)?;
        
        // URI에서 경로 추출
        let path = req.uri().path();
        host_info.path = if path == "/" { None } else { Some(path.to_string()) };

        Ok(host_info)
    }

    /// 호스트 정보를 기반으로 백엔드 서비스를 찾습니다.
    pub fn find_backend(&self, host_info: &HostInfo) -> Result<&BackendService, RoutingError> {
        let request_path = host_info.path.as_deref().unwrap_or("/");

        // 모든 라우트를 순회하면서 매칭 확인
        for ((host, matcher), backend) in &self.routes {
            if host == &host_info.name && matcher.matches(request_path) {
                return Ok(backend);
            }
        }

        // 매칭되는 라우트를 찾지 못한 경우
        Err(RoutingError::BackendNotFound {
            host: host_info.name.clone(),
            available_routes: self.routes.keys()
                .map(|(host, matcher)| format!("{}:{:?}", host, matcher))
                .collect(),
        })
    }

    /// Docker 컨테이너로부터 라우팅 규칙을 업데이트합니다.
    pub fn sync_docker_routes(&mut self, routes: HashMap<(String, PathMatcher), BackendService>) {
        self.routes = routes;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathMatcherKind {
    Exact,
    Prefix,
    Regex,
}

#[derive(Debug, Clone)]
pub struct PathMatcher {
    kind: PathMatcherKind,
    pub pattern: String,
    regex: Option<regex::Regex>,
}

impl PathMatcher {
    pub fn from_str(pattern: &str) -> Result<Self, RoutingError> {
        if pattern.starts_with("^") {
            // 정규식 매칭
            let re = regex::Regex::new(pattern)
                .map_err(|e| RoutingError::InvalidPathPattern {
                    pattern: pattern.to_string(),
                    reason: e.to_string(),
                })?;
            Ok(PathMatcher {
                kind: PathMatcherKind::Regex,
                pattern: pattern.to_string(),
                regex: Some(re),
            })
        } else {
            // '*' 패턴을 제거하고 모두 Prefix로 처리
            Ok(PathMatcher {
                kind: if pattern.ends_with("*") {
                    PathMatcherKind::Prefix
                } else {
                    PathMatcherKind::Exact
                },
                pattern: pattern.trim_end_matches('*').to_string(),
                regex: None,
            })
        }
    }

    pub fn matches(&self, path: &str) -> bool {
        if self.pattern == "/" {
            return true;
        }

        match self.kind {
            PathMatcherKind::Exact => self.pattern == path,
            PathMatcherKind::Prefix => {
                // Traefik 스타일: 접두사 매칭에서는 trailing slash 무시
                let pattern = self.pattern.trim_end_matches('/');
                let path = path.trim_end_matches('/');
                path == pattern || path.starts_with(&format!("{}/", pattern))
            },
            PathMatcherKind::Regex => self.regex.as_ref()
                .map(|r| r.is_match(path))
                .unwrap_or(false),
        }
    }
}

impl PartialEq for PathMatcher {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.pattern == other.pattern
    }
}

impl Eq for PathMatcher {}

impl std::hash::Hash for PathMatcher {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.pattern.hash(state);
    }
} 