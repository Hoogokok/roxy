use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::routing_v2::error::BackendError;

/// 백엔드 서비스 정보를 담는 구조체입니다.
#[derive(Debug)]
pub struct BackendService {
    /// 기본 주소
    pub address: SocketAddr,
    /// 로드밸런서 (선택적)
    pub load_balancer: Option<LoadBalancer>,
    pub middlewares: Option<Vec<String>>,
    pub router_name: Option<String>,
}

impl Clone for BackendService {
    fn clone(&self) -> Self {
        Self {
            address: self.address,
            load_balancer: self.load_balancer.clone(),
            middlewares: self.middlewares.clone(),
            router_name: self.router_name.clone(),
        }
    }
}

impl BackendService {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            address: addr,
            load_balancer: None,
            middlewares: None,
            router_name: None,
        }
    }

    pub fn with_middleware(addr: SocketAddr, middleware: String) -> Self {
        Self {
            address: addr,
            load_balancer: None,
            middlewares: Some(vec![middleware]),
            router_name: None,
        }
    }

    pub fn with_router(addr: SocketAddr, router_name: Option<String>) -> Self {
        Self {
            address: addr,
            load_balancer: None,
            middlewares: None,
            router_name,
        }
    }

    pub fn set_middlewares(&mut self, middlewares: Vec<String>) {
        self.middlewares = Some(middlewares);
    }

    pub fn add_middleware(&mut self, middleware: String) {
        match &mut self.middlewares {
            Some(middlewares) => middlewares.push(middleware),
            None => self.middlewares = Some(vec![middleware]),
        }
    }

    pub fn has_middlewares(&self) -> bool {
        self.middlewares.as_ref().map_or(false, |m| !m.is_empty())
    }

    pub fn get_next_address(&self) -> Result<SocketAddr, BackendError> {
        match &self.load_balancer {
            Some(lb) => lb.get_next_address(),
            None => Ok(self.address),
        }
    }

    pub fn enable_load_balancer(&mut self, strategy: LoadBalancerStrategy) {
        self.load_balancer = Some(LoadBalancer::new(self.address, strategy));
    }

    pub fn add_address(&mut self, addr: SocketAddr, weight: usize) -> Result<(), BackendError> {
        match &mut self.load_balancer {
            Some(lb) => {
                lb.add_address(addr, weight);
                Ok(())
            }
            None => Err(BackendError::LoadBalancerNotEnabled),
        }
    }
}

/// 로드밸런싱 전략을 정의하는 열거형입니다.
/// 현재 라운드로빈과 가중치 기반 두 가지 전략을 지원합니다.
#[derive(Debug)]
pub enum LoadBalancerStrategy {
    /// 라운드 로빈 방식으로 요청을 순차적으로 각 백엔드에 분배합니다.
    /// 모든 백엔드가 동일한 처리 능력을 가질 때 적합합니다.
    RoundRobin {
        current_index: AtomicUsize,
    },
    /// 가중치 기반 방식으로 각 백엔드의 가중치에 따라 요청을 분배합니다.
    /// 백엔드 서버의 처리 능력이 다를 때 사용합니다.
    Weighted {
        current_index: AtomicUsize,
        total_weight: usize,
    },
}

impl Clone for LoadBalancerStrategy {
    fn clone(&self) -> Self {
        match self {
            Self::RoundRobin { current_index } => Self::RoundRobin {
                current_index: AtomicUsize::new(current_index.load(Ordering::Relaxed)),
            },
            Self::Weighted { current_index, total_weight } => Self::Weighted {
                current_index: AtomicUsize::new(current_index.load(Ordering::Relaxed)),
                total_weight: *total_weight,
            },
        }
    }
}

/// 로드밸런서는 여러 백엔드 서버로 요청을 분배하는 역할을 합니다.
/// 각 백엔드는 주소와 가중치를 가지며, 설정된 전략에 따라 요청이 분배됩니다.
#[derive(Debug,Clone)]
pub struct LoadBalancer {
    /// 백엔드 주소와 가중치 목록입니다.
    /// 튜플의 첫 번째 요소는 서버 주소, 두 번째 요소는 가중치입니다.
    pub addresses: Vec<(SocketAddr, usize)>,
    /// 사용할 로드밸런싱 전략입니다.
    strategy: LoadBalancerStrategy,
}

impl LoadBalancer {
    /// 새로운 로드밸런서를 생성합니다.
    /// 초기 주소와 사용할 전략을 지정해야 합니다.
    pub fn new(initial_addr: SocketAddr, strategy: LoadBalancerStrategy) -> Self {
        Self {
            addresses: vec![(initial_addr, 1)],
            strategy,
        }
    }

    /// 새로운 백엔드 주소를 추가합니다.
    /// 가중치 기반 전략을 사용하는 경우 전체 가중치가 자동으로 업데이트됩니다.
    pub fn add_address(&mut self, addr: SocketAddr, weight: usize) {
        self.addresses.push((addr, weight));
        if let LoadBalancerStrategy::Weighted { total_weight, .. } = &mut self.strategy {
            *total_weight += weight;
        }
    }

    /// 다음 요청을 처리할 백엔드 주소를 선택합니다.
    /// 설정된 전략(라운드로빈/가중치)에 따라 적절한 주소를 반환합니다.
    pub fn get_next_address(&self) -> Result<SocketAddr, BackendError> {
        match &self.strategy {
            LoadBalancerStrategy::RoundRobin { current_index } => {
                let index = current_index.fetch_add(1, Ordering::Relaxed) % self.addresses.len();
                self.addresses.get(index)
                    .map(|(addr, _)| *addr)
                    .ok_or(BackendError::NoAddresses)
            }
            LoadBalancerStrategy::Weighted { current_index, total_weight } => {
                let index = current_index.fetch_add(1, Ordering::Relaxed) % *total_weight;
                let mut current_weight = 0;
                
                for (addr, weight) in &self.addresses {
                    current_weight += weight;
                    if index < current_weight {
                        return Ok(*addr);
                    }
                }
                
                self.addresses.last()
                    .map(|(addr, _)| *addr)
                    .ok_or(BackendError::NoAddresses)
            }
        }
    }
} 