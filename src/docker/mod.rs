mod events_types;
mod error_types;
mod retry;
mod client;
pub mod container;

pub use client::{BollardDockerClient, DockerClient};
pub use container::{ContainerInfoExtractor, DefaultExtractor};
pub use events_types::DockerEvent;
pub use error_types::DockerError;
pub use retry::{RetryPolicy, with_retry, ContainerRoutesRetry};

use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage};
use bollard::system::EventsOptions;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc;
use crate::routing::BackendService;
use crate::config::Config;
use tracing::{debug, info, warn};
use tokio::time::Duration;
use std::sync::Arc;

#[derive(Clone)]
pub struct DockerManager {
    client: Arc<Box<dyn DockerClient>>,
    extractor: Box<dyn ContainerInfoExtractor>,
    config: Config,
}

impl DockerManager {
    /// Docker 클라이언트를 초기화합니다.
    pub async fn new(
        client: Box<dyn DockerClient>,
        extractor: Box<dyn ContainerInfoExtractor>,
        config: Config,
    ) -> Self {
        Self {
            client: Arc::new(client),
            extractor,
            config,
        }
    }

    /// 기본 구현을 사용하는 팩토리 메서드
    pub async fn with_defaults(config: Config) -> Result<Self, DockerError> {
        let client = BollardDockerClient::new().await?;
        let extractor = DefaultExtractor::new(
            config.docker_network.clone(),
            config.label_prefix.clone(),
        );

        Ok(Self::new(
            Box::new(client),
            Box::new(extractor),
            config,
        ).await)
    }

    /// 컨테이너 라우트를 조회하고 실패 시 재시도합니다.
    pub async fn get_container_routes(&self) -> Result< HashMap<(String, Option<String>), BackendService>, DockerError> {
        let retry_operation = ContainerRoutesRetry { docker_manager: self };
        let policy = RetryPolicy::new(3, Duration::from_secs(2));
        
        with_retry(retry_operation, policy).await
    }

    /// 실제 컨테이너 라우트 조회 로직
    async fn try_get_container_routes(&self) -> Result<HashMap<(String, Option<String>), BackendService>, DockerError> {
        info!("컨테이너 라우트 조회 시작");
        let containers = self.get_labeled_containers().await?;
        info!(count = containers.len(), "컨테이너 목록 조회 성공");

        let mut routes = HashMap::new();
        for container in containers {
            if let Ok((host, service, path)) = self.container_to_route(&container) {
                routes.insert((host, path), service);
            }
        }

        info!(route_count = routes.len(), "라우팅 테이블 업데이트 완료");
        Ok(routes)
    }

    async fn get_labeled_containers(&self) -> Result<Vec<ContainerSummary>, DockerError> {
        let options = Some(ListContainersOptions::<String> {
            all: true,
            filters: {
                let mut filters = HashMap::new();
                filters.insert("label".to_string(), vec![format!("{}host", self.config.label_prefix)]);
                filters
            },
            ..Default::default()
        });

        self.client.list_containers(options).await
    }


