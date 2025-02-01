use hyper::{Request, Response, StatusCode, Uri};
use hyper::body::{Incoming, Bytes};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use crate::routing::BackendService;

// 프록시 요청을 위한 불변 설정 구조체
#[derive(Clone)]
pub struct ProxyConfig {
    client: legacy::Client<HttpConnector, hyper::body::Incoming>,
}

impl ProxyConfig {
    pub fn new() -> Self {
        let connector = HttpConnector::new();
        let client = legacy::Client::builder(TokioExecutor::new())
            .build::<_, hyper::body::Incoming>(connector);
        
        Self { client }
    }
}

// 순수 함수들: 입력을 받아서 새로운 값을 반환
pub async fn proxy_request(
    config: &ProxyConfig,
    backend: &BackendService,
    req: Request<Incoming>,
) -> Response<Full<Bytes>> {
    // Round-Robin으로 다음 백엔드 선택
    let address = backend.get_next_address();
    
    match build_proxied_request(address, req) {
        Ok(proxied_req) => {
            println!("Proxying request to backend: {}", address);
            match config.client.request(proxied_req).await {
                Ok(res) => {
                    let status = res.status();
                    println!("Backend responded with status: {}", status);
                    
                    let (parts, body) = res.into_parts();
                    match body.collect().await {
                        Ok(collected) => {
                            let bytes = collected.to_bytes();
                            println!("Successfully collected response body ({} bytes)", bytes.len());
                            Response::from_parts(parts, Full::new(bytes))
                        }
                        Err(e) => {
                            eprintln!("Failed to collect response body: {}", e);
                            build_error_response(
                                StatusCode::BAD_GATEWAY,
                                format!("Failed to collect response body: {}", e)
                            )
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Backend request failed: {}", e);
                    build_error_response(
                        StatusCode::BAD_GATEWAY,
                        format!("Backend request failed: {}", e)
                    )
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to build request: {}", e);
            build_error_response(
                StatusCode::BAD_REQUEST,
                format!("Failed to build request: {}", e)
            )
        }
    }
}

fn build_proxied_request(
    address: std::net::SocketAddr,
    req: Request<Incoming>,
) -> Result<Request<Incoming>, hyper::http::Error> {
    let uri: Uri = format!("http://{}{}", address, req.uri().path())
        .parse()
        .unwrap();

    let (parts, body) = req.into_parts();
    Request::builder()
        .method(parts.method)
        .uri(uri)
        .body(body)
}

fn build_error_response(status: StatusCode, message: String) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::from(message)))
        .unwrap()
} 