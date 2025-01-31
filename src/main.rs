mod routing;

use std::convert::Infallible;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use tokio::net::TcpListener;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper_util::rt;
use hyper::StatusCode;
use std::sync::Arc;
use routing::{RoutingTable, BackendService};

async fn handle_request(
    routing_table: Arc<RoutingTable>,
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    match routing_table.route_request(&req) {
        Ok(backend) => {
            println!("Found backend service: {:?}", backend);
            Ok(hyper::Response::builder()
                .status(StatusCode::OK)
                .body(Full::new(Bytes::from(format!("Found backend: {:?}", backend.address))))
                .unwrap())
        }
        Err(e) => {
            println!("Routing error: {}", e);
            let status = match e {
                routing::RoutingError::MissingHost | routing::RoutingError::InvalidHost(_) | 
                routing::RoutingError::InvalidPort(_) | routing::RoutingError::HeaderParseError(_) => StatusCode::BAD_REQUEST,
                routing::RoutingError::BackendNotFound(_) => StatusCode::NOT_FOUND,
            };
            
            Ok(hyper::Response::builder()
                .status(status)
                .body(Full::new(Bytes::from(format!("Error: {}", e))))
                .unwrap())
        }
    }
}

#[tokio::main]
async fn main() {
    // 라우팅 테이블 초기화
    let mut table = RoutingTable::new();
    
    // 테스트용 백엔드 서비스 추가
    table.add_route(
        "example.com".to_string(),
        BackendService {
            address: "127.0.0.1:8080".parse().unwrap(),
        },
    );

    let routing_table = Arc::new(table);
    
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

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("Accepted connection from: {}", addr);
                
                let routing_table = routing_table.clone();
                
                tokio::task::spawn(async move {
                    let io = rt::TokioIo::new(stream);
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(io, service_fn(move |req| handle_request(routing_table.clone(), req)))
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
