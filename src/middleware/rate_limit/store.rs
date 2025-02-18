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
        buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    }

    impl MemoryStore {
        pub fn new() -> Self {
            Self {
                buckets: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        /// 오래된 버킷을 정리합니다.
        pub async fn cleanup(&self, max_idle: Duration) {
            let mut buckets = self.buckets.write().await;
            let now = Instant::now();
            
            buckets.retain(|_, bucket| {
                now.duration_since(bucket.last_update) < max_idle
            });
        }
    }

    #[async_trait]
    impl RateLimitStore for MemoryStore {
        async fn check_rate(&self, key: &str, rate: f64, capacity: f64) -> bool {
            let mut buckets = self.buckets.write().await;
            
            let bucket = buckets.entry(key.to_string()).or_insert_with(|| {
                debug!("새로운 토큰 버킷 생성: key={}, rate={}, capacity={}", 
                    key, rate, capacity);
                TokenBucket::new(rate, capacity)
            });

            bucket.try_consume(1.0)
        }

        async fn time_to_next_request(&self, key: &str) -> Option<Duration> {
            let buckets = self.buckets.read().await;
            buckets.get(key).map(|bucket| bucket.time_to_next_token())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory::MemoryStore;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_rate_limit_basic() {
        let store = MemoryStore::new();
        let key = "test";
        
        // 초당 2개 요청, 최대 3개까지 버스트
        assert!(store.check_rate(key, 2.0, 3.0).await);
        assert!(store.check_rate(key, 2.0, 3.0).await);
        assert!(store.check_rate(key, 2.0, 3.0).await);
        assert!(!store.check_rate(key, 2.0, 3.0).await);
    }

    #[tokio::test]
    async fn test_rate_limit_refill() {
        let store = MemoryStore::new();
        let key = "test";
        
        // 초당 2개 요청
        assert!(store.check_rate(key, 2.0, 2.0).await);
        assert!(store.check_rate(key, 2.0, 2.0).await);
        assert!(!store.check_rate(key, 2.0, 2.0).await);

        // 0.6초 대기 (1개 토큰 생성되어야 함)
        sleep(Duration::from_millis(600)).await;
        assert!(store.check_rate(key, 2.0, 2.0).await);
    }

    #[tokio::test]
    async fn test_cleanup() {
        let store = MemoryStore::new();
        let key = "test";
        
        // 버킷 생성
        assert!(store.check_rate(key, 1.0, 1.0).await);
        
        // cleanup 전에는 버킷이 존재
        assert!(store.time_to_next_request(key).await.is_some());
        
        // cleanup 실행 (1ms 이상 된 버킷 제거)
        sleep(Duration::from_millis(2)).await;
        store.cleanup(Duration::from_millis(1)).await;
        
        // cleanup 후에는 버킷이 제거됨
        assert!(store.time_to_next_request(key).await.is_none());
    }
} 