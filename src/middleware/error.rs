
#[derive(Debug, thiserror::Error)]
pub enum MiddlewareError {
    #[error("설정 오류: {0}")]
    Config(String),

    #[error("처리 오류: {0}")]
    Processing(String),

    #[error("미들웨어 {middleware} 실행 실패: {message}")]
    Execution {
        middleware: String,
        message: String,
    },

    #[error(transparent)]
    Hyper(#[from] hyper::Error),
}