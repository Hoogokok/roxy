use bollard::Docker;
use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage, EventMessageTypeEnum};
use bollard::system::EventsOptions;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::fmt;
use tokio::sync::mpsc;
use crate::routing::BackendService;
use crate::config::Config;
use tracing::{debug, error, info, warn};

#[derive(Debug)]
pub enum DockerError {
    /// Docker 데몬 연결 실패
    ConnectionError {
        source: bollard::errors::Error,
        context: String,
    },
    /// 컨테이너 목록 조회 실패
    ListContainersError {
        source: bollard::errors::Error,
        context: String,
    },
    /// 컨테이너 설정 오류
    ContainerConfigError {
        container_id: String,
        reason: String,
        context: Option<String>,
    },
    /// 주소 파싱 오류
    AddressParseError {
        container_id: String,
        address: String,
        network: String,
        context: Option<String>,
    },
    /// 네트워크 설정 오류
    NetworkError {
        container_id: String,
        network: String,
        reason: String,
        context: Option<String>,
    },
}

impl fmt::Display for DockerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DockerError::ConnectionError { source, context } => 
                write!(f, "Docker 데몬 연결 실패 ({}): {}", context, source),
            DockerError::ListContainersError { source, context } => 
                write!(f, "컨테이너 목록 조회 실패 ({}): {}", context, source),
            DockerError::ContainerConfigError { container_id, reason, context } => 
                if let Some(ctx) = context {
                    write!(f, "컨테이너 {} 설정 오류 ({}): {}", container_id, ctx, reason)
                } else {
                    write!(f, "컨테이너 {} 설정 오류: {}", container_id, reason)
                },
            DockerError::AddressParseError { container_id, address, network, context } => 
                write!(f, "컨테이너 {}의 네트워크 {} 주소 {} 파싱 실패 ({})", 
                    container_id, network, address, context.as_deref().unwrap_or("No context provided")),
            DockerError::NetworkError { container_id, network, reason, context } =>
                write!(f, "컨테이너 {}의 네트워크 {} 설정 오류 ({}): {}", 
                    container_id, network, context.as_deref().unwrap_or("No context provided"), reason),
        }
    }
}

impl std::error::Error for DockerError {}

impl From<bollard::errors::Error> for DockerError {
    fn from(err: bollard::errors::Error) -> Self {
        DockerError::ConnectionError { 
            source: err,
            context: "기본 연결".to_string()
        }
    }
}

pub struct DockerManager {
    docker: Docker,
    config: Config,
}

