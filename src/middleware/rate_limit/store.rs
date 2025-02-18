use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;
use async_trait::async_trait;

/// 토큰 버킷 구현
#[derive(Debug)]
struct TokenBucket {
    /// 현재 사용 가능한 토큰 수
    tokens: f64,
    /// 마지막 업데이트 시간
    last_update: Instant,
    /// 초당 보충되는 토큰 수 (rate)
    rate: f64,
    /// 최대 토큰 수 (capacity)
    capacity: f64,
}

impl TokenBucket {
    fn new(rate: f64, capacity: f64) -> Self {
        Self {
            tokens: capacity,
            last_update: Instant::now(),
            rate,
            capacity,
        }
    }

    /// 토큰을 소비하려고 시도합니다.
    fn try_consume(&mut self, tokens: f64) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        
        // 토큰 보충
        self.tokens = (self.tokens + elapsed.as_secs_f64() * self.rate)
            .min(self.capacity);
        self.last_update = now;

        // 토큰 소비 시도
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// 다음 토큰이 사용 가능할 때까지 남은 시간을 반환합니다.
    fn time_to_next_token(&self) -> Duration {
        if self.tokens > 0.0 {
            Duration::from_secs(0)
        } else {
            let tokens_needed = 1.0;
            let time_needed = tokens_needed / self.rate;
            Duration::from_secs_f64(time_needed)
        }
    }
}

/// 토큰 버킷 저장소
#[derive(Debug, Clone)]
pub struct TokenBucketStore {
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

impl TokenBucketStore {
    pub fn new() -> Self {
        Self {
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 요청을 처리할 수 있는지 확인합니다.
    pub async fn check_rate(&self, key: &str, rate: f64, capacity: f64) -> bool {
        let mut buckets = self.buckets.write().await;
        
        let bucket = buckets.entry(key.to_string()).or_insert_with(|| {
            debug!("새로운 토큰 버킷 생성: key={}, rate={}, capacity={}", key, rate, capacity);
            TokenBucket::new(rate, capacity)
        });

        bucket.try_consume(1.0)
    }

    /// 다음 요청까지 대기해야 하는 시간을 반환합니다.
    pub async fn time_to_next_request(&self, key: &str) -> Option<Duration> {
        let buckets = self.buckets.read().await;
        buckets.get(key).map(|bucket| bucket.time_to_next_token())
    }
}

/// 속도 제한 저장소 trait
#[async_trait]
pub trait RateLimitStore: Send + Sync {
    /// 요청을 처리할 수 있는지 확인합니다.
    async fn check_rate(&self, key: &str, rate: f64, capacity: f64) -> bool;
    
    /// 다음 요청까지 대기해야 하는 시간을 반환합니다.
    async fn time_to_next_request(&self, key: &str) -> Option<Duration>;
}

/// 메모리 기반 저장소 구현을 위한 모듈
pub mod memory {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    
    /// 메모리 기반 저장소
    #[derive(Debug, Clone)]
    pub struct MemoryStore {
        // 실제 구현은 다음 단계에서 진행
        _private: (),
    }

    impl MemoryStore {
        pub fn new() -> Self {
            Self { _private: () }
        }
    }

    #[async_trait]
    impl RateLimitStore for MemoryStore {
        async fn check_rate(&self, _key: &str, _rate: f64, _capacity: f64) -> bool {
            // TODO: 2단계에서 구현
            true
        }

        async fn time_to_next_request(&self, _key: &str) -> Option<Duration> {
            // TODO: 2단계에서 구현
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory::MemoryStore;

    #[tokio::test]
    async fn test_memory_store_trait_implementation() {
        let store = MemoryStore::new();
        
        // 기본 trait 메서드 호출 테스트
        assert!(store.check_rate("test", 1.0, 1.0).await);
        assert_eq!(store.time_to_next_request("test").await, None);
    }
} 