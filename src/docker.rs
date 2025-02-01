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

#[derive(Debug)]
pub enum DockerError {
    /// Docker 데몬 연결 실패
    ConnectionError(bollard::errors::Error),
    /// 컨테이너 목록 조회 실패
    ListContainersError(bollard::errors::Error),
    /// 컨테이너 설정 오류
    ContainerConfigError {
        container_id: String,
        reason: String,
    },
    /// 주소 파싱 오류
    AddressParseError {
        container_id: String,
        address: String,
    },
}

impl fmt::Display for DockerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DockerError::ConnectionError(e) => 
                write!(f, "Docker 데몬 연결 실패: {}", e),
            DockerError::ListContainersError(e) => 
                write!(f, "컨테이너 목록 조회 실패: {}", e),
            DockerError::ContainerConfigError { container_id, reason } => 
                write!(f, "컨테이너 {} 설정 오류: {}", container_id, reason),
            DockerError::AddressParseError { container_id, address } => 
                write!(f, "컨테이너 {}의 주소 {} 파싱 실패", container_id, address),
        }
    }
}

impl std::error::Error for DockerError {}

impl From<bollard::errors::Error> for DockerError {
    fn from(err: bollard::errors::Error) -> Self {
        DockerError::ConnectionError(err)
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
        let options = Some(ListContainersOptions::<String> {
            all: true,
            filters: {
                let mut filters = HashMap::new();
                filters.insert("label".to_string(), vec!["reverse-proxy.host".to_string()]);
                filters
            },
            ..Default::default()
        });

        let containers = self.docker.list_containers(options).await
            .map_err(DockerError::ListContainersError)?;

        let routes = self.extract_routes(&containers);
        println!("Routing table updated with {} routes", routes.len());  // 중요한 로그만 유지

        Ok(routes)
    }

    /// 컨테이너 목록에서 라우팅 정보를 추출합니다.
    fn extract_routes(&self, containers: &[ContainerSummary]) -> HashMap<String, BackendService> {
        containers.iter()
            .filter_map(|container| {
                let container_id = container.id.as_deref().unwrap_or("unknown");
                self.container_to_route(container)
                    .map_err(|e| eprintln!("Failed to process container {}: {}", container_id, e))
                    .ok()
            })
            .collect()
    }

    /// 단일 컨테이너에서 라우팅 정보를 추출합니다.
    fn container_to_route(&self, container: &ContainerSummary) -> Result<(String, BackendService), DockerError> {
        let container_id = container.id.as_deref().unwrap_or("unknown").to_string();
        
        // 네트워크 이름을 설정에서 가져오기
        let ip = container
            .network_settings
            .as_ref()
            .and_then(|settings| settings.networks.as_ref())
            .and_then(|networks| networks.get(&self.config.docker_network))
            .and_then(|network| network.ip_address.as_ref())
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: container_id.clone(),
                reason: format!("IP address not found in network {}", self.config.docker_network),
            })?;

        // 라벨 prefix 사용
        let host = self.extract_host_label(container, &container_id)?;
        let port = container.labels.as_ref()
            .and_then(|labels| labels.get(&format!("{}port", self.config.label_prefix)))
            .and_then(|port| port.parse::<u16>().ok())
            .unwrap_or(80);

        let addr = format!("{}:{}", ip, port).parse().map_err(|_| DockerError::AddressParseError {
            container_id: container_id.clone(),
            address: format!("{}:{}", ip, port),
        })?;

        Ok((host, BackendService { address: addr }))
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
        let config = self.config.clone();  // 설정 복제

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
                        let _ = tx.send(DockerEvent::Error(DockerError::ConnectionError(e))).await;
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
            .unwrap_or(&"unknown".to_string())
            .to_string();

        let manager = DockerManager { 
            docker: docker.clone(), 
            config: config.clone(),
        };

        match event.action.as_deref() {
            Some("start") => {
                if let Ok(Some((host, service))) = manager.get_container_info(&container_id).await {
                    tx.send(DockerEvent::ContainerStarted { 
                        container_id: container_id.clone(),
                        host,
                        service,
                    }).await.map_err(|_| Self::channel_send_error())?;
                }
            },
            Some("stop") | Some("die") | Some("destroy") => {
                // 컨테이너가 중지되기 전의 정보를 가져옴
                if let Ok(Some((host, _))) = manager.get_container_info(&container_id).await {
                    tx.send(DockerEvent::ContainerStopped { 
                        container_id: container_id.clone(),
                        host,
                    }).await.map_err(|_| Self::channel_send_error())?;
                }
            },
            Some("update") => {
                let old_info = manager.get_container_info(&container_id).await?;
                // 컨테이너 설정 업데이트 후 새 정보 가져오기
                let new_info = manager.get_container_info(&container_id).await?;
                if let Some((host, service)) = new_info {
                    tx.send(DockerEvent::ContainerUpdated { 
                        container_id: container_id.clone(),
                        old_host: old_info.map(|(h, _)| h),
                        new_host: Some(host),
                        service: Some(service),
                    }).await.map_err(|_| Self::channel_send_error())?;
                }
            },
            _ => {}
        }

        Ok(())
    }

    fn channel_send_error() -> DockerError {
        DockerError::ConnectionError(
            bollard::errors::Error::IOError { 
                err: std::io::Error::new(
                    std::io::ErrorKind::Other, 
                    "채널 전송 실패"
                )
            }
        )
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
            .map_err(DockerError::ListContainersError)?;

        if let Some(container) = containers.first() {
            self.container_to_route(container).map(Some)
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug)]
pub enum DockerEvent {
    /// 라우팅 테이블 전체 업데이트
    RoutesUpdated(HashMap<String, BackendService>),
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
} 