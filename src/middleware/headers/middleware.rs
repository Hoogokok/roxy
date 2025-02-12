use crate::middleware::{Middleware, Request, Response, MiddlewareError};
use super::config::HeadersConfig;
use async_trait::async_trait;
use hyper::header::{HeaderName, HeaderValue};

/// 헤더 수정 미들웨어
pub struct HeadersMiddleware {
    config: HeadersConfig,
}

impl HeadersMiddleware {
    pub fn new(config: HeadersConfig) -> Self {
        Self {config }
    }

    /// 기본 보안 헤더를 설정합니다.
    fn apply_security_headers(&self, headers: &mut hyper::HeaderMap) {
        // X-Frame-Options: DENY - 클릭재킹 방지
        headers.insert(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY")
        );

        // X-Content-Type-Options: nosniff - MIME 타입 스니핑 방지
        headers.insert(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff")
        );

        // Strict-Transport-Security - HTTPS 강제
        headers.insert(
            HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static("max-age=31536000")
        );
    }
}

#[async_trait]
impl Middleware for HeadersMiddleware {

    async fn handle_request(&self, mut req: Request) -> Result<Request, MiddlewareError> {
        // 요청 헤더 수정
        self.config.request.apply_to_headers(req.headers_mut());
        Ok(req)
    }

    async fn handle_response(&self, mut res: Response) -> Result<Response, MiddlewareError> {
        // 기본 보안 헤더 적용
        self.apply_security_headers(res.headers_mut());
        
        // 응답 헤더 수정
        self.config.response.apply_to_headers(res.headers_mut());
        Ok(res)
    }
} 