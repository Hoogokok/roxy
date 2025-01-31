mod routing;
mod docker;

use std::convert::Infallible;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use tokio::net::TcpListener;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper_util::rt;
use hyper::StatusCode;
use std::sync::Arc;
use routing::RoutingTable;
use docker::DockerManager;
use tokio::time::{interval, Duration};
use crate::docker::DockerEvent;

async fn handle_request(
    routing_table: Arc<tokio::sync::RwLock<RoutingTable>>,
    req: hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let table = routing_table.read().await;
    match table.route_request(&req) {
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

async fn update_routes(
    docker_manager: &DockerManager,
    routing_table: Arc<tokio::sync::RwLock<RoutingTable>>,
) {
    match docker_manager.get_container_routes().await {
        Ok(routes) => {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(routes);
            println!("Updated routing table from Docker containers");
        }
        Err(e) => eprintln!("Failed to update routes from Docker: {}", e),
    }
}

#[tokio::main]
async fn main() {
    // Docker 매니저 초기화
    let docker_manager = DockerManager::new()
        .await
        .expect("Failed to initialize Docker manager");

    // 라우팅 테이블을 RwLock으로 감싸서 동시성 지원
    let routing_table = Arc::new(tokio::sync::RwLock::new(RoutingTable::new()));

    // Docker 이벤트 구독
    let mut event_rx = docker_manager.subscribe_to_events().await;
    let routing_table_clone = routing_table.clone();

    // 이벤트 처리 태스크 시작
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                DockerEvent::RoutesUpdated(routes) => {
                    let mut table = routing_table_clone.write().await;
                    table.sync_docker_routes(routes);
                    println!("라우팅 테이블이 Docker 이벤트로 인해 업데이트되었습니다");
                }
                DockerEvent::Error(e) => {
                    eprintln!("Docker 이벤트 처리 중 에러 발생: {}", e);
                }
            }
        }
    });

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
