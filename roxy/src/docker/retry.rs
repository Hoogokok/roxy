use async_trait::async_trait;
use tokio::time::{sleep, Duration};
use crate::{docker::DockerManager, routing_v2::{BackendService, PathMatcher}};
use std::collections::HashMap;
use tracing::warn;
use crate::docker::DockerError;
use crate::settings::docker::RetrySettings;

/// 재시도 정책
#[derive(Clone)]
pub struct RetryPolicy {
    /// 최대 시도 횟수
    pub max_attempts: u32,
    /// 재시도 간격
    pub interval: Duration,
}

impl From<&RetrySettings> for RetryPolicy {
    fn from(settings: &RetrySettings) -> Self {
        Self {
            max_attempts: settings.max_attempts,
            interval: Duration::from_secs(settings.interval),
        }
    }
}

/// 재시도 가능한 작업 특성
#[async_trait::async_trait]
pub trait RetryableOperation {
    type Output;
    
    /// 작업 실행
    async fn execute(&self) -> Result<Self::Output, DockerError>;
    
    /// 재시도 여부 결정
    fn should_retry(&self, error: &DockerError) -> bool {
        error.is_retryable()
    }
}

/// 재시도 로직 실행
pub async fn with_retry<T: RetryableOperation>(
    operation: T,
    policy: RetryPolicy,
) -> Result<T::Output, DockerError> {
    let mut attempts = 0;
    
    loop {
        attempts += 1;
        match operation.execute().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                if attempts >= policy.max_attempts || !operation.should_retry(&error) {
                    return Err(error);
                }
                
                warn!(
                    error = %error,
                    attempt = attempts,
                    max_attempts = policy.max_attempts,
                    "작업 실패, 재시도 예정"
                );
                
                sleep(policy.interval).await;
            }
        }
    }
}

pub struct ContainerRoutesRetry<'a> {
   pub docker_manager: &'a DockerManager,
}

#[async_trait]
impl<'a> RetryableOperation for ContainerRoutesRetry<'a> {
    type Output = HashMap<(String, PathMatcher), BackendService>;

    async fn execute(&self) -> Result<Self::Output, DockerError> {
        self.docker_manager.try_get_container_routes().await
    }
}

