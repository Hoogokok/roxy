mod routing;
mod docker;
mod proxy;
mod config;
mod logging;
mod tls;

use std::convert::Infallible;
use hyper::{Request, Response, StatusCode};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use tokio::net::TcpListener;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use std::sync::Arc;
use routing::{RouteRule, RoutingTable};
use docker::{DockerManager, DockerEvent};
use config::Config;
use crate::logging::init_logging;
use tracing::{error, info, warn};
use proxy::ProxyConfig;
use hyper_util::rt::TokioIo;
use crate::tls::TlsConfig;

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
        DockerEvent::ContainerStarted { container_id, host, service, path } => {
            match service.get_next_address() {
                Ok(addr) => {
                    table.add_route(host.clone(), service, path.clone());
                    info!(
                        container_id = %container_id,
                        host = %host,
                        address = ?addr,
                        path = ?path,
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
        DockerEvent::ContainerUpdated { container_id, old_host, new_host, service, path } => {
            if let Some(old) = old_host {
                table.remove_route(&old);
            }
            if let Some(host) = new_host {
                if let Some(svc) = service {
                    table.add_route(host.clone(), svc, path.clone());
                    info!(
                        container_id = %container_id,
                        host = %host,
                        path = ?path,
                        "컨테이너 설정 변경"
                    );
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

async fn handle_connection<I>(io: I, routing_table: Arc<tokio::sync::RwLock<RoutingTable>>)
where
    I: hyper::rt::Read + hyper::rt::Write + Send + Unpin + 'static,
{
    if let Err(err) = http1::Builder::new()
        .serve_connection(io, service_fn(move |req| handle_request(routing_table.clone(), req)))
        .await 
    {
        error!(error = %err, "연결 처리 실패");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 설정을 먼저 로드
    let config = Config::load()
        .map_err(|e| {
            eprintln!("설정 로드 실패: {}", e);  // 로깅 초기화 전이므로 eprintln! 사용
            e
        })?;
    
    // 로깅 초기화
    init_logging(&config.logging)
        .map_err(|e| {
            eprintln!("로깅 초기화 실패: {}", e);
            e
        })?;
    
    info!(http_port = config.http_port, "서버 시작");
    
    let docker_manager = DockerManager::with_defaults(config.clone())
        .await
        .map_err(|e| {
            error!(error = %e, "Docker 매니저 초기화 실패");
            e
        })?;

    let routing_table = Arc::new(tokio::sync::RwLock::new(RoutingTable::new()));

    let initial_routes = docker_manager.get_container_routes().await
        .map_err(|e| {
            error!(error = %e, "초기 컨테이너 라우트 획득 실패");
            e
        })?;
    {
        let mut table = routing_table.write().await;
        let route_rules = initial_routes.clone().into_iter()
            .collect();
        table.sync_docker_routes(route_rules);
        info!(routes = ?initial_routes, "초기 라우팅 테이블 설정 완료");
    }

    let mut event_rx = docker_manager.subscribe_to_events().await;
    let routing_table_clone = routing_table.clone();

    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Err(e) = handle_docker_event(event, &mut routing_table_clone.write().await).await {
                error!(error = %e, "Docker 이벤트 처리 실패");
            }
        }
        warn!("Docker 이벤트 스트림 종료");
    });

    let http_listener = TcpListener::bind(format!("0.0.0.0:{}", config.http_port)).await
        .map_err(|e| {
            error!(error = %e, port = config.http_port, "HTTP 포트 바인딩 실패");
            e
        })?;

    info!(port = config.http_port, "HTTP 리스너 시작");

    let https_config = if config.https_enabled {
        let cert_path = config.tls_cert_path.as_ref().unwrap();
        let key_path = config.tls_key_path.as_ref().unwrap();
        
        Some(TlsConfig::new(cert_path, key_path, config.https_port).await?)
    } else {
        None
    };

    loop {
        tokio::select! {
            result = http_listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        info!(client_addr = %addr, "HTTP 클라이언트 연결 수락");
                        let routing_table = routing_table.clone();
                        let io = TokioIo::new(stream);
                        tokio::spawn(async move {
                            handle_connection(io, routing_table).await;
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "HTTP 연결 수락 실패");
                    }
                }
            }
            
            result = async { 
                if let Some(config) = &https_config {
                    config.listener.accept().await
                } else {
                    std::future::pending().await
                }
            } => {
                match result {
                    Ok((stream, addr)) => {
                        info!(client_addr = %addr, "HTTPS 클라이언트 연결 수락");
                        let routing_table = routing_table.clone();
                        let acceptor = https_config.as_ref().unwrap().acceptor.clone();
                        
                        tokio::spawn(async move {
                            match acceptor.accept(stream).await {
                                Ok(tls_stream) => {
                                    let io = TokioIo::new(tls_stream);
                                    handle_connection(io, routing_table).await;
                                }
                                Err(e) => {
                                    error!(error = %e, "TLS 핸드쉐이크 실패");
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "HTTPS 연결 수락 실패");
                    }
                }
            }
        }
    }
}
