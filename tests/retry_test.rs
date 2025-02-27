use reverse_proxy_traefik::docker::{
    with_retry, DockerError, RetryPolicy, RetryableOperation
};
use reverse_proxy_traefik::settings::docker::RetrySettings;
use std::time::Duration;

#[tokio::test]
async fn test_retry_policy_from_settings() {
    let settings = RetrySettings {
        max_attempts: 3,
        interval: 1,
    };
    
    let policy = RetryPolicy::from(&settings);
    assert_eq!(policy.max_attempts, 3);
    assert_eq!(policy.interval, Duration::from_secs(1));
}

#[tokio::test]
async fn test_retry_with_success_after_failure() {
    struct TestOperation {
        attempts: std::sync::atomic::AtomicU32,
    }

    #[async_trait::async_trait]
    impl RetryableOperation for TestOperation {
        type Output = u32;

        async fn execute(&self) -> Result<Self::Output, DockerError> {
            let attempt = self.attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if attempt < 2 {
                Err(DockerError::ConnectionError {
                    source: bollard::errors::Error::IOError {
                        err: std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused")
                    },
                    context: "test".to_string(),
                })
            } else {
                Ok(attempt)
            }
        }
    }

    let operation = TestOperation {
        attempts: std::sync::atomic::AtomicU32::new(0),
    };

    let policy = RetryPolicy {
        max_attempts: 3,
        interval: Duration::from_millis(100),
    };

    let result = with_retry(operation, policy).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 2);
} 