use crate::middleware::{Middleware, MiddlewareError, Request, Response};
use super::{config::RateLimitConfig, store::RateLimitStore};
use async_trait::async_trait;
use hyper::StatusCode;
use http_body_util::Full;
use bytes::Bytes;
use tracing::debug;

/// Rate Limit 미들웨어
pub struct RateLimitMiddleware<S: RateLimitStore> {
    config: RateLimitConfig,
    store: S,
}

impl<S: RateLimitStore> RateLimitMiddleware<S> {
    pub fn new(config: RateLimitConfig, store: S) -> Self {
        Self { config, store }
    }

    /// 클라이언트 식별자를 추출합니다.
    fn get_client_id(req: &Request) -> String {
        // X-Forwarded-For 헤더 확인
        if let Some(forwarded) = req.headers()
            .get("x-forwarded-for")
            .and_then(|h| h.to_str().ok())
        {
            if let Some(ip) = forwarded.split(',').next() {
                return ip.trim().to_string();
            }
        }

        // X-Real-IP 헤더 확인
        if let Some(real_ip) = req.headers()
            .get("x-real-ip")
            .and_then(|h| h.to_str().ok())
        {
            return real_ip.to_string();
        }

        // 헤더가 없는 경우 기본값 사용
        "unknown".to_string()
    }

    /// Rate Limit 초과 응답을 생성합니다.
    async fn create_limit_exceeded_response(&self, key: &str) -> Response {
        let wait_time = self.store.time_to_next_request(key).await
            .unwrap_or_default();

        Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("X-RateLimit-Limit", self.config.average.to_string())
            .header("X-RateLimit-Reset", wait_time.as_secs().to_string())
            .header("Retry-After", wait_time.as_secs().to_string())
            .body(Full::new(Bytes::from("Rate limit exceeded")))
            .unwrap()
    }
}

#[async_trait]
impl<S: RateLimitStore> Middleware for RateLimitMiddleware<S> {
    async fn handle_request(&self, req: Request) -> Result<Request, MiddlewareError> {
        let client_id = Self::get_client_id(&req);
        debug!("Rate limit check for client: {}", client_id);

        if self.store.check_rate(
            &client_id,
            self.config.average as f64,
            self.config.burst as f64
        ).await {
            Ok(req)
        } else {
            let response = self.create_limit_exceeded_response(&client_id).await;
            Err(MiddlewareError::TooManyRequests(response))
        }
    }

    async fn handle_response(&self, res: Response) -> Result<Response, MiddlewareError> {
        Ok(res)
    }
} 