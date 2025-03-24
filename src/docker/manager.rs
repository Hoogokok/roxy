use bollard::secret::EventMessage;
use bollard::container::ListContainersOptions;
use bollard::models::ContainerSummary;
use bollard::system::EventsOptions;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc;
use crate::settings::container::ContainerConfigManager;
use crate::settings::json::ServiceConfig;
use crate::settings::DockerSettings;
use crate::routing_v2::{RoutingTable, PathMatcher, BackendService, LoadBalancerStrategy};
use tracing::{debug, error, info, warn};
use tokio::time::Duration;
use std::sync::Arc;
use crate::middleware::MiddlewareConfig;
use tokio::{
    sync::RwLock,
    task::JoinHandle,
};
use std::path::{Path, PathBuf};
use crate::settings::JsonConfig;
use regex_lite::Regex;
use std::sync::atomic::AtomicUsize;

use super::health::{ContainerHealth, HealthCheckerFactory};
use super::{DockerClient, BollardDockerClient, ContainerInfoExtractor, DefaultExtractor, DockerError};
use super::{ContainerInfo, DockerEvent, BackendServiceBuilder, RetryPolicy, with_retry, ContainerRoutesRetry};

#[derive(Clone)]
pub struct DockerManager {
    pub(crate) client: Arc<Box<dyn DockerClient>>,
    pub(crate) extractor: Box<dyn ContainerInfoExtractor>,
    pub(crate) config: DockerSettings,
    pub(crate) health_checks: Arc<RwLock<HashMap<String, ContainerHealth>>>,
    pub(crate) container_config_manager: Arc<ContainerConfigManager>,
    pub(crate) service_builder: BackendServiceBuilder,
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
        let container_config_manager = Arc::new(container_config_manager);
        
        // 서비스 빌더 초기화
        let client_arc = Arc::new(client);
        let service_builder = BackendServiceBuilder::new(
            extractor.clone_box(),
            client_arc.clone(),
            container_config_manager.clone(),
        );
        
