//! 미들웨어 프레임워크 모듈
//! 
//! HTTP 요청/응답을 처리하는 미들웨어 체인을 구현합니다.

mod chain;
pub mod config;
mod error;
mod traits;
pub mod headers;
pub mod basic_auth;
mod manager;
mod response;
pub mod parser;
mod cors;
mod rate_limit;

pub use chain::MiddlewareChain;
pub use config::MiddlewareConfig;
pub use error::MiddlewareError;
pub use traits::Middleware;
pub use manager::MiddlewareManager;

// 재사용 가능한 타입 별칭
pub type Request<B = hyper::body::Incoming> = hyper::Request<B>;
pub type Response<B = http_body_util::Full<bytes::Bytes>> = hyper::Response<B>;

pub use response::handle_middleware_error;
pub use parser::{HeaderParser, ParserError};