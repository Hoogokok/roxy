use std::collections::HashMap;
use std::net::SocketAddr;
use hyper::header;

// 호스트 정보를 담는 불변 데이터 구조
#[derive(Clone, Debug)]
pub struct HostInfo {
    pub name: String,
    pub port: Option<u16>,
}

impl HostInfo {
    // 호스트 헤더 값에서 HostInfo를 생성하는 순수 함수
    pub fn from_header_value(host: &str) -> Option<Self> {
        let parts: Vec<&str> = host.split(':').collect();
        match parts.as_slice() {
            [name] => Some(HostInfo {
                name: name.to_string(),
                port: None,
            }),
            [name, port] => Some(HostInfo {
                name: name.to_string(),
                port: port.parse().ok(),
            }),
            _ => None,
        }
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

    pub fn get_backend(&self, host: &str) -> Option<&BackendService> {
        self.routes.get(host)
    }

    // 요청에서 호스트 정보를 추출하는 순수 함수
    pub fn extract_host(req: &hyper::Request<hyper::body::Incoming>) -> Option<HostInfo> {
        req.headers()
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .and_then(HostInfo::from_header_value)
    }

    // 호스트 정보로 백엔드 서비스 찾기
    pub fn find_backend(&self, host_info: &HostInfo) -> Option<&BackendService> {
        self.get_backend(&host_info.name)
    }
} 