        Self {
            client: client_arc,
            extractor,
            config,
            health_checks: Arc::new(RwLock::new(HashMap::new())),
            container_config_manager,
            service_builder,
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

    /// 실제 컨테이너 라우트 조회
    pub async fn try_get_container_routes(&self) -> Result<HashMap<(String, PathMatcher), BackendService>, DockerError> {
        info!("컨테이너 라우트 조회 시작");
        let containers = self.get_labeled_containers().await?;
        info!(count = containers.len(), "컨테이너 목록 조회 성공");

        let services = self.group_containers_by_service(containers).await;
        let mut routes = HashMap::new();
        
        for infos in services.values() {
            if !infos.is_empty() {
                match self.create_backend_service(infos).await {
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

    pub async fn get_labeled_containers(&self) -> Result<Vec<ContainerSummary>, DockerError> {
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
        let container_config_manager = Arc::new(container_config_manager);

        let manager = DockerManager { 
            client: docker.clone(),
            extractor: Box::new(DefaultExtractor::new(
                config.network.clone(),
                config.label_prefix.clone(),
            )),
            config: config.clone(),
            health_checks,
            container_config_manager: container_config_manager.clone(),
            service_builder: BackendServiceBuilder::new(
                Box::new(DefaultExtractor::new(
                    config.network.clone(),
                    config.label_prefix.clone(),
                )),
                docker.clone(),
                container_config_manager,
            ),
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
        
        // 기존 정보 가져오기
        let old_info = manager.get_container_info(container_id).await?;
        
        // 컨테이너 정보 조회
        let options = Some(ListContainersOptions::<String> {
            all: true,
            filters: {
                let mut filters = HashMap::new();
                filters.insert("id".to_string(), vec![container_id.to_string()]);
                filters
            },
            ..Default::default()
        });

        let containers = manager.client.list_containers(options).await?;
        
        match containers.first() {
            Some(container) => {
                // 컨테이너 정보 추출
                let container_info = manager.extractor.extract_info(container)?;
                
                // 컨테이너 라벨 가져오기
                let labels = container.labels.clone().unwrap_or_default();
                
                // JSON 설정과 Docker 라벨 병합
                let merged_settings = manager.get_container_merged_settings(container_id, &labels).await?;
                
                // 백엔드 서비스 생성
                let mut service = manager.extractor.create_backend(&container_info)?;
                
                // 병합된 설정에서 HTTP 포트 적용
                let http_port = merged_settings.server.http_port();
                if http_port != container_info.port {
                    info!(
                        container_id = %container_id, 
                        original_port = %container_info.port,
                        new_port = %http_port,
                        "HTTP 포트 업데이트"
                    );
                    service.update_port(http_port);
                }
                
                // 업데이트 이벤트 전송
                tx.send(DockerEvent::ContainerUpdated { 
                    container_id: container_id.to_string(),
                    old_host: old_info.map(|(h, _, _)| h),
                    new_host: Some(container_info.host.clone()),
                    service: Some(service),
                    path_matcher: container_info.path_matcher.clone(),
                }).await.map_err(|_| Self::channel_send_error())?;
                
                Ok(())
            },
            None => {
                warn!(container_id = %container_id, "업데이트된 컨테이너를 찾을 수 없음");
                Ok(())
            }
        }
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
    pub async fn get_container_info(&self, container_id: &str) -> Result<Option<(String, BackendService, Option<PathMatcher>)>, DockerError> {
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
    pub async fn group_containers_by_service(&self, containers: Vec<ContainerSummary>) 
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
    pub async fn create_backend_service(&self, infos: &[ContainerInfo]) -> Result<(String, PathMatcher, BackendService), DockerError> {
        debug!("백엔드 서비스 생성 시작");
        
        // BackendServiceBuilder 호출하여 서비스 구축
        self.service_builder.build_from_containers(infos).await
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

    /// 컨테이너 설정 파일 변경을 감지하고 처리하는 기능
    pub async fn watch_container_config_changes(&self, watcher: &mut crate::settings::watcher::ConfigWatcher) -> Result<bool, DockerError> {
        let container_paths = self.get_container_config_paths().await?;
        if container_paths.is_empty() {
            return Ok(false);
        }
        
        // 컨테이너 ID와 경로 매핑 생성
        let mut container_configs = HashMap::new();
        for (container_id, path) in &container_paths {
            container_configs.insert(container_id.clone(), path.clone());
        }
        
        // 설정 변경 감지 (최대 1초 대기)
        let changed_configs = watcher.watch_container_configs(&container_configs, Duration::from_secs(1)).await;
        if changed_configs.is_empty() {
            return Ok(false);
        }
        
        // 변경된 설정 처리
        let mut updated = false;
        for (container_id, path) in changed_configs {
            info!(
                container_id = %container_id,
                path = %path.display(),
                "컨테이너 설정 파일 변경 감지"
            );
            
            // 설정 파일 재로드
            match self.load_container_json_config(&container_id, &path).await {
                Ok(_) => {
                    info!(container_id = %container_id, "컨테이너 설정 파일 재로드 성공");
                    updated = true;
                }
                Err(e) => {
                    error!(
                        container_id = %container_id,
                        error = %e,
                        "컨테이너 설정 파일 재로드 실패"
                    );
                }
            }
        }
        
        Ok(updated)
    }

    /// 컨테이너별 JSON 설정 파일 로드
    pub async fn load_container_json_config(&self, container_id: &str, path: &Path) -> Result<(), DockerError> {
        info!(container_id = %container_id, path = %path.display(), "컨테이너 JSON 설정 로드");
        
        // 설정 파일 로드 시도
        let json_config = JsonConfig::from_file(path)
            .map_err(|e| DockerError::ContainerConfigError {
                container_id: container_id.to_string(),
                reason: "JSON 설정 로드 실패".to_string(),
                context: Some(e.to_string()),
            })?;
        
        
        // 컨테이너 ID와 설정 매핑 저장
        self.container_config_manager.as_ref().container_configs.insert(container_id.to_string(), json_config);
        
        Ok(())
    }

    /// 컨테이너별 JSON 설정과 도커 라벨을 병합하여 최종 설정 반환
    pub async fn get_container_merged_settings(
        &self, 
        container_id: &str, 
        docker_labels: &HashMap<String, String>
    ) -> Result<crate::settings::Settings<crate::settings::typestate::Validated>, DockerError> {
        // ContainerConfigManager의 merge_config 메소드를 활용하여 병합 로직 단순화
        let settings = self.container_config_manager.as_ref().merge_config(container_id, docker_labels);
        
        // 이미 Validated 상태의 설정이 반환되므로 추가 검증 불필요
        Ok(settings)
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

    /// JSON 설정 파일을 기반으로 라우팅 테이블을 업데이트합니다.
    pub async fn update_routing_from_json(
        &self, 
        container_id: &str, 
        routing_table: Arc<RwLock<RoutingTable>>
    ) -> Result<(), DockerError> {
        debug!("update_routing_from_json 시작 - container_id: {}", container_id);
        
        // 컨테이너 설정 가져오기
        let json_config = match self.container_config_manager.as_ref().get_container_config(container_id) {
            Some(config) => {
                debug!("컨테이너 JSON 설정 찾음 - 라우터 수: {}, 서비스 수: {}", 
                    config.routers.len(), config.services.len());
                config
            },
            None => {
                debug!("컨테이너 JSON 설정 없음");
                return Ok(());
            }
        };
        
        // 라우터 및 서비스 설정 확인
        let routers = &json_config.routers;
        let services = &json_config.services;
        
        if routers.is_empty() || services.is_empty() {
            debug!("라우터 또는 서비스 설정 없음");
            return Ok(());
        }
        
        let mut table = routing_table.write().await;
        debug!(
            "JSON 설정에서 라우팅 업데이트 처리 - 라우터 수: {}, 서비스 수: {}",
            routers.len(), services.len()
        );
        
        // 각 라우터 설정에 대해 처리
        for (router_name, router_config) in routers {
            debug!("라우터 처리 - 이름: {}, 규칙: {}", router_name, router_config.rule);
            
            // 서비스 정보 가져오기
            let service_name = &router_config.service;
            let service_config = match services.get(service_name.as_str()) {
                Some(config) => {
                    debug!("서비스 찾음 - 이름: {}", service_name);
                    config
                },
                None => {
                    debug!("라우터에 지정된 서비스를 찾을 수 없음 - 라우터: {}, 서비스: {}", 
                        router_name, service_name);
                    continue;
                }
            };
            
            // 라우터 규칙 파싱
            match parse_router_rule(router_config.rule.as_str()) {
                Ok((host, path_matcher)) => {
                    debug!("라우터 규칙 파싱 성공 - 호스트: {}, 경로: {}", host, path_matcher);
                    
                    // 백엔드 서비스 생성
                    match create_backend_from_service_config(&host, service_config) {
                        Ok(backend_service) => {
                            debug!(
                                "백엔드 서비스 생성 성공 - 주소: {:?}",
                                backend_service.address
                            );
                            
                            // 라우팅 테이블에 추가
                            table.add_route(host, backend_service, Some(path_matcher));
                            debug!("라우트 추가됨");
                        },
                        Err(e) => {
                            debug!(
                                "백엔드 서비스 생성 실패 - 라우터: {}, 오류: {:?}",
                                router_name, e
                            );
                        }
                    }
                },
                Err(e) => {
                    debug!(
                        "라우터 규칙 파싱 실패 - 라우터: {}, 규칙: {}, 오류: {:?}",
                        router_name, router_config.rule, e
                    );
                }
            }
        }
        
        debug!(
            "JSON 설정에서 라우팅 업데이트 완료 - 라우트 수: {}", 
            table.routes.len()
        );
        Ok(())
    }
}

// 2. 라우터 규칙 파싱 함수 추가
fn parse_router_rule(rule: &str) -> Result<(String, PathMatcher), DockerError> {
    // Host(`example.com`) && PathPrefix(`/api`) 형식 파싱
    let host_regex = Regex::new(r#"Host\(`([^`]+)`\)"#).unwrap();
    let path_regex = Regex::new(r#"(PathPrefix|Path)\(`([^`]+)`\)"#).unwrap();
    
    // 호스트 추출
    let host = match host_regex.captures(rule) {
        Some(caps) => caps.get(1).unwrap().as_str().to_string(),
        None => {
            return Err(DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: format!("라우터 규칙에서 Host를 찾을 수 없음: {}", rule),
                context: None,
            });
        }
    };
    
    // 경로 추출 (없으면 기본 경로 사용)
    let path = match path_regex.captures(rule) {
        Some(caps) => {
            let path_type = caps.get(1).unwrap().as_str();
            let path_value = caps.get(2).unwrap().as_str();
            
            // PathPrefix와 Path에 따라 다른 매처 생성
            if path_type == "PathPrefix" {
                PathMatcher::from_str(&format!("{}/*", path_value)).unwrap_or_else(|_| PathMatcher::from_str("/").unwrap())
            } else {
                PathMatcher::from_str(path_value).unwrap_or_else(|_| PathMatcher::from_str("/").unwrap())
            }
        },
        None => PathMatcher::from_str("/").unwrap() // 경로 지정이 없으면 루트 경로 사용
    };
    
    debug!("라우터 규칙 파싱 결과 - host: {}, path: {}", host, path);
    Ok((host, path))
}

// 3. 서비스 설정에서 백엔드 서비스 생성 함수 추가
fn create_backend_from_service_config(host: &str, service_config: &ServiceConfig) -> Result<BackendService, DockerError> {
    // 로드밸런서 설정 확인
    let servers = &service_config.loadbalancer.servers;
    if servers.is_empty() {
        return Err(DockerError::ContainerConfigError {
            container_id: "unknown".to_string(),
            reason: format!("서비스에 서버가 정의되지 않음: {}", host),
            context: None,
        });
    }
    
    // 첫 번째 서버 주소 얻기
    let first_server = &servers[0];
    
    // URL 파싱
    let url = first_server.url.as_str();
    
    // URL에서 호스트/포트 추출
    let url_regex = Regex::new(r"https?://([^:/]+)(?::(\d+))?").unwrap();
    let (addr_host, port) = match url_regex.captures(url) {
        Some(caps) => {
            let host = caps.get(1).unwrap().as_str();
            let port = caps.get(2).map_or("80", |p| p.as_str()).parse::<u16>().unwrap_or(80);
            (host, port)
        },
        None => {
            return Err(DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: format!("잘못된 서버 URL 형식: {}", url),
                context: None,
            });
        }
    };
    
    debug!("주소 파싱 결과 - 호스트: {}, 포트: {}", addr_host, port);
    
    // SocketAddr 생성 - localhost를 127.0.0.1로 변환
    let host_ip = if addr_host == "localhost" {
        "127.0.0.1".to_string()
    } else {
        addr_host.to_string()
    };
    
    debug!("소켓 주소 생성 - IP: {}, 포트: {}", host_ip, port);
    
    let socket_addr = match format!("{}:{}", host_ip, port).parse::<std::net::SocketAddr>() {
        Ok(addr) => {
            debug!("소켓 주소 생성 성공: {:?}", addr);
            addr
        },
        Err(e) => {
            let error_msg: String = e.to_string();
            debug!("소켓 주소 생성 실패: {} - {}", format!("{}:{}", host_ip, port), error_msg);
            
            // IP 주소로 직접 변환 시도
            match host_ip.parse::<std::net::IpAddr>() {
                Ok(ip) => {
                    let addr = std::net::SocketAddr::new(ip, port);
                    debug!("IP 주소로 직접 변환 성공: {:?}", addr);
                    addr
                },
                Err(_) => {
                    // 마지막 수단: 127.0.0.1 사용
                    debug!("IP 변환 실패, 기본값 127.0.0.1 사용");
                    std::net::SocketAddr::new(
                        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                        port
                    )
                }
            }
        }
    };
    
    // 기본 서비스 생성
    let mut service = BackendService::new(socket_addr);
    
    // 추가 서버가 있으면 로드밸런서 설정
    if servers.len() > 1 {
        // 기본 라운드 로빈 전략 사용
        service.enable_load_balancer(LoadBalancerStrategy::RoundRobin {
            current_index: AtomicUsize::new(0)
        });
        
        // 첫 번째 이후의 모든 서버 추가
        for server in servers.iter().skip(1) {
            let url = server.url.as_str();
            if let Some(caps) = url_regex.captures(url) {
                let host = caps.get(1).unwrap().as_str();
                let port = caps.get(2).map_or("80", |p| p.as_str()).parse::<u16>().unwrap_or(80);
                
                // localhost를 127.0.0.1로 변환
                let server_ip = if host == "localhost" {
                    "127.0.0.1".to_string()
                } else {
                    host.to_string()
                };
                
                if let Ok(addr) = format!("{}:{}", server_ip, port).parse() {
                    let _ = service.add_address(addr, server.weight as usize);
                } else {
                    // IP 주소로 직접 변환 시도
                    match server_ip.parse::<std::net::IpAddr>() {
                        Ok(ip) => {
                            let addr = std::net::SocketAddr::new(ip, port);
                            let _ = service.add_address(addr, server.weight as usize);
                        },
                        Err(_) => {
                            // fallback: 127.0.0.1 사용
                            let addr = std::net::SocketAddr::new(
                                std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                                port
                            );
                            let _ = service.add_address(addr, server.weight as usize);
                        }
                    }
                }
            }
        }
    }
    
    Ok(service)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use bollard::system::EventsOptions;
    use bollard::models::{ContainerSummary, ContainerSummaryNetworkSettings, EndpointSettings};
    use std::pin::Pin;
    use futures_util::stream::Stream;
    use crate::routing_v2::BackendService;
    use crate::settings::typestate::Validatable;
    use std::net::{IpAddr, SocketAddr};
    use std::str::FromStr;
    use crate::settings::types::ConfigPath;
    
    // 테스트용 DockerClient 모의 구현체
    struct MockDockerClient {
        containers: Mutex<Vec<ContainerSummary>>,
    }
    
    impl MockDockerClient {
        fn new(containers: Vec<ContainerSummary>) -> Self {
            Self { containers: Mutex::new(containers) }
        }
    }
    
    #[async_trait]
    impl DockerClient for MockDockerClient {
        async fn list_containers(&self, _options: Option<ListContainersOptions<String>>) -> Result<Vec<ContainerSummary>, DockerError> {
            Ok(self.containers.lock().unwrap().clone())
        }
        
        fn events(&self, _options: Option<EventsOptions<String>>) -> Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>> {
            Box::pin(futures_util::stream::empty())
        }
        
        fn clone_box(&self) -> Box<dyn DockerClient> {
            Box::new(Self { 
                containers: Mutex::new(self.containers.lock().unwrap().clone()) 
            })
        }
    }
    
    // ContainerInfoExtractor 모의 구현체
    struct MockContainerInfoExtractor {
        info: ContainerInfo,
    }
    
    impl MockContainerInfoExtractor {
        fn new(info: ContainerInfo) -> Self {
            Self { info }
        }
    }
    
    impl ContainerInfoExtractor for MockContainerInfoExtractor {
        fn extract_info(&self, _container: &ContainerSummary) -> Result<ContainerInfo, DockerError> {
            Ok(self.info.clone())
        }
        
        fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, DockerError> {
            let socket_addr = SocketAddr::new(
                IpAddr::from_str(&info.ip).unwrap_or_else(|_| IpAddr::from_str("127.0.0.1").unwrap()),
                info.port
            );
            Ok(BackendService::new(socket_addr))
        }
        
        fn clone_box(&self) -> Box<dyn ContainerInfoExtractor> {
            Box::new(MockContainerInfoExtractor::new(self.info.clone()))
        }
        
        fn get_label_prefix(&self) -> &str {
            "rproxy."
        }
    }
    
    // 테스트 헬퍼 함수: 테스트용 컨테이너 요약 생성
    fn create_test_container(id: &str, host: &str, ip: &str, _port: u16) -> ContainerSummary {
        let mut container = ContainerSummary::default();
        container.id = Some(id.to_string());
        
        // 테스트용 라벨 설정
        let mut labels = HashMap::new();
        labels.insert("rproxy.enable".to_string(), "true".to_string());
        labels.insert("rproxy.http.routers.test.rule".to_string(), format!("Host(`{}`)", host));
        container.labels = Some(labels);
        
        // 네트워크 설정
        let mut networks = HashMap::new();
        let endpoint = EndpointSettings {
            ip_address: Some(ip.to_string()),
            ..Default::default()
        };
        networks.insert("bridge".to_string(), endpoint);
        
        let network_settings = ContainerSummaryNetworkSettings {
            networks: Some(networks),
            ..Default::default()
        };
        container.network_settings = Some(network_settings);
        
        container
    }
    
    // 테스트 헬퍼 함수: 테스트용 컨테이너 정보 생성
    fn create_test_container_info(id: &str, host: &str, ip: &str, port: u16, config_path: Option<&str>) -> ContainerInfo {
        ContainerInfo {
            container_id: Some(id.to_string()),
            host: host.to_string(),
            ip: ip.to_string(),
            port,
            path_matcher: None,
            router_name: Some("test-router".to_string()),
            health_check: None,
            json_config_path: config_path.map(|p| ConfigPath::new(p.to_string()).validate().unwrap()),
            middlewares: None,
            load_balancer: None,
        }
    }
    
    // 테스트 헬퍼 함수: 테스트용 DockerManager 생성
    async fn create_test_docker_manager(container_info: ContainerInfo, containers: Vec<ContainerSummary>) -> DockerManager {
        let client = Box::new(MockDockerClient::new(containers));
        let extractor = Box::new(MockContainerInfoExtractor::new(container_info));
        let docker_settings = crate::settings::DockerSettings::default();
        
        DockerManager::new(client, extractor, docker_settings).await
    }
    
    // 테스트 함수: 컨테이너 라우트 조회
    #[tokio::test]
    async fn test_get_container_routes() {
        // 테스트용 컨테이너 정보 생성
        let container_id = "test-container";
        let host = "test.local";
        let ip = "127.0.0.1";
        let port = 8080;
        
        let container_info = create_test_container_info(container_id, host, ip, port, None);
        let container = create_test_container(container_id, host, ip, port);
        
        // DockerManager 생성
        let manager = create_test_docker_manager(
            container_info.clone(),
            vec![container]
        ).await;
        
        // 컨테이너 라우트 조회
        let routes = manager.try_get_container_routes().await.expect("라우트 조회 실패");
        
        // 검증
        assert!(!routes.is_empty(), "라우트가 비어 있습니다");
        
        // 라우트 키 확인 (host, path_matcher)
        let key = routes.keys().next().unwrap();
        assert_eq!(key.0, host, "라우트 호스트가 예상과 다릅니다");
    }

    #[tokio::test]
    async fn test_watch_container_config_changes() {
        use std::fs::{self, File};
        use std::io::Write;
        use tempfile::tempdir;
        use crate::settings::watcher::ConfigWatcher;
        use tokio::time::Duration;
        use std::time::SystemTime;

        // 임시 디렉토리 생성
        let temp_dir = tempdir().expect("임시 디렉토리 생성 실패");
        let temp_path = temp_dir.path();
        
        // 테스트용 컨테이너 설정 파일 생성
        let config_file_path = temp_path.join("test-container.json");
        let config_content = r#"{
            "server": {
                "http_port": 9090
            }
        }"#;
        
        let mut file = File::create(&config_file_path).expect("설정 파일 생성 실패");
        file.write_all(config_content.as_bytes()).expect("설정 파일 쓰기 실패");
        
        // 테스트용 컨테이너 정보 및 DockerManager 생성
        let container_id = "test-config-container";
        let host = "config-test.local";
        let ip = "127.0.0.1";
        let port = 8080;
        
        let container_info = create_test_container_info(
            container_id, 
            host, 
            ip, 
            port, 
            Some(config_file_path.to_str().unwrap())
        );
        let container = create_test_container(container_id, host, ip, port);
        
        // DockerManager 생성
        let manager = create_test_docker_manager(
            container_info.clone(),
            vec![container]
        ).await;
        
        // 설정 파일 로드
        manager.load_container_json_config(container_id, &config_file_path)
            .await
            .expect("컨테이너 설정 로드 실패");
        
        // ConfigWatcher 초기화
        let mut watcher = ConfigWatcher::new();
        watcher.add_path(&config_file_path);
        watcher.start_with_interval(Duration::from_millis(100)).await.expect("watcher 시작 실패");
        
        // 설정 변경 전 확인
        let settings = manager.get_container_merged_settings(container_id, &HashMap::new())
            .await
            .expect("병합된 설정 조회 실패");
        assert_eq!(settings.server.http_port(), 9090, "초기 HTTP 포트가 9090이어야 함");
        
        // 파일 변경을 위한 대기
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        // 설정 파일 내용 변경 (HTTP 포트를 8080으로 변경)
        let updated_config = r#"{
            "server": {
                "http_port": 8080
            }
        }"#;
        
        // 파일 시스템에서 변경 감지를 위해 파일을 다시 작성
        let mut file = File::create(&config_file_path).expect("설정 파일 열기 실패");
        file.write_all(updated_config.as_bytes()).expect("설정 파일 업데이트 실패");
        
        // 파일 변경 시간 업데이트 확인
        let metadata = fs::metadata(&config_file_path).expect("메타데이터 조회 실패");
        if let Ok(modified) = metadata.modified() {
            let now = SystemTime::now();
            let diff = now.duration_since(modified).unwrap_or_default();
            assert!(diff < Duration::from_secs(2), "파일 수정 시간이 현재와 가깝지 않음");
        }
        
        // 변경 감지를 위한 대기
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // 변경 감지 및 처리
        let updated = manager.watch_container_config_changes(&mut watcher)
            .await
            .expect("설정 변경 감지 실패");
        
        assert!(updated, "설정 변경이 감지되지 않음");
        
        // 변경 후 설정 확인
        let settings = manager.get_container_merged_settings(container_id, &HashMap::new())
            .await
            .expect("병합된 설정 조회 실패");
        
        assert_eq!(settings.server.http_port(), 8080, "업데이트된 HTTP 포트가 8080이어야 함");
    }

    #[tokio::test]
    async fn test_container_config_priority() {
        use std::fs::File;
        use std::io::Write;
        use tempfile::tempdir;
        
        // 임시 디렉토리 생성
        let temp_dir = tempdir().expect("임시 디렉토리 생성 실패");
        let temp_path = temp_dir.path();
        
        // 테스트용 컨테이너 설정 파일 생성 (HTTP 포트 8888 설정)
        let config_file_path = temp_path.join("priority-test.json");
        let config_content = r#"{
            "server": {
                "http_port": 8888
            }
        }"#;
        
        let mut file = File::create(&config_file_path).expect("설정 파일 생성 실패");
        file.write_all(config_content.as_bytes()).expect("설정 파일 쓰기 실패");
        
        // 테스트용 컨테이너 정보 생성
        let container_id = "priority-test-container";
        let host = "priority-test.local";
        let ip = "127.0.0.1";
        let port = 8080;
        
        // Docker 라벨에 HTTP 포트 7777 설정
        let mut container = create_test_container(container_id, host, ip, port);
        let mut labels = container.labels.take().unwrap_or_default();
        labels.insert("rproxy.http.services.test.loadbalancer.server.port".to_string(), "7777".to_string());
        container.labels = Some(labels);
        
        let container_info = create_test_container_info(
            container_id, 
            host, 
            ip, 
            port, 
            Some(config_file_path.to_str().unwrap())
        );
        
        // DockerManager 생성 (소유권 문제 해결을 위해 복제)
        let manager = create_test_docker_manager(
            container_info.clone(),
            vec![container]
        ).await;
        
        // 컨테이너 설정 파일 로드
        manager.load_container_json_config(container_id, &config_file_path)
            .await
            .expect("컨테이너 설정 로드 실패");
        
        // Docker 라벨 생성
        let mut docker_labels = HashMap::new();
        docker_labels.insert("rproxy.http.services.test.loadbalancer.server.port".to_string(), "7777".to_string());
        
        // 병합된 설정 조회
        let settings = manager.get_container_merged_settings(container_id, &docker_labels)
            .await
            .expect("병합된 설정 조회 실패");
        
        // JSON 설정(8888)이 Docker 라벨(7777)보다 우선 적용되어야 함
        assert_eq!(settings.server.http_port(), 8888, "JSON 설정이 Docker 라벨보다 우선 적용되어야 함");
    }

    #[tokio::test]
    async fn test_load_container_json_config() {
        use std::fs::File;
        use std::io::Write;
        use tempfile::tempdir;
        
        // 임시 디렉토리 생성
        let temp_dir = tempdir().expect("임시 디렉토리 생성 실패");
        let temp_path = temp_dir.path();
        
        // 테스트용 컨테이너 설정 파일 생성
        let config_file_path = temp_path.join("config-test.json");
        let config_content = r#"{
            "server": {
                "http_port": 9999
            },
            "middleware": {
                "test-middleware": {
                    "type": "basic-auth",
                    "enabled": true,
                    "settings": {
                        "users": "test:password"
                    }
                }
            }
        }"#;
        
        let mut file = File::create(&config_file_path).expect("설정 파일 생성 실패");
        file.write_all(config_content.as_bytes()).expect("설정 파일 쓰기 실패");
        
        // 테스트용 컨테이너 정보 생성
        let container_id = "config-loader-test";
        let host = "config-loader.local";
        let ip = "127.0.0.1";
        let port = 8080;
        
        let container_info = create_test_container_info(
            container_id, 
            host, 
            ip, 
            port, 
            Some(config_file_path.to_str().unwrap())
        );
        
        let container = create_test_container(container_id, host, ip, port);
        
        // DockerManager 생성
        let manager = create_test_docker_manager(
            container_info.clone(),
            vec![container]
        ).await;
        
        // 설정 파일 로드 전 확인
        let settings_before = manager.get_container_merged_settings(container_id, &HashMap::new())
            .await
            .expect("병합된 설정 조회 실패");
        assert_ne!(settings_before.server.http_port(), 9999, "설정 로드 전 HTTP 포트는 9999가 아니어야 함");
        
        // 설정 파일 로드
        let result = manager.load_container_json_config(container_id, &config_file_path).await;
        assert!(result.is_ok(), "설정 파일 로드에 실패함");
        
        // 설정이 컨테이너 설정 관리자에 저장되었는지 확인
        let config_exists = manager.container_config_manager.as_ref().container_configs.contains_key(container_id);
        assert!(config_exists, "컨테이너 설정이 관리자에 저장되지 않음");
        
        // 설정 로드 후 확인
        let settings_after = manager.get_container_merged_settings(container_id, &HashMap::new())
            .await
            .expect("병합된 설정 조회 실패");
        assert_eq!(settings_after.server.http_port(), 9999, "설정 로드 후 HTTP 포트는 9999여야 함");
        
        // 무효한 JSON 파일 처리 확인
        let invalid_path = temp_path.join("invalid.json");
        let invalid_content = r#"{ invalid json }"#;
        let mut file = File::create(&invalid_path).expect("파일 생성 실패");
        file.write_all(invalid_content.as_bytes()).expect("파일 쓰기 실패");
        
        let invalid_result = manager.load_container_json_config("invalid-container", &invalid_path).await;
        assert!(invalid_result.is_err(), "무효한 JSON을 오류로 처리해야 함");
    }

    #[tokio::test]
    async fn test_update_routing_from_json() {
        use std::fs::File;
        use std::io::Write;
        use tempfile::tempdir;
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use crate::routing_v2::RoutingTable;
        use crate::routing_v2::HostInfo;
        use tracing::debug;
        
        // 임시 디렉토리 및 테스트 JSON 파일 준비
        let temp_dir = tempdir().expect("임시 디렉토리 생성 실패");
        let config_path = temp_dir.path().join("router-test.json");
        
        debug!("테스트: 임시 디렉토리 생성됨 - {:?}", temp_dir.path());
        
        // 라우터 정보가 포함된 JSON 설정 파일 작성
        let json_content = r#"{
            "routers": {
                "api": {
                    "rule": "Host(`api.example.com`)",
                    "service": "api-service"
                },
                "web": {
                    "rule": "Host(`www.example.com`)",
                    "service": "web-service"
                }
            },
            "services": {
                "api-service": {
                    "loadbalancer": {
                        "servers": [
                            { "url": "http://localhost:8001" },
                            { "url": "http://localhost:8002" }
                        ]
                    }
                },
                "web-service": {
                    "loadbalancer": {
                        "servers": [
                            { "url": "http://localhost:9001" }
                        ]
                    }
                }
            }
        }"#;
        
        let mut file = File::create(&config_path).expect("JSON 파일 생성 실패");
        file.write_all(json_content.as_bytes()).expect("JSON 파일 쓰기 실패");
        
        debug!("테스트: JSON 파일 생성됨 - {:?}", config_path);
        
        // 테스트용 DockerManager 및 RoutingTable 생성
        let container_id = "router-test-container";
        let host = "example.com";
        let ip = "127.0.0.1";
        let port = 8080;
        
        let container_info = create_test_container_info(
            container_id, 
            host, 
            ip, 
            port, 
            Some(config_path.to_str().unwrap())
        );
        
        let container = create_test_container(container_id, host, ip, port);
        
        let manager = create_test_docker_manager(
            container_info.clone(),
            vec![container]
        ).await;
        
        debug!("테스트: DockerManager 생성됨");
        
        // JSON 설정 로드
        let load_result = manager.load_container_json_config(container_id, &config_path).await;
        assert!(load_result.is_ok(), "JSON 설정 로드 실패: {:?}", load_result.err());
        
        // 설정이 제대로 로드되었는지 확인
        let config = manager.container_config_manager.as_ref().get_container_config(container_id);
        assert!(config.is_some(), "컨테이너 설정이 로드되지 않음");
        
        if let Some(config) = &config {
            debug!("테스트: 컨테이너 설정 로드됨");
            debug!("테스트: 라우터 수: {}", config.routers.len());
            debug!("테스트: 서비스 수: {}", config.services.len());
            
            // 라우터와 서비스 내용 확인
            for (name, router) in &config.routers {
                debug!("테스트: 라우터 {} - 규칙: {}, 서비스: {}", name, router.rule, router.service);
            }
            
            for (name, service) in &config.services {
                debug!("테스트: 서비스 {} - 서버 수: {}", name, service.loadbalancer.servers.len());
                for (i, server) in service.loadbalancer.servers.iter().enumerate() {
                    debug!("테스트: 서비스 {} - 서버 {}: URL: {}", name, i, server.url);
                }
            }
        }
        
        // 라우팅 테이블 생성
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        
        // JSON 설정에서 라우팅 업데이트 테스트
        let result = manager.update_routing_from_json(container_id, routing_table.clone()).await;
        assert!(result.is_ok(), "라우팅 업데이트 실패: {:?}", result.err());
        
        // 라우팅 테이블 확인
        let table = routing_table.read().await;
        
        // 라우트 수 확인
        debug!("테스트: 라우팅 테이블의 라우트 수: {}", table.routes.len());
        for (key, _) in &table.routes {
            debug!("테스트: 라우트 - 호스트: {}, 경로: {}", key.0, key.1);
        }
        
        // parse_router_rule 함수 테스트
        debug!("테스트: parse_router_rule 함수 테스트");
        let rule1 = "Host(`api.example.com`)";
        match parse_router_rule(rule1) {
            Ok((host, path)) => debug!("Rule1 파싱 결과 - 호스트: {}, 경로: {}", host, path),
            Err(e) => debug!("Rule1 파싱 실패: {:?}", e),
        }
        
        let rule2 = "Host(`www.example.com`)";
        match parse_router_rule(rule2) {
            Ok((host, path)) => debug!("Rule2 파싱 결과 - 호스트: {}, 경로: {}", host, path),
            Err(e) => debug!("Rule2 파싱 실패: {:?}", e),
        }
        
        // create_backend_from_service_config 함수 테스트
        debug!("테스트: create_backend_from_service_config 함수 테스트");
        if let Some(config) = config {
            if let Some(service_config) = config.services.get("api-service") {
                match create_backend_from_service_config("api.example.com", service_config) {
                    Ok(backend) => debug!("Backend 생성 성공: {:?}", backend.address),
                    Err(e) => debug!("Backend 생성 실패: {:?}", e),
                }
            }
        }
        
        assert_eq!(table.routes.len(), 2, "2개의 라우트가 추가되어야 함");
        
        // 호스트 정보로 직접 백엔드 검색
        let api_host_info = HostInfo {
            name: "api.example.com".to_string(),
            port: None,
            path: None
        };
        
        let web_host_info = HostInfo {
            name: "www.example.com".to_string(),
            port: None,
            path: None
        };
        
        // API 라우트 테스트
        let api_service = table.find_backend(&api_host_info).expect("API 서비스 찾을 수 없음");
        let api_addr = api_service.get_next_address().expect("API 서비스 주소 없음");
        assert_eq!(api_addr.port(), 8001, "API 서비스 포트 불일치");
        
        // 웹 라우트 테스트
        let web_service = table.find_backend(&web_host_info).expect("웹 서비스 찾을 수 없음");
        let web_addr = web_service.get_next_address().expect("웹 서비스 주소 없음");
        assert_eq!(web_addr.port(), 9001, "웹 서비스 포트 불일치");
    }
} 