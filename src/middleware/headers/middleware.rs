use crate::middleware::{Middleware, Request, Response, MiddlewareError};
use super::config::HeadersConfig;
use async_trait::async_trait;

/// 헤더 수정 미들웨어
pub struct HeadersMiddleware {
    name: String,
    config: HeadersConfig,
}

impl HeadersMiddleware {
    pub fn new(name: String, config: HeadersConfig) -> Self {
        Self { name, config }
    }
}

#[async_trait]
impl Middleware for HeadersMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    async fn handle_request(&self, req: Request) -> Result<Request, MiddlewareError> {
        // TODO: 요청 헤더 수정 구현
        Ok(req)
    }

    async fn handle_response(&self, res: Response) -> Result<Response, MiddlewareError> {
        // TODO: 응답 헤더 수정 구현
        Ok(res)
    }
} 