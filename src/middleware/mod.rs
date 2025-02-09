//! 미들웨어 프레임워크 모듈
//! 
//! HTTP 요청/응답을 처리하는 미들웨어 체인을 구현합니다.

mod chain;
mod config;
mod error;
mod traits;
pub mod headers;

pub use chain::MiddlewareChain;
pub use config::MiddlewareConfig;
pub use error::MiddlewareError;
pub use traits::Middleware;

// 재사용 가능한 타입 별칭
pub type Request = hyper::Request<hyper::body::Incoming>;
pub type Response = hyper::Response<http_body_util::Full<bytes::Bytes>>;
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;