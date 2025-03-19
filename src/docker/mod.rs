mod error_types;
pub mod events_types;
mod retry;
mod client;
pub mod container;
mod health;
mod container_test;

pub use client::{BollardDockerClient, DockerClient};
use container::ContainerInfo;
pub use container::{ContainerInfoExtractor, DefaultExtractor};
pub use events_types::{DockerEvent, HealthStatus};
pub use error_types::DockerError;
pub use retry::{RetryPolicy, with_retry, ContainerRoutesRetry};

use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage};
use bollard::system::EventsOptions;
use futures_util::{stream::StreamExt, Stream};
use std::collections::HashMap;
use tokio::sync::mpsc;
use crate::settings::container::ContainerConfigManager;
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
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct DockerManager {
    client: Arc<Box<dyn DockerClient>>,
    extractor: Box<dyn ContainerInfoExtractor>,
    config: DockerSettings,
    health_checks: Arc<RwLock<HashMap<String, ContainerHealth>>>,
    container_config_manager: Arc<ContainerConfigManager>,
}

impl DockerManager {
    /// Docker 클라이언트를 초기화합니다.
    pub async fn new(
        client: Box<dyn DockerClient>,
        extractor: Box<dyn ContainerInfoExtractor>,
        config: DockerSettings,
    ) -> Self {
        // 설정 관리자 초기화
        let (container_config_manager, _config_rx) = ContainerConfigManager::new();
        
        Self {
            client: Arc::new(client),
            extractor,
            config,
            health_checks: Arc::new(RwLock::new(HashMap::new())),
            container_config_manager: Arc::new(container_config_manager),
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

        let (container_config_manager, _) = ContainerConfigManager::new();
        
        let manager = DockerManager { 
            client: docker.clone(),
            extractor: Box::new(DefaultExtractor::new(
                config.network.clone(),
                config.label_prefix.clone(),
            )),
            config: config.clone(),
            health_checks,
            container_config_manager: Arc::new(container_config_manager),
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
            
            // 1. 첫 번째 컨테이너의 개별 로드밸런서 설정 확인
            // 2. 없으면 글로벌 설정 확인
            // 3. 그것도 없으면 기본 라운드로빈 사용
            let strategy = match &first.load_balancer {
                // 컨테이너별 설정이 있으면 사용
                Some(lb_strategy) => {
                    debug!("첫 번째 컨테이너의 로드밸런서 설정 사용");
                    lb_strategy.clone()
                },
                // 컨테이너별 설정이 없으면 글로벌 설정 확인
                None => {
                    if self.config.has_load_balancer() {
                        debug!("글로벌 로드밸런서 설정 사용: {}", self.config.load_balancer_strategy());
                        
                        if self.config.load_balancer_strategy() == "weighted" {
                            let weight = self.config.load_balancer_weight().unwrap_or(1) as usize;
                            
                            LoadBalancerStrategy::Weighted {
                                current_index: AtomicUsize::new(0),
                                total_weight: weight, // 초기값은 첫 번째 컨테이너 가중치
                            }
                        } else {
                            LoadBalancerStrategy::RoundRobin {
                                current_index: AtomicUsize::new(0),
                            }
                        }
                    } else {
                        // 기본 라운드로빈 전략
                        debug!("로드밸런서 설정 없음, 기본 라운드로빈 사용");
                        LoadBalancerStrategy::RoundRobin {
                            current_index: AtomicUsize::new(0),
                        }
                    }
                }
            };
            
            service.enable_load_balancer(strategy);
            
            // 추가 백엔드 추가
            let mut calculated_total_weight = match &service.load_balancer {
                Some(lb) => lb.get_total_weight().unwrap_or(0),
                None => 0
            };
            
            for info in &infos[1..] {
                let addr = self.extractor.parse_socket_addr(&info.ip, info.port)?;
                
                // 각 컨테이너별 가중치 결정
                let weight = match &info.load_balancer {
                    // 컨테이너별 설정이 있으면 사용
                    Some(LoadBalancerStrategy::Weighted { total_weight, .. }) => {
                        debug!(
                            container_ip = %info.ip,
                            weight = %total_weight,
                            "컨테이너별 가중치 적용"
                        );
                        *total_weight
                    },
                    // 없으면 글로벌 설정 사용
                    _ => {
                        let global_weight = self.config.load_balancer_weight().unwrap_or(1) as usize;
                        debug!(
                            container_ip = %info.ip,
                            weight = %global_weight,
                            "글로벌 가중치 적용"
                        );
                        global_weight
                    }
                };
                
                // 백엔드 추가
                service.add_address(addr, weight)?;
                
                // 가중치 기반인 경우 총 가중치 누적
                if let Some(ref lb) = service.load_balancer {
                    if lb.is_weighted() {
                        calculated_total_weight += weight;
                    }
                }
            }
            
            // 가중치 기반인 경우 총 가중치 업데이트
            if let Some(ref mut lb) = service.load_balancer {
                if lb.is_weighted() {
                    debug!("로드밸런서 총 가중치 업데이트: {}", calculated_total_weight);
                    if let Err(e) = lb.set_total_weight(calculated_total_weight) {
                        warn!("로드밸런서 가중치 업데이트 실패: {}", e);
                    }
                }
            }
        }
        
        // 미들웨어 처리
        if let Some(middlewares) = &first.middlewares {
            service.set_middlewares(middlewares.clone());
        }
        
        let path_matcher = first.path_matcher.clone().unwrap_or_else(|| PathMatcher::from_str("/").unwrap());
        
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

    /// 모든 컨테이너의 설정 파일 경로 반환
    pub async fn get_container_config_paths(&self) -> Result<Vec<(String, PathBuf)>, DockerError> {
        let mut paths = Vec::new();
        let containers = self.get_labeled_containers().await?;
        
        for container in &containers {
            let id = match &container.id {
                Some(id) => id.clone(),
                None => continue,
            };
            
            // 컨테이너 정보 추출
            if let Ok(info) = self.extractor.extract_info(container) {
                if let Some(path) = &info.json_config_path {
                    paths.push((id, PathBuf::from(path.as_str())));
                }
            }
        }
        
        Ok(paths)
    }

    /// 컨테이너별 JSON 설정 파일 로드
    pub async fn load_container_json_config(&self, container_id: &str, path: &Path) -> Result<(), DockerError> {
        info!(container_id = %container_id, path = %path.display(), "컨테이너 JSON 설정 로드");
        
        // 설정 파일 로드 시도
        self.container_config_manager
            .load_container_config(container_id.to_string(), path)
            .await
            .map_err(|e| DockerError::ContainerConfigError {
                container_id: container_id.to_string(),
                reason: "JSON 설정 로드 실패".to_string(),
                context: Some(e.to_string()),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::pin::Pin;
    use async_trait::async_trait;
    use tempfile;
    use crate::settings::types::ConfigPath;
    use crate::settings::docker::DockerSettings;
    use crate::settings::typestate::{Raw, Validated, Validatable};

    // DockerClient Mock 구현
    struct MockDockerClient;

    #[async_trait::async_trait]
    impl DockerClient for MockDockerClient {
        fn clone_box(&self) -> Box<dyn DockerClient> {
            Box::new(Self)
        }

        async fn list_containers(
            &self, 
            _options: Option<ListContainersOptions<String>>
        ) -> Result<Vec<ContainerSummary>, DockerError> {
            Ok(vec![])
        }

        fn events(
            &self, 
            _options: Option<EventsOptions<String>>
        ) -> Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>> {
            Box::pin(futures_util::stream::empty())
        }
    }

    // 테스트용 ContainerInfoExtractor 구현
    struct MockExtractor;

    impl ContainerInfoExtractor for MockExtractor {
        fn clone_box(&self) -> Box<dyn ContainerInfoExtractor> {
            Box::new(Self)
        }

        fn extract_info(&self, _container: &ContainerSummary) -> Result<ContainerInfo, DockerError> {
            let raw_path = ConfigPath::<Raw>::new("/path/to/config.json".to_string());
            let validated_path = raw_path.validate().unwrap();
            
            Ok(ContainerInfo {
                host: "example.com".to_string(),
                ip: "127.0.0.1".to_string(),
                port: 8080,
                path_matcher: None,
                middlewares: None,
                router_name: None,
                health_check: None,
                load_balancer: None,
                json_config_path: Some(validated_path),
            })
        }

        fn create_backend(&self, _info: &ContainerInfo) -> Result<BackendService, DockerError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_load_container_json_config() {
        // 임시 디렉토리 및 JSON 파일 생성
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("test-config.json");
        
        // 간단한 JSON 파일 내용 작성
        let json_content = r#"{"version": "1.0", "server": {"http_port": 8080}}"#;
        std::fs::write(&config_path, json_content).unwrap();
        
        // DockerManager 생성
        let settings = DockerSettings::default();
        let docker_client = Box::new(MockDockerClient);
        let extractor = Box::new(MockExtractor);
        
        let manager = DockerManager::new(docker_client, extractor, settings).await;
        
        // 컨테이너 설정 로드 테스트
        let result = manager.load_container_json_config("test-container", &config_path).await;
        
        // 로드 성공 확인
        assert!(result.is_ok());
        
        // 설정이 실제로 저장되었는지 확인
        let config = manager.container_config_manager.container_configs.get("test-container");
        assert!(config.is_some());
    }

    /* 추후 구현할 테스트들은 주석 처리
    #[tokio::test]
    async fn test_merge_container_settings() {
        // 임시 디렉토리 및 JSON 파일 생성
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("test-config.json");
        
        // JSON 파일 내용 작성 - 특정 포트 설정 포함
        let json_content = r#"{"version": "1.0", "server": {"http_port": 9090}}"#;
        std::fs::write(&config_path, json_content).unwrap();
        
        // DockerManager 생성
        let settings = DockerSettings::default();
        let docker_client = Box::new(MockDockerClient);
        let extractor = Box::new(MockExtractor);
        
        let manager = DockerManager::new(docker_client, extractor, settings).await;
        
        // 컨테이너 설정 로드
        let result = manager.load_container_json_config("test-container", &config_path).await;
        assert!(result.is_ok());
        
        // 도커 라벨 설정 (낮은 우선순위)
        let mut docker_labels = HashMap::new();
        docker_labels.insert("rproxy.server.http_port".to_string(), "8080".to_string());
        
        // 설정 병합
        let merged_settings = manager.get_container_merged_settings("test-container", &docker_labels);
        assert!(merged_settings.is_some());
        
        // JSON 설정(9090)이 도커 라벨(8080)보다 우선 적용되었는지 확인
        let settings = merged_settings.unwrap();
        assert_eq!(settings.server.http_port(), 9090);
    }

    // DockerClient Mock 구현 (컨테이너 목록 반환)
    struct MockDockerClientWithContainers;

    #[async_trait]
    impl DockerClient for MockDockerClientWithContainers {
        fn clone_box(&self) -> Box<dyn DockerClient> {
            Box::new(Self)
        }

        async fn list_containers(
            &self, 
            _options: Option<ListContainersOptions<String>>
        ) -> Result<Vec<ContainerSummary>, DockerError> {
            let mut container1 = ContainerSummary::default();
            container1.id = Some("container1".to_string());
            
            let mut container2 = ContainerSummary::default();
            container2.id = Some("container2".to_string());
            
            Ok(vec![container1, container2])
        }

        fn events(
            &self, 
            _options: Option<EventsOptions<String>>
        ) -> Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>> {
            Box::pin(futures_util::stream::empty())
        }
    }

    #[tokio::test]
    async fn test_get_container_config_paths() {
        // DockerManager 생성
        let settings = DockerSettings::default();
        let docker_client = Box::new(MockDockerClientWithContainers);
        let extractor = Box::new(MockExtractor);
        
        let manager = DockerManager::new(docker_client, extractor, settings).await;
        
        // 컨테이너 설정 파일 경로 조회
        let paths = manager.get_container_config_paths().await.unwrap();
        
        // 두 개의 컨테이너가 모두 동일한 경로를 반환해야 함 (MockExtractor가 동일한 경로를 반환)
        assert_eq!(paths.len(), 2);
        
        // 컨테이너 ID 확인
        let container_ids: Vec<&str> = paths.iter().map(|(id, _)| id.as_str()).collect();
        assert!(container_ids.contains(&"container1"));
        assert!(container_ids.contains(&"container2"));
        
        // 경로 확인
        for (_, path) in &paths {
            assert_eq!(path.to_str().unwrap(), "/path/to/config.json");
        }
    }
    */
}
