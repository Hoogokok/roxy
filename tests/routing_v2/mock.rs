use hyper::body::Body;
use hyper::{Request, Response};
use std::sync::Arc;
use tokio::sync::Mutex;
use bytes::Bytes;
use std::pin::Pin;
use std::task::{Context, Poll};
use http_body::Frame;
use std::sync::atomic::{AtomicUsize, Ordering};

// Mock Body 구현
struct MockBody {
    data: &'static [u8],
}

impl MockBody {
    fn new(data: &'static [u8]) -> Self {
        Self { data }
    }
}

impl Body for MockBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if self.data.is_empty() {
            Poll::Ready(None)
        } else {
            let data = self.data;
            self.data = &[];
            Poll::Ready(Some(Ok(Frame::data(Bytes::from(data)))))
        }
    }
}

type BodyType = Box<dyn Body<Data = Bytes, Error = hyper::Error>>;

// 요청 카운터만 저장
#[derive(Default)]
pub struct RequestLog {
    pub request_count: AtomicUsize,
    pub response_body: &'static str,
}

// Mock 서버 구현
pub struct MockServer {
    log: Arc<Mutex<RequestLog>>,
}

impl MockServer {
    pub fn new() -> Self {
        Self {
            log: Arc::new(Mutex::new(RequestLog::default())),
        }
    }

    pub async fn with_response(response: &'static str) -> Self {
        let server = Self::new();
        {
            let mut log = server.log.lock().await;
            log.response_body = response;
        }  // lock is dropped here
        server
    }

    pub async fn request_count(&self) -> usize {
        self.log.lock().await.request_count.load(Ordering::Relaxed)
    }

    pub async fn handle_request(&self, request: Request<BodyType>) -> Result<Response<BodyType>, hyper::Error> {
        let log = self.log.lock().await;
        log.request_count.fetch_add(1, Ordering::Relaxed);
        
        let body = MockBody::new(log.response_body.as_bytes());
        Ok(Response::new(Box::new(body)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_empty_request() -> Request<BodyType> {
        let mock_body = MockBody::new(b"");
        let boxed_body: BodyType = Box::new(mock_body);
        Request::new(boxed_body)
    }

    #[tokio::test]
    async fn test_mock_server_basic() {
        let server = MockServer::with_response("hello").await;
        let request = create_empty_request();
        
        let response = server.handle_request(request).await.unwrap();
        assert_eq!(server.request_count().await, 1);
        
        let body = response.into_body();
    }

    #[tokio::test]
    async fn test_mock_server_multiple_requests() {
        let server = MockServer::with_response("test").await;
        
        for _ in 0..3 {
            let request = create_empty_request();
            let _ = server.handle_request(request).await.unwrap();
        }

        assert_eq!(server.request_count().await, 3);
    }
} 