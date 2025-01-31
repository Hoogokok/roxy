mod routing;

use std::convert::Infallible;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use tokio::net::TcpListener;
use http_body_util::{Empty, Full};
use hyper::body::Bytes;
use hyper_util::rt;
use hyper::{Method, StatusCode, header};
use std::sync::Arc;

// 라우트 설정을 위한 불변 데이터 구조
#[derive(Clone)]
struct Route {
    path: String,
    method: Method,
    handler: Arc<dyn Fn() -> hyper::Response<Full<Bytes>> + Send + Sync>,
}

// 라우터 상태를 표현하는 불변 데이터 구조
#[derive(Clone)]
struct Router {
    routes: Arc<Vec<Route>>,
}

impl Router {
    fn new(routes: Vec<Route>) -> Self {
        Router {
            routes: Arc::new(routes)
        }
    }

    // 순수 함수: 입력이 같으면 항상 같은 출력을 반환
    async fn route(&self, req: &hyper::Request<hyper::body::Incoming>) -> hyper::Response<Full<Bytes>> {
        println!("Routing request: {} {}", req.method(), req.uri().path());
        
        // 함수형 스타일로 라우트 찾기
        self.routes
            .iter()
            .find(|route| route.method == *req.method() && route.path == req.uri().path())
            .map(|route| (route.handler)())
            .unwrap_or_else(|| {
                hyper::Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Full::new(Bytes::from("Not Found")))
                    .unwrap()
            })
    }
}

// 핸들러 함수들 - 순수 함수로 구현
fn welcome_handler() -> hyper::Response<Full<Bytes>> {
    hyper::Response::builder()
        .status(StatusCode::OK)
        .body(Full::new(Bytes::from("Welcome to Reverse Proxy!")))
        .unwrap()
}

// 호스트 정보를 담는 불변 데이터 구조
#[derive(Clone, Debug)]
struct HostInfo {
    name: String,
    port: Option<u16>,
}

impl HostInfo {
    // 호스트 헤더 값에서 HostInfo를 생성하는 순수 함수
    fn from_header_value(host: &str) -> Option<Self> {
        let parts: Vec<&str> = host.split(':').collect();
        match parts.as_slice() {
            [name] => Some(HostInfo {
                name: name.to_string(),
                port: None,
            }),
            [name, port] => Some(HostInfo {
                name: name.to_string(),
                port: port.parse().ok(),
            }),
            _ => None,
        }
    }
}

// 요청에서 호스트 정보를 추출하는 순수 함수
fn extract_host(req: &hyper::Request<hyper::body::Incoming>) -> Option<HostInfo> {
    req.headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .and_then(HostInfo::from_header_value)
}

// handle_request 함수 수정
async fn handle_request(
    router: Arc<Router>,
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    // 호스트 헤더 추출
    match extract_host(&req) {
        Some(host) => {
            println!("Received request for host: {:?}", host);
            Ok(hyper::Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from(format!("Received request for host: {:?}", host))))
                .unwrap())
        }
        None => {
            println!("No host header found");
            Ok(hyper::Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::new(Bytes::from("Missing or invalid Host header")))
                .unwrap())
        }
    }
}

#[tokio::main]
async fn main() {
    // 라우트 설정을 데이터로 정의
    let routes = vec![
        Route {
            path: "/".to_string(),
            method: Method::GET,
            handler: Arc::new(welcome_handler),
        },
    ];

    let router = Arc::new(Router::new(routes));
    
    // TCP 리스너 생성
    let listener = match TcpListener::bind("0.0.0.0:80").await {
        Ok(listener) => {
            println!("Reverse Proxy listening on port 80");
            listener
        }
        Err(e) => {
            eprintln!("Failed to bind to port 80: {}", e);
            return;
        }
    };

    // 연결 수락 루프
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("Accepted connection from: {}", addr);
                
                // 라우터 클론
                let router = router.clone();
                
                // 각 연결에 대해 새로운 태스크 생성
                tokio::task::spawn(async move {
                    let io = rt::TokioIo::new(stream);
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(io, service_fn(move |req| handle_request(router.clone(), req)))
                        .await 
                    {
                        eprintln!("Error serving connection: {}", err);
                    }
                });
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }
}
