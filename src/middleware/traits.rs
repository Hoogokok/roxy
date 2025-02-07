use super::{Request, Response, MiddlewareError};
use async_trait::async_trait;

/// 미들웨어 트레이트
/// 
/// HTTP 요청과 응답을 수정할 수 있는 인터페이스를 정의합니다.
#[async_trait]
pub trait Middleware: Send + Sync {
    /// 미들웨어의 고유 이름을 반환합니다.
    fn name(&self) -> &str;

    /// HTTP 요청을 처리합니다.
    async fn handle_request(&self, req: Request) -> Result<Request, MiddlewareError>;

    /// HTTP 응답을 처리합니다.
    async fn handle_response(&self, res: Response) -> Result<Response, MiddlewareError>;
}