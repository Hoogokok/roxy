use super::{Middleware, Request, Response, MiddlewareError};
use std::sync::Arc;
use tracing::{debug, error};
use std::any::Any;

/// 미들웨어 체인
/// 
/// 여러 미들웨어를 순서대로 실행합니다.
#[derive(Default, Clone)]
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
    pub fn add_boxed(&mut self, middleware: Box<dyn Middleware>) {
        let arc: Arc<dyn Middleware> = Arc::from(middleware);
        let type_id = arc.as_ref().type_id();
        
        // 이미 존재하는 같은 타입의 미들웨어가 있다면 제거
        self.middlewares.retain(|m| m.as_ref().type_id() != type_id);
        self.middlewares.push(arc);
    }

    /// 요청 체인을 실행합니다.
    pub async fn handle_request(&self, mut req: Request) -> Result<Request, MiddlewareError> {
        debug!("미들웨어 체인 요청 처리 시작 - 미들웨어 수: {}", self.middlewares.len());
        for (index, middleware) in self.middlewares.iter().enumerate() {
            debug!("요청 미들웨어 실행 #{} - 타입: {:?}", index, std::any::type_name::<dyn Middleware>());
            req = middleware.handle_request(req).await?;
        }
        debug!("미들웨어 체인 요청 처리 완료");
        Ok(req)
    }

    /// 응답 체인을 실행합니다.
    pub async fn handle_response(&self, mut res: Response) -> Result<Response, MiddlewareError> {
        debug!("미들웨어 체인 응답 처리 시작 - 미들웨어 수: {}", self.middlewares.len());
        // 응답은 역순으로 처리
        for (index, middleware) in self.middlewares.iter().rev().enumerate() {
            debug!("응답 미들웨어 실행 #{} - 타입: {:?}", index, std::any::type_name::<dyn Middleware>());
            res = middleware.handle_response(res).await?;
        }
        debug!("미들웨어 체인 응답 처리 완료 - 최종 헤더: {:?}", res.headers());
        Ok(res)
    }

    pub fn middleware_count(&self) -> usize {
        self.middlewares.len()
    }
} 