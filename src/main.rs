use std::convert::Infallible;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use tokio::net::TcpListener;
use http_body_util::{Empty, Full};
use hyper::body::Bytes;
use hyper_util::rt;
use hyper::{Method, StatusCode};
use std::sync::Arc;

// Router 구조체 정의
#[derive(Clone)]
struct Router;

impl Router {
    fn new() -> Self {
        Router
    }

    async fn route(&self, req: &hyper::Request<hyper::body::Incoming>) -> hyper::Response<Full<Bytes>> {
        println!("Routing request: {} {}", req.method(), req.uri().path());
        
        match (req.method(), req.uri().path()) {
            (&Method::GET, "/") => {
                hyper::Response::builder()
                    .status(StatusCode::OK)
                    .body(Full::new(Bytes::from("Welcome to Reverse Proxy!")))
                    .unwrap()
            }
            _ => {
                hyper::Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Full::new(Bytes::from("Not Found")))
                    .unwrap()
            }
        }
    }
}

// handle_request 함수 수정
async fn handle_request(
    router: Arc<Router>,
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    Ok(router.route(&req).await)
}

#[tokio::main]
async fn main() {
    let router = Arc::new(Router::new());
    
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
