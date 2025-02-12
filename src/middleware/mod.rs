//! 미들웨어 프레임워크 모듈
//! 
//! HTTP 요청/응답을 처리하는 미들웨어 체인을 구현합니다.

mod chain;
pub mod config;
mod error;
mod traits;
pub mod headers;
pub mod basic_auth;

pub use chain::MiddlewareChain;
pub use config::MiddlewareConfig;
use config::MiddlewareType;
pub use error::MiddlewareError;
pub use traits::Middleware;

// 재사용 가능한 타입 별칭
pub type Request<B = hyper::body::Incoming> = hyper::Request<B>;
pub type Response<B = http_body_util::Full<bytes::Bytes>> = hyper::Response<B>;
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

use crate::middleware::basic_auth::{BasicAuthConfig, BasicAuthMiddleware, create_authenticator};
use crate::middleware::headers::{HeadersConfig, HeadersMiddleware};

/// 미들웨어 설정으로부터 미들웨어 인스턴스를 생성합니다.
pub fn create_middleware(config: &MiddlewareConfig) -> Result<Box<dyn Middleware>, MiddlewareError> {
    match config.middleware_type {
        MiddlewareType::BasicAuth => {
            // settings를 BasicAuthConfig로 변환
            let auth_config: BasicAuthConfig = serde_json::from_value(
                serde_json::to_value(&config.settings)?
            )?;
            
            // 인증기 생성
            let authenticator = create_authenticator(&auth_config)?;
            
            // 미들웨어 생성
            Ok(Box::new(BasicAuthMiddleware::new(
                auth_config,
                authenticator,
            )))
        }
        MiddlewareType::Headers => {
            // settings를 HeadersConfig로 변환
            let headers_config: HeadersConfig = serde_json::from_value(
                serde_json::to_value(&config.settings)?
            )?;
            
            // 미들웨어 생성
            Ok(Box::new(HeadersMiddleware::new(
                headers_config,
            )))
        }
    }
}