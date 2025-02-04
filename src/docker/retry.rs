use async_trait::async_trait;
use tokio::time::{sleep, Duration};
use crate::docker::DockerManager;
use std::collections::HashMap;
use tracing::warn;
use crate::routing::BackendService;
use crate::docker::DockerError;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub delay: Duration,
}

impl RetryPolicy {
    pub fn new(max_attempts: u32, delay: Duration) -> Self {
        Self { max_attempts, delay }
    }
}

#[async_trait]
pub trait Retryable {
    type Output;
    type Error;
    
    async fn execute(&self) -> Result<Self::Output, Self::Error>;
    fn should_retry(&self, result: &Result<Self::Output, Self::Error>) -> bool;
    fn on_retry(&self, attempt: u32, error: &Self::Error);
}

pub async fn with_retry<T>(
    operation: T,
    policy: RetryPolicy,
) -> Result<T::Output, T::Error>
where
    T: Retryable,
{
    let mut attempt = 0;
    
    while attempt < policy.max_attempts {
        let result = operation.execute().await;
        
        if !operation.should_retry(&result) {
            return result;
        }
        
        if let Err(ref e) = result {
            operation.on_retry(attempt, e);
        }
        
        attempt += 1;
        if attempt < policy.max_attempts {
            sleep(policy.delay).await;
        }
    }
    
    operation.execute().await
}

pub struct ContainerRoutesRetry<'a> {
   pub docker_manager: &'a DockerManager,
}

#[async_trait]
impl<'a> Retryable for ContainerRoutesRetry<'a> {
    type Output = HashMap<String, BackendService>;
    type Error = DockerError;

    async fn execute(&self) -> Result<Self::Output, Self::Error> {
        self.docker_manager.try_get_container_routes().await
    }

    fn should_retry(&self, result: &Result<Self::Output, Self::Error>) -> bool {
        match result {
            Ok(routes) => routes.is_empty(),
            Err(_) => true,
        }
    }

    fn on_retry(&self, attempt: u32, error: &Self::Error) {
        warn!(
            error = %error,
            "컨테이너 라우트 조회 실패 (시도 {}/{}), 재시도 중...", 
            attempt + 1,
            3
        );
    }
} 

