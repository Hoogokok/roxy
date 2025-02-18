use hyper::{Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;
use super::MiddlewareError;

/// 미들웨어 에러를 HTTP 응답으로 변환합니다.
pub fn handle_middleware_error(err: MiddlewareError) -> Response<Full<Bytes>> {
    let (status, message) = match err {
        MiddlewareError::Config { message } => {
            (StatusCode::INTERNAL_SERVER_ERROR, message)
        }
        MiddlewareError::Runtime { message, .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, message)
        }
        MiddlewareError::InvalidAuth(message) => {
            (StatusCode::UNAUTHORIZED, message)
        }
        MiddlewareError::InvalidFormat(message) => {
            (StatusCode::BAD_REQUEST, message)
        }
        MiddlewareError::InvalidLabel { key, value, reason } => {
            // 라벨 파싱 에러는 서버 설정 오류이므로 500 반환
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("라벨 설정 오류 - key: {}, value: {}, 원인: {}", key, value, reason)
            )
        }
        MiddlewareError::InvalidRequest(message) => {
            (StatusCode::BAD_REQUEST, message)
        }
        MiddlewareError::PreflightResponse(response) => {
            return response;
        }
    };

    Response::builder()
        .status(status)
        .body(Full::new(Bytes::from(message)))
        .unwrap()
} 