use std::fmt;

use super::BoxError;

#[derive(Debug)]
pub enum MiddlewareError {
    /// 미들웨어 설정 오류
    Config {
        middleware: String,
        message: String,
    },
    /// 미들웨어 실행 중 오류
    Runtime {
        middleware: String,
        message: String,
        source: Option<BoxError>,
    },
}

impl fmt::Display for MiddlewareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config { middleware, message } => {
                write!(f, "미들웨어 {} 설정 오류: {}", middleware, message)
            }
            Self::Runtime { middleware, message, .. } => {
                write!(f, "미들웨어 {} 실행 오류: {}", middleware, message)
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