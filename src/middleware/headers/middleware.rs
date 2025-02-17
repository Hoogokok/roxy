use crate::middleware::{Middleware, MiddlewareError, Request, Response};
use super::config::HeadersConfig;
use async_trait::async_trait;
use hyper::header::{HeaderName, HeaderValue};
use tracing::{debug, instrument};

/// 헤더 수정 미들웨어
#[derive(Debug)]
pub struct HeadersMiddleware {
    config: HeadersConfig,
}

impl HeadersMiddleware {
    pub fn new(config: HeadersConfig) -> Self {
        Self {
            config,
        }
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
    #[instrument(skip(self, req), fields(req_headers = ?req.headers()))]
    async fn handle_request(&self, mut req: Request) -> Result<Request, MiddlewareError> {
        debug!("헤더 요청 헤더 처리 시작: {:?}", self.config.request);
        // request HeaderModification 사용
        self.config.request.apply_to_headers(req.headers_mut());
        debug!("요청 헤더 수정 완료: {:?}", req.headers());
        Ok(req)
    }

    #[instrument(skip(self, res), fields(res_headers = ?res.headers()))]
    async fn handle_response(&self, mut res: Response) -> Result<Response, MiddlewareError> {
        debug!(config = ?self.config, "응답 헤더 처리 시작");
        
        // 기본 보안 헤더 적용
        self.apply_security_headers(res.headers_mut());
        
        // response HeaderModification 사용
        self.config.response.apply_to_headers(res.headers_mut());
        
        debug!(modified_headers = ?res.headers(), "응답 헤더 수정 완료");
        Ok(res)
    }
} 