use std::fmt;

use super::BoxError;
use super::parser::ParserError;

#[derive(Debug)]
pub enum MiddlewareError {
    /// 미들웨어 설정 오류
    Config {
        message: String,
    },
    /// 미들웨어 실행 중 오류
    Runtime {
        message: String,
        source: Option<BoxError>,
    },
    InvalidAuth(String),
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