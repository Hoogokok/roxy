mod error_types;
pub mod events_types;
mod retry;
mod client;
pub mod container;
mod health;

pub use client::{BollardDockerClient, DockerClient};
use container::ContainerInfo;
pub use container::{ContainerInfoExtractor, DefaultExtractor};
pub use events_types::{DockerEvent, HealthStatus};
pub use error_types::DockerError;
pub use retry::{RetryPolicy, with_retry, ContainerRoutesRetry};

use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage};
use bollard::system::EventsOptions;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc;
use crate::settings::DockerSettings;
use crate::routing_v2::{BackendService, PathMatcher};
use tracing::{debug, error, info, warn};
use tokio::time::Duration;
use std::sync::Arc;
use crate::middleware::MiddlewareConfig;
use tokio::{
    sync::RwLock,
    task::JoinHandle,
};
use self::health::{ContainerHealth, HealthCheckerFactory};
use std::sync::atomic::AtomicUsize;
use crate::routing_v2::LoadBalancerStrategy;

#[derive(Clone)]
pub struct DockerManager {
    client: Arc<Box<dyn DockerClient>>,
    extractor: Box<dyn ContainerInfoExtractor>,
    config: DockerSettings,
    health_checks: Arc<RwLock<HashMap<String, ContainerHealth>>>,
}

impl DockerManager {
    /// Docker 클라이언트를 초기화합니다.
    pub async fn new(
        client: Box<dyn DockerClient>,
        extractor: Box<dyn ContainerInfoExtractor>,
        config: DockerSettings,
    ) -> Self {
        Self {
            client: Arc::new(client),
            extractor,
            config,
            health_checks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 기본 구현을 사용하는 팩토리 메서드
    pub async fn with_defaults(settings: DockerSettings) -> Result<Self, DockerError> {
        let client = BollardDockerClient::new().await?;
        let extractor = DefaultExtractor::new(
            settings.network.clone(),
            settings.label_prefix.clone(),
        );

        Ok(Self::new(
            Box::new(client),
            Box::new(extractor),
            settings,
        ).await)
    }

    /// 컨테이너 라우트를 조회하고 실패 시 재시도합니다.
    pub async fn get_container_routes(&self) -> Result<HashMap<(String, PathMatcher), BackendService>, DockerError> {
        let retry_operation = ContainerRoutesRetry { docker_manager: self };
        let policy = RetryPolicy::from(&self.config.retry);
        
        with_retry(retry_operation, policy).await
    }

    /// 실제 컨테이너 라우트 조회 로직
    async fn try_get_container_routes(&self) -> Result<HashMap<(String, PathMatcher), BackendService>, DockerError> {
        info!("컨테이너 라우트 조회 시작");
        let containers = self.get_labeled_containers().await?;
        info!(count = containers.len(), "컨테이너 목록 조회 성공");

        let services = self.group_containers_by_service(containers).await;
        let mut routes = HashMap::new();
        
        for infos in services.values() {
            if !infos.is_empty() {
                match self.create_backend_service(infos) {
                    Ok((host, path_matcher, service)) => {
                        routes.insert((host, path_matcher), service);
                    }
                    Err(e) => {
                        warn!("백엔드 서비스 생성 실패: {}", e);
                    }
                }
            }
        }
        
        Ok(routes)
    }

    async fn get_labeled_containers(&self) -> Result<Vec<ContainerSummary>, DockerError> {
        let options = Some(ListContainersOptions::<String> {
            all: true,
            filters: HashMap::new(),  // 모든 컨테이너를 조회합니다.
            ..Default::default()
        });

        self.client.list_containers(options).await
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
        let health_checks = self.health_checks.clone();

        // 초기 라우트와 미들웨어 설정 전송
        if let Ok(routes) = self.try_get_container_routes().await {
            let _ = tx.send(DockerEvent::RoutesUpdated(routes)).await;
        }
        
        // 미들웨어 설정도 초기에 전송
        if let Ok(middleware_configs) = self.get_middleware_configs().await {
            let _ = tx.send(DockerEvent::MiddlewareConfigsUpdated(middleware_configs)).await;
        }

        // 헬스체크 시작
        let health_check_handle = self.start_health_checks(tx.clone()).await;

        tokio::spawn(async move {
            let options = EventsOptions {
                filters: Self::create_event_filters(),
                ..Default::default()
            };

            let mut events = docker.events(Some(options));

            while let Some(event) = events.next().await {
                match event {
                    Ok(event_msg) => {
                        if let Err(e) = Self::handle_container_event(
                            &docker, 
                            &config,
                            health_checks.clone(),
                            &event_msg,
                            &tx
                        ).await {
                            let _ = tx.send(DockerEvent::Error(e)).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(DockerEvent::Error(e)).await;
                    }
                }
            }

            // 이벤트 스트림이 종료되면 헬스체크도 중단
            health_check_handle.abort();
        });

        rx
    }

    /// Docker 이벤트를 처리하고 필요한 경우 라우팅 테이블을 업데이트합니다.
    async fn handle_container_event(
        docker: &Arc<Box<dyn DockerClient>>,
        config: &DockerSettings,
        health_checks: Arc<RwLock<HashMap<String, ContainerHealth>>>,
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
                config.network.clone(),
                config.label_prefix.clone(),
            )),
            config: config.clone(),
            health_checks,
        };

        // 이벤트 처리 후 미들웨어 설정도 업데이트
        let result = match event.action.as_deref() {
            Some("start") => {
                info!("컨테이너 시작 이벤트 감지: {}", container_id);
                Self::handle_container_start(&manager, container_id, tx).await
            }
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
        };

        // 미들웨어 설정 업데이트
        if let Ok(middleware_configs) = manager.get_middleware_configs().await {
            tx.send(DockerEvent::MiddlewareConfigsUpdated(middleware_configs))
                .await
                .map_err(|_| Self::channel_send_error())?;
        }

        result
    }

