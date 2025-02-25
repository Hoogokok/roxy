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

#[derive(Debug)]
pub enum LoadBalancerStrategy {
    /// 라운드 로빈 방식
    RoundRobin {
        current_index: AtomicUsize,
    },
    /// 가중치 기반 방식
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

#[derive(Debug,Clone)]
pub struct LoadBalancer {
    /// 주소 목록 (가중치 포함)
    pub addresses: Vec<(SocketAddr, usize)>,
    /// 로드밸런싱 전략
    strategy: LoadBalancerStrategy,
}

impl LoadBalancer {
    pub fn new(initial_addr: SocketAddr, strategy: LoadBalancerStrategy) -> Self {
        Self {
            addresses: vec![(initial_addr, 1)],
            strategy,
        }
    }

    pub fn add_address(&mut self, addr: SocketAddr, weight: usize) {
        self.addresses.push((addr, weight));
        if let LoadBalancerStrategy::Weighted { total_weight, .. } = &mut self.strategy {
            *total_weight += weight;
        }
    }

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