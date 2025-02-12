use super::{Request, Response, MiddlewareError};
use async_trait::async_trait;
use hyper::body::Body;

/// 미들웨어 트레이트
/// 
/// HTTP 요청과 응답을 수정할 수 있는 인터페이스를 정의합니다.
#[async_trait]
pub trait Middleware: Send + Sync {
    /// HTTP 요청을 처리합니다.
    async fn handle_request(&self, req: Request) -> Result<Request, MiddlewareError>;

    /// HTTP 응답을 처리합니다.
    async fn handle_response(&self, res: Response) -> Result<Response, MiddlewareError>;
}

// 제네릭 버전은 별도 트레이트로
#[async_trait]
pub trait GenericMiddleware<B>: Send + Sync 
where
    B: Body + Send + 'static,
{
    async fn handle_request(&self, req: hyper::Request<B>) -> Result<hyper::Request<B>, MiddlewareError>;
    async fn handle_response(&self, res: hyper::Response<B>) -> Result<hyper::Response<B>, MiddlewareError>;
}