    /// 주기적인 헬스 체크 시작
    pub async fn start_health_checks(&self, tx: mpsc::Sender<DockerEvent>) -> JoinHandle<()> {
        let health_checks = self.health_checks.clone();
        let interval = self.config.health_check.interval;
        let health_checks_ptr = format!("{:p}", &*health_checks.read().await);
        info!("start_health_checks - health_checks 위치: {}", health_checks_ptr);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval));
            loop {
                interval.tick().await;
                let mut checks = health_checks.write().await;
                let count = checks.len();
                info!("헬스체크 실행 중... 컨테이너 수: {}, health_checks 위치: {}", count, health_checks_ptr);
                for (container_id, health) in checks.iter_mut() {
                    let host = health.host.clone();  
                    match health.check().await {
                        Ok(result) => {
                            let _ = tx.send(DockerEvent::ContainerHealthChanged {
                                container_id: container_id.clone(),
                                host,
                                status: result.status.clone(),
                                message: result.message.clone(),
                                consecutive_failures: health.consecutive_failures,
                            }).await;
                        }
                        Err(e) => {
                            error!(
                                error = %e,
                                container_id = %container_id,
                                "헬스 체크 실행 실패"
                            );
                        }
                    }
                }
            }
        })
    }

    /// 컨테이너 시작 시 헬스 체크 설정
    async fn handle_container_start(
        manager: &DockerManager,
        container_id: &str,
        tx: &mpsc::Sender<DockerEvent>,
    ) -> Result<(), DockerError> {
        info!(container_id = %container_id, "컨테이너 시작 이벤트 수신");
        
        // 컨테이너 정보 조회
        let containers = manager.get_labeled_containers().await?;
        let container = containers.iter()
            .find(|c| c.id.as_deref() == Some(container_id))
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: container_id.to_string(),
                reason: "컨테이너를 찾을 수 없음".to_string(),
                context: None,
            })?;

        match manager.get_container_info(container_id).await? {
            Some((host, service, path_matcher)) => {
                // 기존 이벤트 전송
                tx.send(DockerEvent::ContainerStarted { 
                    container_id: container_id.to_string(),
                    host: host.clone(),
                    service: service.clone(),
                    path_matcher,
                }).await.map_err(|_| Self::channel_send_error())?;

                // 헬스 체크 설정
                if let Ok(info) = manager.extractor.extract_info(container) {
                    debug!(
                        container_id = %container_id,
                        info = ?info,
                        "헬스체크 설정 시도"
                    );
                    manager.setup_health_check(container_id.to_string(), &info).await?;
                    debug!(container_id = %container_id, "헬스체크 설정 완료");
                } else {
                    warn!(container_id = %container_id, "컨테이너 정보 추출 실패");
                }

                Ok(())
            }
            None => {
                warn!(container_id = %container_id, "시작된 컨테이너 정보를 찾을 수 없음");
                Ok(())
            }
        }
    }

    /// 컨테이너 중지 시 헬스 체크 제거
    async fn handle_container_stop(
        manager: &DockerManager,
        container_id: &str,
        tx: &mpsc::Sender<DockerEvent>,
    ) -> Result<(), DockerError> {
        // 헬스 체크 제거
        manager.remove_health_check(container_id).await;

        // 기존 이벤트 전송
        if let Some((host, _, _)) = manager.get_container_info(container_id).await? {
            tx.send(DockerEvent::ContainerStopped { 
                container_id: container_id.to_string(),
                host,
            }).await.map_err(|_| Self::channel_send_error())?;
        }

        Ok(())
    }

    async fn handle_container_update(
        manager: &DockerManager,
        container_id: &str,
        tx: &mpsc::Sender<DockerEvent>,
    ) -> Result<(), DockerError> {
        info!(container_id = %container_id, "컨테이너 업데이트 이벤트 수신");
        
        let old_info = manager.get_container_info(container_id).await?;
        let new_info = manager.get_container_info(container_id).await?;
        
        if let Some((host, service, path_matcher)) = new_info {
            info!(
                container_id = %container_id,
                old_host = ?old_info.as_ref().map(|(h, _, _)| h),
                new_host = %host,
                path_matcher = ?path_matcher,
                "컨테이너 설정 변경 처리"
            );
            
            tx.send(DockerEvent::ContainerUpdated { 
                container_id: container_id.to_string(),
                old_host: old_info.map(|(h, _, _)| h),
                new_host: Some(host),
                service: Some(service),
                path_matcher,
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
    async fn get_container_info(&self, container_id: &str) -> Result<Option<(String, BackendService, Option<PathMatcher>)>, DockerError> {
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

        match containers.first() {
            Some(container) => {
                let info = self.extractor.extract_info(container)?;
                let service = self.extractor.create_backend(&info)?;
                Ok(Some((info.host, service, info.path_matcher)))
            }
            None => Ok(None),
        }
    }

    // 컨테이너 라벨 조회 메서드 추가
    pub async fn get_container_labels(&self) -> Result<HashMap<String, String>, DockerError> {
        let containers = self.client.list_containers(None).await?;
        let mut all_labels = HashMap::new();
        
        for container in containers {
            if let Some(labels) = container.labels {
                all_labels.extend(labels);
            }
        }
        
        Ok(all_labels)
    }

    // 미들웨어 설정 조회 메서드 추가
    pub async fn get_middleware_configs(&self) -> Result<Vec<(String, MiddlewareConfig)>, DockerError> {
        let labels = self.get_container_labels().await?;
        
        MiddlewareConfig::from_labels(&labels)
            .map_err(|e| DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: format!("미들웨어 설정 파싱 실패: {}", e),
                context: None,
            })
    }

    /// 컨테이너 헬스 체크 설정
    pub async fn setup_health_check(&self, container_id: String, info: &ContainerInfo) -> Result<(), DockerError> {
        let health_checks_ptr = format!("{:p}", &*self.health_checks.read().await);
        info!("setup_health_check - health_checks 위치: {}", health_checks_ptr);
        
        debug!(
            container_id = %container_id,
            host = %info.host,
            ip = %info.ip,
            port = %info.port,
            health_check = ?info.health_check,
            "헬스체크 설정 시작"
        );

        if let Some(health_check) = &info.health_check {
            let addr = format!("{}:{}", info.ip, info.port);
            
            if let Some(checker) = HealthCheckerFactory::create(addr.clone(), &health_check.check_type, health_check.timeout) {
                let container_health = ContainerHealth::new(container_id.clone(), info.host.clone(), checker);
                self.health_checks.write().await.insert(container_id.clone(), container_health);
                info!(
                    container_id = %container_id,
                    addr = %addr,
                    check_type = ?health_check.check_type,
                    health_checks_ptr = %health_checks_ptr,
                    "헬스체크 설정 완료"
                );
            } else {
                warn!(
                    container_id = %container_id,
                    addr = %addr,
                    "헬스체크 생성 실패"
                );
            }
        }
        Ok(())
    }

    /// 컨테이너 헬스 체크 제거
    pub async fn remove_health_check(&self, container_id: &str) {
        self.health_checks.write().await.remove(container_id);
    }

    // 컨테이너들을 서비스 이름으로 그룹화
    async fn group_containers_by_service(&self, containers: Vec<ContainerSummary>) 
        -> HashMap<String, Vec<ContainerInfo>> 
    {
        let mut services: HashMap<String, Vec<ContainerInfo>> = HashMap::new();
        
        for container in containers {
            if let Ok(info) = self.extractor.extract_info(&container) {
                let service_name = info.router_name.clone()
                    .unwrap_or_else(|| info.host.clone());
                services.entry(service_name)
                    .or_default()
                    .push(info);
            }
        }
        
        services
    }

    // 그룹화된 컨테이너들을 하나의 백엔드 서비스로 변환
    fn create_backend_service(&self, infos: &[ContainerInfo]) -> Result<(String, PathMatcher, BackendService), DockerError> {
        let first = &infos[0];
        debug!("서비스 생성 시작: host={}, path={:?}", first.host, first.path_matcher);
        
        let mut service = self.extractor.create_backend(first)?;
        
        // 여러 컨테이너가 있으면 로드밸런서 활성화
        if infos.len() > 1 {
            debug!("로드밸런서 활성화: 컨테이너 수={}", infos.len());
            service.enable_load_balancer(LoadBalancerStrategy::RoundRobin {
                current_index: AtomicUsize::new(0)
            });
            
            // 추가 컨테이너들의 주소 등록
            for info in &infos[1..] {
                let addr = self.extractor.parse_socket_addr(&info.ip, info.port)?;
                debug!("백엔드 주소 추가: {}", addr);
                service.add_address(addr, 1)?;
            }
        }

        let path_matcher = first.path_matcher.clone()
            .unwrap_or_else(|| PathMatcher::from_str("/").unwrap());
        debug!("최종 경로 매처: {:?}", path_matcher);
        
        Ok((first.host.clone(), path_matcher, service))
    }

    // 헬스체크 설정을 위한 헬퍼 함수
    async fn setup_container_health_check(
        &self,
        container: &ContainerSummary,
    ) -> Result<(), DockerError> {
        let id = container.id.as_ref().ok_or_else(|| DockerError::ContainerConfigError {
            container_id: "unknown".to_string(),
            reason: "컨테이너 ID 없음".to_string(),
            context: None,
        })?;
        
        let info = self.extractor.extract_info(container)?;
        
        debug!(
            container_id = %id,
            "컨테이너 헬스체크 초기 설정 시도"
        );
        
        self.setup_health_check(id.clone(), &info).await
    }

    pub async fn setup_initial_health_checks(&self) -> Result<(), DockerError> {
        info!("초기 컨테이너 헬스체크 설정 시작");
        
        let containers = self.get_labeled_containers().await?;
        for container in containers {
            if let Err(e) = self.setup_container_health_check(&container).await {
                debug!(error = %e, "컨테이너 헬스체크 설정 실패");
                // 개별 컨테이너 오류는 무시하고 계속 진행
                continue;
            }
        }
        
        info!("초기 컨테이너 헬스체크 설정 완료");
        Ok(())
    }
}
