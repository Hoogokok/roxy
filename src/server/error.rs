use crate::{docker::DockerError, settings::SettingsError};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    /// 설정 관련 오류
    Config {
        /// 오류 메시지
        message: String,
        /// 관련 파일 경로 (선택적)
        file_path: Option<PathBuf>,
        /// 원인 오류 (선택적)
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    /// 파일 시스템 I/O 오류
    Io {
        /// 오류 메시지
        message: String,
        /// 원인 오류
        source: std::io::Error,
    },
    
    /// Docker 관련 오류
    Docker {
        /// 오류 메시지
        message: String,
        /// 원인 오류
        source: DockerError,
    },
    
    /// 설정 파일 감시 오류
    ConfigWatch {
        /// 오류 메시지
        message: String,
        /// 관련 파일 경로 (선택적)
        file_path: Option<PathBuf>,
        /// 원인 오류 (선택적)
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    /// 서버 실행 오류
    Server {
        /// 오류 메시지
        message: String,
        /// 원인 오류 (선택적)
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    /// 미들웨어 관련 오류
    Middleware {
        /// 오류 메시지
        message: String,
        /// 미들웨어 ID (선택적)
        middleware_id: Option<String>,
        /// 원인 오류 (선택적)
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    /// 기타 오류
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io { 
            message: err.to_string(),
            source: err,
        }
    }
}

impl From<DockerError> for Error {
    fn from(err: DockerError) -> Self {
        Error::Docker {
            message: err.to_string(),
            source: err,
        }
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for Error {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Error::Other(err)
    }
}

impl From<SettingsError> for Error {
    fn from(err: SettingsError) -> Self {
        Error::Config {
            message: err.to_string(),
            file_path: None,
            source: Some(Box::new(err)),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Config { message, file_path, .. } => {
                if let Some(path) = file_path {
                    write!(f, "설정 오류 (파일: {}): {}", path.display(), message)
                } else {
                    write!(f, "설정 오류: {}", message)
                }
            },
            Error::Io { message, .. } => write!(f, "I/O 오류: {}", message),
            Error::Docker { message, .. } => write!(f, "Docker 오류: {}", message),
            Error::ConfigWatch { message, file_path, .. } => {
                if let Some(path) = file_path {
                    write!(f, "설정 감시 오류 (파일: {}): {}", path.display(), message)
                } else {
                    write!(f, "설정 감시 오류: {}", message)
                }
            },
            Error::Server { message, .. } => write!(f, "서버 오류: {}", message),
            Error::Middleware { message, middleware_id, .. } => {
                if let Some(id) = middleware_id {
                    write!(f, "미들웨어 오류 (ID: {}): {}", id, message)
                } else {
                    write!(f, "미들웨어 오류: {}", message)
                }
            },
            Error::Other(e) => write!(f, "오류: {}", e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            Error::Docker { source, .. } => Some(source),
            Error::Config { source, .. } => source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static)),
            Error::ConfigWatch { source, .. } => source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static)),
            Error::Server { source, .. } => source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static)),
            Error::Middleware { source, .. } => source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static)),
            Error::Other(e) => Some(e.as_ref()),
        }
    }
}