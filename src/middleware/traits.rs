// src/middleware/traits.rs
use async_trait::async_trait;
use hyper::{Request, Response, Body};

#[async_trait]
pub trait Middleware: Send + Sync + 'static {
    // 요청 처리
    async fn handle_request(
        &self,
        req: Request<Body>,
    ) -> Result<Request<Body>, MiddlewareError>;

    // 응답 처리
    async fn handle_response(
        &self,
        res: Response<Body>,
    ) -> Result<Response<Body>, MiddlewareError>;

    // 미들웨어 이름 (로깅 및 설정용)
    fn name(&self) -> &str;
}