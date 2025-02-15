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
}

impl Clone for BackendService {
    fn clone(&self) -> Self {
        Self {
            addresses: self.addresses.clone(),
            current_index: AtomicUsize::new(
                self.current_index.load(Ordering::Relaxed)
            ),
            middlewares: self.middlewares.clone(),
        }
    }
}

impl BackendService {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addresses: vec![addr],
            current_index: AtomicUsize::new(0),
            middlewares: None,
        }
    }

    pub fn with_middleware(addr: SocketAddr, middleware: String) -> Self {
        Self {
            addresses: vec![addr],
            current_index: AtomicUsize::new(0),
            middlewares: Some(vec![middleware]),
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