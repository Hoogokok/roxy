use std::collections::HashMap;
use std::net::SocketAddr;

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
} 