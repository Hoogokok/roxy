use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::routing_v2::error::BackendError;

/// 백엔드 서비스 정보를 담는 구조체입니다.
#[derive(Debug)]
pub struct BackendService {
    pub addresses: Vec<SocketAddr>,
    current_index: AtomicUsize,
    pub middlewares: Option<Vec<String>>,
    pub router_name: Option<String>,
}

impl Clone for BackendService {
    fn clone(&self) -> Self {
        Self {
            addresses: self.addresses.clone(),
            current_index: AtomicUsize::new(
                self.current_index.load(Ordering::Relaxed)
            ),
            middlewares: self.middlewares.clone(),
            router_name: self.router_name.clone(),
        }
    }
}

impl BackendService {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addresses: vec![addr],
            current_index: AtomicUsize::new(0),
            middlewares: None,
            router_name: None,
        }
    }

    pub fn with_middleware(addr: SocketAddr, middleware: String) -> Self {
        Self {
            addresses: vec![addr],
            current_index: AtomicUsize::new(0),
            middlewares: Some(vec![middleware]),
            router_name: None,
        }
    }

    pub fn with_router(addr: SocketAddr, router_name: Option<String>) -> Self {
        Self {
            addresses: vec![addr],
            current_index: AtomicUsize::new(0),
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
        let len = self.addresses.len();
        if len == 0 {
            return Err(BackendError::NoAddresses);
        }
        
        let index = self.current_index.fetch_add(1, Ordering::Relaxed) % len;
        self.addresses.get(index)
            .copied()
            .ok_or_else(|| BackendError::IndexOutOfBounds { index, len })
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

#[derive(Debug)]
pub struct LoadBalancer {
    /// 주소 목록 (가중치 포함)
    addresses: Vec<(SocketAddr, usize)>,
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