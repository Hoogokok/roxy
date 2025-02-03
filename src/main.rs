mod routing;
mod docker;
mod proxy;
mod config;
mod logging;

use std::convert::Infallible;
use hyper::{Request, Response, StatusCode};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt;
use tokio::net::TcpListener;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use std::sync::Arc;
use routing::RoutingTable;
use docker::{DockerManager, DockerEvent};
use config::Config;
use crate::logging::init_logging;
use tracing::{error, info, warn};
use proxy::ProxyConfig;

async fn handle_request(
    routing_table: Arc<tokio::sync::RwLock<RoutingTable>>,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let table = routing_table.read().await;
    let proxy_config = ProxyConfig::new();
    
    match table.route_request(&req) {
        Ok(backend) => {
            match proxy::proxy_request(&proxy_config, backend, req).await {
                Ok(response) => Ok(response),
                Err(e) => {
                    error!(error = %e, "프록시 요청 실패");
                    Ok(proxy::error_response(&e))
                }
            }
        }
        Err(e) => {
            error!(error = %e, "라우팅 실패");
            let status = match e {
                routing::RoutingError::MissingHost | 
                routing::RoutingError::InvalidHost { .. } | 
                routing::RoutingError::InvalidPort { .. } | 
                routing::RoutingError::HeaderParseError { .. } => StatusCode::BAD_REQUEST,
                routing::RoutingError::BackendNotFound { .. } => StatusCode::NOT_FOUND,
            };
            
            Ok(Response::builder()
                .status(status)
                .body(Full::new(Bytes::from(format!("Error: {}", e))))
                .unwrap_or_else(|e| {
                    error!(error = %e, "에러 응답 생성 실패");
                    Response::new(Full::new(Bytes::from("Internal Server Error")))
                }))
        }
    }
}

async fn handle_docker_event(
    event: DockerEvent,
    table: &mut tokio::sync::RwLockWriteGuard<'_, RoutingTable>,
) -> Result<(), Box<dyn std::error::Error>> {
    match event {
        DockerEvent::ContainerStarted { container_id, host, service } => {
            match service.get_next_address() {
                Ok(addr) => {
                    table.add_route(host.clone(), service);
                    info!(
                        container_id = %container_id,
                        host = %host,
                        address = ?addr,
                        "컨테이너 시작"
                    );
                }
                Err(e) => {
                    error!(
                        error = %e,
                        container_id = %container_id,
                        host = %host,
                        "컨테이너 시작 실패: 백엔드 주소 획득 실패"
                    );
                }
            }
        }
        DockerEvent::ContainerStopped { container_id, host } => {
            table.remove_route(&host);
            info!(container_id = %container_id, host = %host, "컨테이너 중지");
        }
        DockerEvent::RoutesUpdated(routes) => {
            table.sync_docker_routes(routes);
            info!("라우팅 테이블 업데이트");
        }
        DockerEvent::ContainerUpdated { container_id, old_host, new_host, service } => {
            if let Some(old) = old_host {
                table.remove_route(&old);
            }
            if let Some(host) = new_host {
                if let Some(svc) = service {
                    table.add_route(host.clone(), svc);
                    info!(container_id = %container_id, host = %host, "컨테이너 설정 변경");
                }
            }
        }
        DockerEvent::Error(e) => {
            error!(error = %e, "Docker 이벤트 처리 오류");
            return Err(e.into());
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();
    
    // 설정 로드
    let config = Config::from_env()
        .map_err(|e| {
            error!(error = %e, "설정 로드 실패");
            e
        })?;
    
    info!(http_port = config.http_port, "서버 시작");
    
    // Docker 매니저 초기화
    let docker_manager = DockerManager::new(config.clone())
        .await
        .map_err(|e| {
            error!(error = %e, "Docker 매니저 초기화 실패");
            e
        })?;

    let routing_table = Arc::new(tokio::sync::RwLock::new(RoutingTable::new()));

    // 초기 라우팅 테이블 설정
    let initial_routes = docker_manager.get_container_routes().await
        .map_err(|e| {
            error!(error = %e, "초기 컨테이너 라우트 획득 실패");
            e
        })?;
    {
        let mut table = routing_table.write().await;
        table.sync_docker_routes(initial_routes.clone());
        info!(routes = ?initial_routes, "초기 라우팅 테이블 설정 완료");
    }

    // Docker 이벤트 구독
    let mut event_rx = docker_manager.subscribe_to_events().await;
    let routing_table_clone = routing_table.clone();

    // 이벤트 처리 태스크 시작
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Err(e) = handle_docker_event(event, &mut routing_table_clone.write().await).await {
                error!(error = %e, "Docker 이벤트 처리 실패");
            }
        }
        warn!("Docker 이벤트 스트림 종료");
    });

    // TCP 리스너 생성
    let listener = TcpListener::bind(format!("0.0.0.0:{}", config.http_port)).await
        .map_err(|e| {
            error!(error = %e, port = config.http_port, "포트 바인딩 실패");
            e
        })?;

    info!(port = config.http_port, "리버스 프록시 서버 시작");

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!(client_addr = %addr, "클라이언트 연결 수락");
                
                let routing_table = routing_table.clone();
                
                tokio::task::spawn(async move {
                    let io = rt::TokioIo::new(stream);
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(io, service_fn(move |req| handle_request(routing_table.clone(), req)))
                        .await 
                    {
                        error!(error = %err, "연결 처리 실패");
                    }
                });
            }
            Err(e) => {
                error!(error = %e, "연결 수락 실패");
            }
        }
    }
}
