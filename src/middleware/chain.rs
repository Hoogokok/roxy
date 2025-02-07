use super::{Middleware, Request, Response, MiddlewareError};
use std::sync::Arc;
use tracing::{debug, error};

/// 미들웨어 체인
/// 
/// 여러 미들웨어를 순서대로 실행합니다.
#[derive(Default)]
pub struct MiddlewareChain {
    middlewares: Vec<Arc<dyn Middleware>>,
}

impl MiddlewareChain {
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }

    /// 미들웨어를 체인에 추가합니다.
    pub fn add<M>(&mut self, middleware: M) 
    where
        M: Middleware + 'static,
    {
        self.middlewares.push(Arc::new(middleware));
    }

    /// 요청 체인을 실행합니다.
    pub async fn handle_request(&self, mut req: Request) -> Result<Request, MiddlewareError> {
        for middleware in &self.middlewares {
            debug!(middleware = middleware.name(), "요청 처리 시작");
            match middleware.handle_request(req).await {
                Ok(new_req) => {
                    debug!(middleware = middleware.name(), "요청 처리 완료");
                    req = new_req;
                }
                Err(e) => {
                    error!(
                        middleware = middleware.name(),
                        error = %e,
                        "요청 처리 실패"
                    );
                    return Err(e);
                }
            }
        }
        Ok(req)
    }

    /// 응답 체인을 실행합니다.
    pub async fn handle_response(&self, mut res: Response) -> Result<Response, MiddlewareError> {
        // 응답은 역순으로 처리
        for middleware in self.middlewares.iter().rev() {
            debug!(middleware = middleware.name(), "응답 처리 시작");
            match middleware.handle_response(res).await {
                Ok(new_res) => {
                    debug!(middleware = middleware.name(), "응답 처리 완료");
                    res = new_res;
                }
                Err(e) => {
                    error!(
                        middleware = middleware.name(),
                        error = %e,
                        "응답 처리 실패"
                    );
                    return Err(e);
                }
            }
        }
        Ok(res)
    }
} 