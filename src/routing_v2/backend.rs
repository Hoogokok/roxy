use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::routing_v2::error::BackendError;

/// 백엔드 서비스 정보를 담는 구조체입니다.
#[derive(Debug)]
pub struct BackendService {
    pub addresses: Vec<SocketAddr>,
    current_index: AtomicUsize,
}

impl Clone for BackendService {
    fn clone(&self) -> Self {
        Self {
            addresses: self.addresses.clone(),
            current_index: AtomicUsize::new(
                self.current_index.load(Ordering::Relaxed)
            ),
        }
    }
}

impl BackendService {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addresses: vec![addr],
            current_index: AtomicUsize::new(0),
        }
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