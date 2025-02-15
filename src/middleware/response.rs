use hyper::{Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use super::MiddlewareError;

/// 미들웨어 에러를 HTTP 응답으로 변환합니다.
pub fn handle_middleware_error(err: MiddlewareError) -> Response<Full<Bytes>> {
    let status = match &err {
        MiddlewareError::Config { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        MiddlewareError::Runtime { .. } => StatusCode::UNAUTHORIZED,
        MiddlewareError::InvalidAuth(_) => StatusCode::UNAUTHORIZED,
        MiddlewareError::InvalidFormat(_) => StatusCode::BAD_REQUEST,
    };

    Response::builder()
        .status(status)
        .body(Full::new(Bytes::from(err.to_string())))
        .unwrap_or_else(|_| {
            Response::new(Full::new(Bytes::from("Internal Server Error")))
        })
} 