impl DockerManager {
    /// Docker 클라이언트를 초기화합니다.
    pub async fn new(config: Config) -> Result<Self, DockerError> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(DockerManager { docker, config })
    }

    /// reverse-proxy.host 라벨이 있는 컨테이너를 찾고 
    /// 호스트-백엔드 서비스 매핑을 반환합니다.
    pub async fn get_container_routes(&self) -> Result<HashMap<String, BackendService>, DockerError> {
        info!("컨테이너 라우트 조회 시작");
        
        let options = Some(ListContainersOptions::<String> {
            all: true,
            filters: {
                let mut filters = HashMap::new();
                filters.insert("label".to_string(), vec!["reverse-proxy.host".to_string()]);
                filters
            },
            ..Default::default()
        });

        let containers = match self.docker.list_containers(options).await {
            Ok(containers) => {
                info!(count = containers.len(), "컨테이너 목록 조회 성공");
                containers
            },
            Err(e) => {
                error!(error = %e, "컨테이너 목록 조회 실패");
                return Err(DockerError::ListContainersError {
                    source: e,
                    context: "라우팅 가능한 컨테이너 조회 중 오류".to_string()
                });
            }
        };

        let routes = self.extract_routes(&containers);
        info!(route_count = routes.len(), "라우팅 테이블 업데이트 완료");

        Ok(routes)
    }

    /// 컨테이너 목록에서 라우팅 정보를 추출합니다.
    fn extract_routes(&self, containers: &[ContainerSummary]) -> HashMap<String, BackendService> {
        let mut routes = HashMap::new();
        
        for container in containers {
            if let Ok((host, addr)) = self.container_to_route(container) {
                let host_clone = host.clone();
                routes.entry(host)
                    .and_modify(|service: &mut BackendService| {
                        service.addresses.push(addr.get_next_address());
                        info!(
                            host = %host_clone,
                            address = ?addr.get_next_address(),
                            "기존 서비스에 주소 추가"
                        );
                    })
                    .or_insert_with(|| {
                        info!(
                            host = %host_clone,
                            address = ?addr.get_next_address(),
                            "새 서비스 생성"
                        );
                        addr
                    });
            }
        }
        
        if !routes.is_empty() {
            debug!(routes = ?routes, "라우팅 테이블 구성 완료");
        }
        routes
    }

    /// 단일 컨테이너에서 라우팅 정보를 추출합니다.
    fn container_to_route(&self, container: &ContainerSummary) -> Result<(String, BackendService), DockerError> {
        let container_id = container.id.as_deref().unwrap_or("unknown").to_string();
        
        // IP 주소 가져오기
        let network_settings = container.network_settings.as_ref()
            .ok_or_else(|| DockerError::NetworkError {
                container_id: container_id.clone(),
                network: self.config.docker_network.clone(),
                reason: "네트워크 설정을 찾을 수 없음".to_string(),
                context: None,
            })?;

        let networks = network_settings.networks.as_ref()
            .ok_or_else(|| DockerError::NetworkError {
                container_id: container_id.clone(),
                network: self.config.docker_network.clone(),
                reason: "네트워크 정보를 찾을 수 없음".to_string(),
                context: None,
            })?;

        let network = networks.get(&self.config.docker_network)
            .ok_or_else(|| DockerError::NetworkError {
                container_id: container_id.clone(),
                network: self.config.docker_network.clone(),
                reason: "지정된 네트워크를 찾을 수 없음".to_string(),
                context: None,
            })?;

        let ip = network.ip_address.as_ref()
            .ok_or_else(|| DockerError::NetworkError {
                container_id: container_id.clone(),
                network: self.config.docker_network.clone(),
                reason: "IP 주소를 찾을 수 없음".to_string(),
                context: None,
            })?;

        // 호스트와 포트 가져오기
        let host = self.extract_host_label(container, &container_id)?;
        let port = container.labels.as_ref()
            .and_then(|labels| labels.get(&format!("{}port", self.config.label_prefix)))
            .and_then(|port| port.parse::<u16>().ok())
            .unwrap_or(80);

        // IP가 비어있지 않은지 확인
        if ip.is_empty() {
            return Err(DockerError::NetworkError {
                container_id,
                network: self.config.docker_network.clone(),
                reason: "IP 주소가 비어있음".to_string(),
                context: None,
            });
        }

        // 주소 파싱
        let addr = format!("{}:{}", ip, port).parse().map_err(|_| DockerError::AddressParseError {
            container_id: container_id.clone(),
            address: format!("{}:{}", ip, port),
            network: self.config.docker_network.clone(),
            context: None,
        })?;

        info!(
            container_id = %container_id,
            ip = %ip,
            port = %port,
            host = %host,
            "컨테이너 라우팅 정보 추출"
        );
        
        Ok((host, BackendService::new(addr)))
    }

    /// 컨테이너에서 호스트 라벨을 추출합니다.
    fn extract_host_label(&self, container: &ContainerSummary, container_id: &str) -> Result<String, DockerError> {
        container
            .labels
            .as_ref()
            .and_then(|labels| labels.get(&format!("{}host", self.config.label_prefix)))
            .map(|s| s.to_string())
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: container_id.to_string(),
                reason: format!("{}host label missing", self.config.label_prefix),
                context: None,
            })
    }

    fn create_event_filters() -> HashMap<String, Vec<String>> {
        let mut filters = HashMap::new();
        filters.insert(
            "type".to_string(),
            vec!["container".to_string()]
        );
        filters.insert(
            "event".to_string(),
            vec![
                "start".to_string(),
                "stop".to_string(),
                "die".to_string(),
                "destroy".to_string(),
                "update".to_string(),
            ]
        );
        filters
    }

    /// Docker 이벤트를 구독하고 라우팅 테이블 업데이트를 위한 이벤트를 전송합니다.
    pub async fn subscribe_to_events(&self) -> mpsc::Receiver<DockerEvent> {
        let (tx, rx) = mpsc::channel(32);
        let docker = self.docker.clone();
        let config = self.config.clone();

        // 초기 라우트 전송
        if let Ok(routes) = self.get_container_routes().await {
            let _ = tx.send(DockerEvent::RoutesUpdated(routes)).await;
        }

        tokio::spawn(async move {
            let options = EventsOptions {
                filters: Self::create_event_filters(),
                ..Default::default()
            };

            let mut events = docker.events(Some(options));

            while let Some(event) = events.next().await {
                match event {
                    Ok(event) => {
                        if let Err(e) = Self::handle_docker_event(&docker, &config, &event, &tx).await {  // config 추가
                            let _ = tx.send(DockerEvent::Error(e)).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(DockerEvent::Error(DockerError::ConnectionError { 
                            source: e,
                            context: "Docker 이벤트 구독".to_string()
                        })).await;
                    }
                }
            }
        });

        rx
    }

    /// Docker 이벤트를 처리하고 필요한 경우 라우팅 테이블을 업데이트합니다.
    async fn handle_docker_event(
        docker: &Docker,
        config: &Config,
        event: &EventMessage,
        tx: &mpsc::Sender<DockerEvent>,
    ) -> Result<(), DockerError> {
        if event.typ != Some(EventMessageTypeEnum::CONTAINER) {
            return Ok(());
        }

        let container_id = event.actor.as_ref()
            .and_then(|actor| actor.id.as_ref())
            .map(String::from)
            .unwrap_or_else(|| {
                warn!("컨테이너 ID를 찾을 수 없음");
                "unknown".to_string()
            });

        let manager = DockerManager { 
            docker: docker.clone(), 
            config: config.clone(),
        };

        match event.action.as_deref() {
            Some("start") => {
                info!(container_id = %container_id, "컨테이너 시작 이벤트 수신");
                match manager.get_container_info(&container_id).await {
                    Ok(Some((host, service))) => {
                        info!(
                            container_id = %container_id,
                            host = %host,
                            "컨테이너 시작 처리 완료"
                        );
                        if let Err(_) = tx.send(DockerEvent::ContainerStarted { 
                            container_id: container_id.clone(),
                            host,
                            service,
                        }).await {
                            error!(container_id = %container_id, "이벤트 전송 실패");
                            return Err(Self::channel_send_error());
                        }
                    }
                    Ok(None) => {
                        warn!(container_id = %container_id, "시작된 컨테이너 정보를 찾을 수 없음");
                    }
                    Err(e) => {
                        error!(
                            container_id = %container_id,
                            error = %e,
                            "컨테이너 정보 조회 실패"
                        );
                        return Err(e);
                    }
                }
            },
            Some("stop") | Some("die") | Some("destroy") => {
                info!(
                    container_id = %container_id,
                    action = %event.action.as_deref().unwrap_or("unknown"),
                    "컨테이너 중지 관련 이벤트 수신"
                );
                match manager.get_container_info(&container_id).await {
                    Ok(Some((host, _))) => {
                        if let Err(_) = tx.send(DockerEvent::ContainerStopped { 
                            container_id: container_id.clone(),
                            host,
                        }).await {
                            error!(container_id = %container_id, "이벤트 전송 실패");
                            return Err(Self::channel_send_error());
                        }
                    }
                    Ok(None) => {
                        warn!(container_id = %container_id, "중지된 컨테이너 정보를 찾을 수 없음");
                    }
                    Err(e) => {
                        error!(
                            container_id = %container_id,
                            error = %e,
                            "컨테이너 정보 조회 실패"
                        );
                        return Err(e);
                    }
                }
            },
            Some("update") => {
                info!(container_id = %container_id, "컨테이너 업데이트 이벤트 수신");
                let old_info = manager.get_container_info(&container_id).await?;
                let new_info = manager.get_container_info(&container_id).await?;
                
                if let Some((host, service)) = new_info {
                    info!(
                        container_id = %container_id,
                        old_host = ?old_info.as_ref().map(|(h, _)| h),
                        new_host = %host,
                        "컨테이너 설정 변경 처리"
                    );
                    if let Err(_) = tx.send(DockerEvent::ContainerUpdated { 
                        container_id: container_id.clone(),
                        old_host: old_info.map(|(h, _)| h),
                        new_host: Some(host),
                        service: Some(service),
                    }).await {
                        error!(container_id = %container_id, "이벤트 전송 실패");
                        return Err(Self::channel_send_error());
                    }
                }
            },
            action => {
                debug!(
                    container_id = %container_id,
                    action = ?action,
                    "처리되지 않는 컨테이너 이벤트"
                );
            }
        }

        Ok(())
    }

    fn channel_send_error() -> DockerError {
        DockerError::ConnectionError { 
            source: bollard::errors::Error::IOError { 
                err: std::io::Error::new(
                    std::io::ErrorKind::Other, 
                    "채널 전송 실패"
                )
            },
            context: "채널 전송".to_string()
        }
    }

    /// 단일 컨테이너의 라우팅 정보를 가져옵니다.
    async fn get_container_info(&self, container_id: &str) -> Result<Option<(String, BackendService)>, DockerError> {
        let options = Some(ListContainersOptions::<String> {
            all: true,
            filters: {
                let mut filters = HashMap::new();
                filters.insert("id".to_string(), vec![container_id.to_string()]);
                filters
            },
            ..Default::default()
        });

        let containers = self.docker.list_containers(options).await
            .map_err(|e| DockerError::ListContainersError {
                source: e,
                context: format!("컨테이너 {} 정보 조회", container_id)
            })?;

        if let Some(container) = containers.first() {
            self.container_to_route(container).map(Some)
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
pub enum DockerEvent {
    /// 컨테이너 시작
    ContainerStarted {
        container_id: String,
        host: String,
        service: BackendService,
    },
    /// 컨테이너 중지
    ContainerStopped {
        container_id: String,
        host: String,
    },
    /// 컨테이너 설정 변경
    ContainerUpdated {
        container_id: String,
        old_host: Option<String>,
        new_host: Option<String>,
        service: Option<BackendService>,
    },
    /// 에러 상황
    Error(DockerError),
    /// 라우팅 테이블 업데이트
    RoutesUpdated(HashMap<String, BackendService>),
} 