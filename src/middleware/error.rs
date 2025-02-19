use std::fmt;

use super::parser::ParserError;
use hyper::Response;
use http_body_util::Full;
use bytes::Bytes;

#[derive(Debug)]
pub enum MiddlewareError {
    /// 미들웨어 설정 오류
    Config {
        message: String,
    },
    /// 미들웨어 실행 중 오류
    Runtime {
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    InvalidAuth(String),
    InvalidFormat(String),
    InvalidLabel {
        key: String,
        value: String,
        reason: String,
    },
    InvalidRequest(String),
    PreflightResponse(Response<Full<Bytes>>),
    /// Rate limit 초과 에러
    TooManyRequests(Response<Full<Bytes>>),
}

impl fmt::Display for MiddlewareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config { message } => {
                write!(f, "설정 오류: {}", message)
            }
            Self::Runtime { message, .. } => {
                write!(f, "실행 오류: {}", message)
            }
            Self::InvalidAuth(message) => {
                write!(f, "인증 오류: {}", message)
            }
            Self::InvalidFormat(message) => {
                write!(f, "형식 오류: {}", message)
            }
            Self::InvalidLabel { key, value, reason } => {
                write!(f, "라벨 오류: key={}, value={}, reason={}", key, value, reason)
            }
            Self::InvalidRequest(message) => {
                write!(f, "요청 오류: {}", message)
            }
            Self::PreflightResponse(_) => {
                write!(f, "Preflight 응답 오류")
            }
            Self::TooManyRequests(_) => {
                write!(f, "Rate limit exceeded")
            }
        }
    }
}

impl std::error::Error for MiddlewareError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Runtime { source: Some(err), .. } => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for MiddlewareError {
    fn from(err: serde_json::Error) -> Self {
        MiddlewareError::Config {
            message: err.to_string(),
        }
    }
}

impl From<ParserError> for MiddlewareError {
    fn from(err: ParserError) -> Self {
        MiddlewareError::InvalidAuth(err.to_string())
    }
} 