    /// 컨테이너에서 라우팅 정보를 추출합니다.
    fn container_to_route(&self, container: &ContainerSummary) -> Result<(String, BackendService, Option<String>), DockerError> {
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

        // path 레이블 가져오기
        let path = container.labels.as_ref()
            .and_then(|labels| labels.get(&format!("{}path", self.config.label_prefix)))
            .map(|s| s.to_string());

        info!(
            container_id = %container_id,
            ip = %ip,
            port = %port,
            host = %host,
            path = ?path,
            "컨테이너 라우팅 정보 추출"
        );
        
        Ok((host, BackendService::new(addr), path))
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
        let mut filters: HashMap<String, Vec<String>> = HashMap::new();
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
        let docker = self.client.clone();
        let config = self.config.clone();

        // 초기 라우트 전송
        if let Ok(routes) = self.try_get_container_routes().await {
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
                        if let Err(e) = Self::handle_container_event(&docker, &config, &event, &tx).await {
                            let _ = tx.send(DockerEvent::Error(e)).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(DockerEvent::Error(e)).await;
                    }
                }
            }
        });

        rx
    }

    /// Docker 이벤트를 처리하고 필요한 경우 라우팅 테이블을 업데이트합니다.
    async fn handle_container_event(
        docker: &Arc<Box<dyn DockerClient>>,
        config: &Config,
        event: &EventMessage,
        tx: &mpsc::Sender<DockerEvent>,
    ) -> Result<(), DockerError> {
        let container_id = event.actor.as_ref()
            .and_then(|actor| actor.id.as_ref())
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: "컨테이너 ID 누락".to_string(),
                context: None,
            })?;

        let manager = DockerManager { 
            client: docker.clone(),
            extractor: Box::new(DefaultExtractor::new(
                config.docker_network.clone(),
                config.label_prefix.clone(),
            )),
            config: config.clone(),
        };

        match event.action.as_deref() {
            Some("start") => Self::handle_container_start(&manager, container_id, tx).await,
            Some("stop" | "die" | "destroy") => Self::handle_container_stop(&manager, container_id, tx).await,
            Some("update") => Self::handle_container_update(&manager, container_id, tx).await,
            action => {
                debug!(
                    container_id = %container_id,
                    action = ?action,
                    "처리되지 않는 컨테이너 이벤트"
                );
                Ok(())
            }
        }
    }

    async fn handle_container_start(
        manager: &DockerManager,
        container_id: &str,
        tx: &mpsc::Sender<DockerEvent>,
    ) -> Result<(), DockerError> {
        info!(container_id = %container_id, "컨테이너 시작 이벤트 수신");
        
        match manager.get_container_info(container_id).await? {
            Some((host, service, path)) => {
                info!(
                    container_id = %container_id,
                    host = %host,
                    path = ?path,
                    "컨테이너 시작 처리 완료"
                );
                tx.send(DockerEvent::ContainerStarted { 
                    container_id: container_id.to_string(),
                    host,
                    service,
                    path,
                }).await.map_err(|_| Self::channel_send_error())?;
                Ok(())
            }
            None => {
                warn!(container_id = %container_id, "시작된 컨테이너 정보를 찾을 수 없음");
                Ok(())
            }
        }
    }

    async fn handle_container_stop(
        manager: &DockerManager,
        container_id: &str,
        tx: &mpsc::Sender<DockerEvent>,
    ) -> Result<(), DockerError> {
        info!(
            container_id = %container_id,
            "컨테이너 중지 관련 이벤트 수신"
        );

        match manager.get_container_info(container_id).await? {
            Some((host, _, _)) => {  // path 정보는 무시하고 호스트만 사용
                tx.send(DockerEvent::ContainerStopped { 
                    container_id: container_id.to_string(),
                    host,
                }).await.map_err(|_| Self::channel_send_error())?;
                Ok(())
            }
            None => {
                warn!(container_id = %container_id, "중지된 컨테이너 정보를 찾을 수 없음");
                Ok(())
            }
        }
    }

    async fn handle_container_update(
        manager: &DockerManager,
        container_id: &str,
        tx: &mpsc::Sender<DockerEvent>,
    ) -> Result<(), DockerError> {
        info!(container_id = %container_id, "컨테이너 업데이트 이벤트 수신");
        
        let old_info = manager.get_container_info(container_id).await?;
        let new_info = manager.get_container_info(container_id).await?;
        
        if let Some((host, service, path)) = new_info {
            info!(
                container_id = %container_id,
                old_host = ?old_info.as_ref().map(|(h, _, _)| h),
                new_host = %host,
                path = ?path,
                "컨테이너 설정 변경 처리"
            );
            
            tx.send(DockerEvent::ContainerUpdated { 
                container_id: container_id.to_string(),
                old_host: old_info.map(|(h, _, _)| h),
                new_host: Some(host),
                service: Some(service),
                path,
            }).await.map_err(|_| Self::channel_send_error())?;
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
    async fn get_container_info(&self, container_id: &str) -> Result<Option<(String, BackendService, Option<String>)>, DockerError> {
        let options = Some(ListContainersOptions::<String> {
            all: true,
            filters: {
                let mut filters = HashMap::new();
                filters.insert("id".to_string(), vec![container_id.to_string()]);
                filters
            },
            ..Default::default()
        });

        let containers = self.client.list_containers(options).await?;

        if let Some(container) = containers.first() {
            self.container_to_route(container).map(Some)
        } else {
            Ok(None)
        }
    }
}
