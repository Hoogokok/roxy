use crate::middleware::{Middleware, Request, Response, MiddlewareError};
use super::config::BasicAuthConfig;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hyper::{header, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use super::auth::Authenticator;

/// Basic 인증 미들웨어
pub struct BasicAuthMiddleware {
    config: BasicAuthConfig,
    authenticator: Box<dyn Authenticator>,
}

impl BasicAuthMiddleware {
    pub fn new(config: BasicAuthConfig, authenticator: Box<dyn Authenticator>) -> Self {
        Self { 
            config,
            authenticator,
        }
    }

    /// Authorization 헤더에서 자격증명을 추출합니다.
    fn extract_credentials(&self, req: &Request) -> Option<(String, String)> {
        req.headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|auth| {
                if !auth.starts_with("Basic ") {
                    return None;
                }
                let credentials = auth.trim_start_matches("Basic ").trim();
                BASE64.decode(credentials).ok()
            })
            .and_then(|decoded| String::from_utf8(decoded).ok())
            .and_then(|pair| {
                let mut parts = pair.splitn(2, ':');
                Some((
                    parts.next()?.to_string(),
                    parts.next()?.to_string(),
                ))
            })
    }

    /// 401 Unauthorized 응답을 생성합니다.
    fn unauthorized_response(&self) -> Response {
        Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(
                header::WWW_AUTHENTICATE,
                format!("Basic realm=\"{}\"", self.config.realm)
            )
            .body(Full::new(Bytes::from("Unauthorized")))
            .unwrap()
    }
}

#[async_trait]
impl Middleware for BasicAuthMiddleware {

    async fn handle_request(&self, req: Request) -> Result<Request, MiddlewareError> {
        // 자격증명 추출
        match self.extract_credentials(&req) {
            Some((username, password)) => {
                // 변경: 인증기를 통한 검증
                if self.authenticator.verify_credentials(&username, &password) {
                    Ok(req)
                } else {
                    Err(MiddlewareError::Runtime {
                        message: "Invalid credentials".to_string(),
                        source: None,
                    })
                }
            }
            None => {
                Err(MiddlewareError::Runtime {
                    message: "Missing or invalid Authorization header".to_string(),
                    source: None,
                })
            }
        }
    }

    async fn handle_response(&self, res: Response) -> Result<Response, MiddlewareError> {
        // 인증 실패 시 401 응답 반환
        if !res.status().is_success() {
            Ok(self.unauthorized_response())
        } else {
            Ok(res)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::middleware::basic_auth::create_authenticator;

    use super::*;
    use std::collections::HashMap;

    fn create_test_middleware() -> BasicAuthMiddleware {
        let mut users = HashMap::new();
        users.insert(
            "test".to_string(),
            "$apr1$H6uskkkW$IgXLP6ewTrSuBkTrqE8wj/".to_string()
        );

        let config = BasicAuthConfig {
            users,
            realm: "Test Realm".to_string(),
            ..Default::default()
        };

        let authenticator = create_authenticator(&config).unwrap();
        BasicAuthMiddleware::new(config, authenticator)
    }

    #[tokio::test]
    async fn test_unauthorized_response() {
        let middleware = create_test_middleware();
        let response = middleware.unauthorized_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.headers().contains_key(header::WWW_AUTHENTICATE));
    }
}
