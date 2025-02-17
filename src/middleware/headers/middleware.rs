use crate::middleware::{HeaderParser, Middleware, MiddlewareError, Request, Response};
use super::config::HeadersConfig;
use async_trait::async_trait;
use hyper::header::{HeaderName, HeaderValue};
use tracing::{debug, instrument};

/// 헤더 수정 미들웨어
#[derive(Debug)]
pub struct HeadersMiddleware {
    config: HeadersConfig,
    parser: HeaderParser,
}

impl HeadersMiddleware {
    pub fn new(config: HeadersConfig) -> Self {
        Self {
            config,
            parser: HeaderParser::new(),
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

    /// 복잡한 헤더 값 파싱 (예: Cookie, Set-Cookie)
    fn parse_complex_value(&self, value: &str) -> Option<(String, String)> {
        // 1. key=value 형식 파싱
        let mut parts = value.splitn(2, '=');
        let key = parts.next()?.trim();
        let value = parts.next()?.split(';').next()?.trim();
        
        // 2. 유효성 검사
        if key.is_empty() || value.is_empty() {
            return None;
        }

        Some((key.to_string(), value.to_string()))
    }

    /// 헤더 값 처리
    fn process_header(&mut self, name: &HeaderName, value: &HeaderValue) -> Result<Vec<(String, String)>, MiddlewareError> {
        match name.as_str() {
            // Cookie 헤더 처리
            "cookie" => self.process_cookie_header(value),
            
            // Set-Cookie 헤더 처리
            "set-cookie" => self.process_set_cookie_header(value),
            
            // 기타 헤더는 단순 처리
            _ => Ok(vec![(
                name.as_str().to_string(),
                value.to_str()
                    .map_err(|_| MiddlewareError::InvalidFormat("Invalid header value".into()))?
                    .to_string()
            )])
        }
    }

    /// Cookie 헤더 처리
    fn process_cookie_header(&mut self, value: &HeaderValue) -> Result<Vec<(String, String)>, MiddlewareError> {
        let headers = self.parser.parse_streaming(value.as_bytes())?;
        
        let mut cookies = Vec::new();
        for (_, cookie_str) in headers {
            // "key1=value1; key2=value2" 형식 파싱
            for pair in cookie_str.split(';') {
                if let Some((key, value)) = self.parse_complex_value(pair) {
                    cookies.push((key, value));
                }
            }
        }
        
        Ok(cookies)
    }

    /// Set-Cookie 헤더 처리
    fn process_set_cookie_header(&mut self, value: &HeaderValue) -> Result<Vec<(String, String)>, MiddlewareError> {
        let headers = self.parser.parse_streaming(value.as_bytes())?;
        
        let mut cookies = Vec::new();
        for (_, cookie_str) in headers {
            // "key=value; expires=date; path=/" 형식 파싱
            if let Some((key, value)) = self.parse_complex_value(&cookie_str) {
                cookies.push((key, value));
            }
        }
        
        Ok(cookies)
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