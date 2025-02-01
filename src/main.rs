mod routing;
mod docker;
mod proxy;
mod config;

use std::convert::Infallible;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt;
use tokio::net::TcpListener;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::StatusCode;
use std::sync::Arc;
use routing::RoutingTable;
use docker::DockerManager;
use crate::docker::DockerEvent;
use hyper::body::Incoming;
use hyper::Request;
use config::Config;

async fn handle_request(
    routing_table: Arc<tokio::sync::RwLock<RoutingTable>>,
    req: Request<Incoming>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    let table = routing_table.read().await;
    let proxy_config = proxy::ProxyConfig::new();
    
    match table.route_request(&req) {
        Ok(backend) => {
            Ok(proxy::proxy_request(&proxy_config, backend, req).await)
        }
        Err(e) => {
            println!("Routing error: {}", e);
            let status = match e {
                routing::RoutingError::MissingHost | 
                routing::RoutingError::InvalidHost(_) | 
                routing::RoutingError::InvalidPort(_) | 
                routing::RoutingError::HeaderParseError(_) => StatusCode::BAD_REQUEST,
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
    // 설정 로드
    let config = Config::from_env();
    println!("Starting with config: {:?}", config);
    
    // Docker 매니저 초기화
    let docker_manager = DockerManager::new(config.clone())
        .await
        .expect("Failed to initialize Docker manager");

    // 라우팅 테이블을 RwLock으로 감싸서 동시성 지원
    let routing_table = Arc::new(tokio::sync::RwLock::new(RoutingTable::new()));

    // 초기 라우팅 테이블 설정
    let initial_routes = docker_manager.get_container_routes().await
        .expect("Failed to get initial container routes");
    {
        let mut table = routing_table.write().await;
        table.sync_docker_routes(initial_routes.clone());
        println!("Initial routing table setup completed");
        println!("Current routes: {:?}", initial_routes);
    }

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
    let listener = match TcpListener::bind(format!("0.0.0.0:{}", config.http_port)).await {
        Ok(listener) => {
            println!("Reverse Proxy listening on port {}", config.http_port);
            listener
        }
        Err(e) => {
            eprintln!("Failed to bind to port {}: {}", config.http_port, e);
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
