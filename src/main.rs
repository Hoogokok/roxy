mod routing;
mod docker;
mod proxy;
mod config;
mod logging;

use std::convert::Infallible;
use hyper::{Request, Response, StatusCode};
use hyper::server::conn::http1;
use hyper::service::service_fn;
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
use tokio_rustls::rustls::{self, Certificate, PrivateKey};
use tokio_rustls::TlsAcceptor;
use std::fs::File;
use std::io::BufReader;
use hyper_util::rt::TokioIo;

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

async fn load_tls_config(cert_path: &str, key_path: &str) -> Result<rustls::ServerConfig, Box<dyn std::error::Error>> {
    let cert_file = File::open(cert_path)?;
    let mut cert_reader = BufReader::new(cert_file);
    let certs = rustls_pemfile::certs(&mut cert_reader)?
        .into_iter()
        .map(Certificate)
        .collect();

    let key_file = File::open(key_path)?;
    let mut key_reader = BufReader::new(key_file);
    let key = rustls_pemfile::pkcs8_private_keys(&mut key_reader)?
        .first()
        .ok_or("개인키를 찾을 수 없음")?
        .clone();

    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, PrivateKey(key))?;

    Ok(config)
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
    init_logging();
    
    let config = Config::from_env()
        .map_err(|e| {
            error!(error = %e, "설정 로드 실패");
            e
        })?;
    
    info!(http_port = config.http_port, "서버 시작");
    
    let docker_manager = DockerManager::new(config.clone())
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
        table.sync_docker_routes(initial_routes.clone());
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

    let https_acceptor = if config.https_enabled {
        let cert_path = config.tls_cert_path.as_ref().unwrap();
        let key_path = config.tls_key_path.as_ref().unwrap();
        
        let tls_config = load_tls_config(cert_path, key_path).await?;
        let acceptor = TlsAcceptor::from(Arc::new(tls_config));
        
        let https_listener = TcpListener::bind(format!("0.0.0.0:{}", config.https_port)).await
            .map_err(|e| {
                error!(error = %e, port = config.https_port, "HTTPS 포트 바인딩 실패");
                e
            })?;

        info!(port = config.https_port, "HTTPS 리스너 시작");
        Some((https_listener, acceptor))
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
                if let Some((listener, acceptor)) = &https_acceptor {
                    listener.accept().await
                } else {
                    std::future::pending().await
                }
            } => {
                match result {
                    Ok((stream, addr)) => {
                        info!(client_addr = %addr, "HTTPS 클라이언트 연결 수락");
                        let routing_table = routing_table.clone();
                        let acceptor = https_acceptor.as_ref().unwrap().1.clone();
                        
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
