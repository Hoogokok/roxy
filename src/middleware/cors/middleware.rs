use crate::middleware::{Middleware, MiddlewareError, Request, Response};
use super::config::CorsConfig;
use async_trait::async_trait;
use hyper::{header, Method, HeaderMap};
use tracing::{debug, instrument};
use http_body_util::Full;
use bytes::Bytes;

/// CORS 미들웨어
#[derive(Debug)]
pub struct CorsMiddleware {
    config: CorsConfig,
}

impl CorsMiddleware {
    pub fn new(config: CorsConfig) -> Self {
        Self { config }
    }

    /// Origin 검증
    fn validate_origin(&self, origin: &str) -> bool {
        self.config.allow_origins.iter().any(|allowed| {
            if allowed == "*" {
                true
            } else {
                allowed == origin
            }
        })
    }

    /// CORS 헤더 설정
    fn set_cors_headers(&self, headers: &mut HeaderMap, origin: &str) {
        // Access-Control-Allow-Origin
        if self.validate_origin(origin) {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                origin.parse().unwrap()
            );
        }

        // Access-Control-Allow-Credentials
        if self.config.allow_credentials {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                "true".parse().unwrap()
            );
        }

        // Access-Control-Expose-Headers
        if !self.config.expose_headers.is_empty() {
            headers.insert(
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                self.config.expose_headers.join(", ").parse().unwrap()
            );
        }
    }

    /// Preflight 요청 처리
    fn handle_preflight(&self, req: &Request) -> Result<Response, MiddlewareError> {
        let origin = req.headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| MiddlewareError::InvalidRequest("Missing origin header".into()))?;

        if !self.validate_origin(origin) {
            return Err(MiddlewareError::InvalidRequest("Origin not allowed".into()));
        }

        let mut response = Response::new(Full::new(Bytes::from("")));
        let headers = response.headers_mut();

        // 기본 CORS 헤더 설정
        self.set_cors_headers(headers, origin);

        // Access-Control-Allow-Methods
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            self.config.allow_methods.join(", ").parse().unwrap()
        );

        // Access-Control-Allow-Headers
        if !self.config.allow_headers.is_empty() {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                self.config.allow_headers.join(", ").parse().unwrap()
            );
        }

        // Access-Control-Max-Age
        if let Some(max_age) = self.config.max_age {
            headers.insert(
                header::ACCESS_CONTROL_MAX_AGE,
                max_age.to_string().parse().unwrap()
            );
        }

        Ok(response)
    }
}

#[async_trait]
impl Middleware for CorsMiddleware {
    #[instrument(skip(self, req))]
    async fn handle_request(&self, req: Request) -> Result<Request, MiddlewareError> {
        // OPTIONS 요청은 preflight로 처리
        if req.method() == Method::OPTIONS {
            debug!("Handling CORS preflight request");
            return Err(MiddlewareError::PreflightResponse(self.handle_preflight(&req)?));
        }

        // 일반 요청의 Origin 검증
        if let Some(origin) = req.headers().get(header::ORIGIN) {
            debug!(?origin, "Validating CORS request origin");
            if !self.validate_origin(origin.to_str().unwrap_or("")) {
                return Err(MiddlewareError::InvalidRequest("Origin not allowed".into()));
            }
        }

        Ok(req)
    }

    #[instrument(skip(self, res))]
    async fn handle_response(&self, mut res: Response) -> Result<Response, MiddlewareError> {
        let origin = res.headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        
        if let Some(origin) = origin {
            debug!(?origin, "Setting CORS response headers");
            self.set_cors_headers(res.headers_mut(), &origin);
        }
        
        Ok(res)
    }
} 