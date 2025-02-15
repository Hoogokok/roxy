use crate::middleware::{HeaderParser, Middleware, MiddlewareError, Request, Response};
use super::config::HeadersConfig;
use async_trait::async_trait;
use hyper::header::{HeaderName, HeaderValue};

/// 헤더 수정 미들웨어
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

    /// 복잡한 헤더 값을 파싱하고 처리
    fn process_header(&mut self, name: &HeaderName, value: &HeaderValue) -> Result<Vec<(String, String)>, MiddlewareError> {
        // 1. 헤더 값이 여러 줄인 경우 (예: Set-Cookie)
        let headers = self.parser.parse_streaming(value.as_bytes())?;
        
        // 2. 각 헤더 라인 처리
        let mut processed = Vec::new();
        for (_, header_value) in headers {
            // 예: "key=value; expires=date; path=/"와 같은 형식 파싱
            if let Some(parsed) = self.parse_complex_value(&header_value) {
                processed.push(parsed);
            }
        }

        Ok(processed)
    }

    /// 복잡한 헤더 값 파싱 (예: Cookie, Set-Cookie)
    fn parse_complex_value(&self, value: &str) -> Option<(String, String)> {
        // 구체적인 파싱 로직
        // 예: "key=value; expires=date"를 파싱하여 key와 value 추출
        None
    }

    fn process_cookie_header(&mut self, value: &HeaderValue) -> Result<Vec<(String, String)>, MiddlewareError> {
        let headers = self.parser.parse_streaming(value.as_bytes())?;
        
        let mut cookies = Vec::new();
        for (_, cookie_str) in headers {
            // "key=value; path=/" 형식의 쿠키 파싱
            if let Some((name, value)) = cookie_str.split_once('=') {
                if let Some(value) = value.split(';').next() {
                    cookies.push((name.trim().to_string(), value.trim().to_string()));
                }
            }
        }
        
        Ok(cookies)
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