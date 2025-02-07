use std::collections::HashMap;
use hyper::header;
use tracing::{debug, info, warn};

use crate::routing_v2::{
    HostInfo,
    BackendService,
    PathMatcher,
    RoutingError,
};

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