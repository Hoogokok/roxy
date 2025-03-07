use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, instrument, warn};

use crate::docker::DockerManager;
use crate::middleware::{MiddlewareConfig, MiddlewareManager};
use crate::routing_v2::RoutingTable;
use crate::settings::{Either, HttpsDisabled, HttpsEnabled, Settings};
use crate::settings::watcher::{ConfigEvent, ConfigWatcher, WatcherConfig};
use super::docker::DockerEventHandler;
use super::error::Error;
use super::handler::RequestHandler;
use super::listener::ServerListener;
use super::Result;

// 공통 인터페이스 정의
pub trait ServerInterface: Send + Sync {
    async fn start(&mut self) -> Result<()>;
    async fn start_config_watcher(&mut self) -> Result<(mpsc::Receiver<()>, tokio::task::JoinHandle<()>)>;
}

// 서버 매니저 (제네릭)
pub struct ServerManager<HttpsState = HttpsDisabled> {
    pub config: Settings<HttpsState>,
    pub docker_manager: DockerManager,
    pub routing_table: Arc<RwLock<RoutingTable>>,
    middleware_manager: MiddlewareManager,
    config_watcher: Option<ConfigWatcher>,
    shared_config: Option<Arc<RwLock<Settings<HttpsState>>>>,
    shared_middleware_manager: Option<Arc<RwLock<MiddlewareManager>>>,
}

// 모든 ServerManager에 공통적인 기능 구현
impl<HttpsState> ServerManager<HttpsState> 
where
    HttpsState: Clone + Send + Sync + 'static,
{
    // 공통 생성자
    pub fn new(
        config: Settings<HttpsState>,
        docker_manager: DockerManager,
        routing_table: Arc<RwLock<RoutingTable>>,
        middleware_manager: MiddlewareManager,
    ) -> Self {
        Self {
            config,
            docker_manager,
            routing_table,
            middleware_manager,
            config_watcher: None,
            shared_config: None,
            shared_middleware_manager: None,
        }
    }
    
    // 감시자 초기화
    async fn initialize_watcher(config: &WatcherConfig) -> Result<ConfigWatcher> {
        let mut watcher = ConfigWatcher::new();
        watcher.add_path(&config.config_path);
        watcher.start_with_interval(config.poll_interval).await?;
        Ok(watcher)
    }
    
    // 이벤트 로깅
    fn log_config_events(events: &[ConfigEvent]) {
        for event in events {
            match event {
                ConfigEvent::Created(path) => info!(path = %path.display(), "설정 파일 생성됨"),
                ConfigEvent::Modified(path) => info!(path = %path.display(), "설정 파일 수정됨"),
                ConfigEvent::Deleted(path) => info!(path = %path.display(), "설정 파일 삭제됨")
            }
        }
    }
    
    // 이벤트 분류
    fn classify_events(events: Vec<ConfigEvent>) -> (Vec<PathBuf>, bool) {
        let mut file_paths = Vec::new();
        let mut has_remove_event = false;
        
        for event in events {
            match event {
                ConfigEvent::Created(path) | ConfigEvent::Modified(path) => {
                    if path.extension().map_or(false, |ext| ext == "json") {
                        file_paths.push(path);
                    }
                },
                ConfigEvent::Deleted(_) => {
                    has_remove_event = true;
                },
                _ => {}
            }
        }
        
        (file_paths, has_remove_event)
    }
    
    // 알림 전송
    async fn send_config_update_notification(tx: &mpsc::Sender<()>, updated: bool) -> Result<()> {
        if updated {
            tx.send(()).await.map_err(|e| {
                Error::ConfigWatch {
                    message: format!("설정 업데이트 알림 전송 실패: {}", e),
                    file_path: None,
                    source: None,
                }
            })?;
        }
        Ok(())
    }
}

// HTTP용 구현
impl ServerManager<HttpsDisabled> {
    // HTTP 서버 생성
    #[instrument(skip(settings), level = "debug", err)]
    pub async fn create_http(settings: Settings<HttpsDisabled>) -> Result<Self> {
        let docker_manager = DockerManager::with_defaults(settings.docker.clone()).await?;
        
        // 초기 상태 체크 설정
        if let Err(e) = docker_manager.setup_initial_health_checks().await {
            error!(error = %e, "초기 상태 체크 설정 실패");
        }
        
        // 설정 소스 병합 (환경 변수, JSON, Docker 라벨)
        let mut settings = settings;
        if let Ok(labels) = docker_manager.get_container_labels().await {
            // 모든 설정 소스 병합
            settings.merge_all_config_sources(&labels).await?;
        } else {
            // Docker 라벨을 사용할 수 없는 경우 환경 변수에서만 JSON 설정 로드
            settings.load_json_from_env().await?;
        }
        
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        
        // 초기 라우트 설정
        let initial_routes = docker_manager.get_container_routes().await?;
        {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(initial_routes);
        }
        
        let middleware_manager = MiddlewareManager::new(&settings.middleware, &settings.router_middlewares);
        
        Ok(Self::new(settings, docker_manager, routing_table, middleware_manager))
    }
    
    // HTTP 서버 실행
    #[instrument(skip(self), level = "info", err)]
    pub async fn start_http(&mut self) -> Result<()> {
        // 설정 파일 감시 시작
        if let Err(e) = self.start_config_watcher().await {
            error!("설정 감시자 시작 실패: {}", e);
        }

        // Docker 이벤트 구독
        let mut event_rx = self.docker_manager.subscribe_to_events().await;
        let event_handler = DockerEventHandler::new(
            self.routing_table.clone(),
            Arc::new(RwLock::new(self.middleware_manager.clone())),
        );

        // Docker 이벤트 처리 태스크 시작
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Err(e) = event_handler.handle_event(event).await {
                    error!("이벤트 처리 오류: {}", e);
                }
            }
            warn!("Docker 이벤트 스트림 종료");
        });

        // HTTP 리스너 생성
        let listener = ServerListener::new(&self.config).await?;
        
        // RequestHandler 생성
        let handler = Arc::new(RequestHandler::new(
            self.routing_table.clone(),
            self.middleware_manager.clone(),
        ));

        // 리스너 실행
        listener.run(handler).await
    }
    
    // 설정 감시자 시작
    #[instrument(skip(self), level = "info", err)]
    pub async fn start_config_watcher(&mut self) -> Result<(mpsc::Receiver<()>, tokio::task::JoinHandle<()>)> {
        let watcher_config = WatcherConfig::from_env();
        
        if !watcher_config.enabled {
            return Err(Error::ConfigWatch { 
                message: "설정 감시자가 비활성화되어 있습니다".into(),
                file_path: None,
                source: None,
            });
        }

        // 설정 업데이트 알림을 위한 채널 생성
        let (tx, rx) = mpsc::channel(8);
        
        // 공유 설정 생성
        let shared_config = Arc::new(RwLock::new(self.config.clone()));
        self.shared_config = Some(shared_config.clone());
        
        // 공유 미들웨어 매니저 생성
        let shared_middleware_manager = Arc::new(RwLock::new(self.middleware_manager.clone()));
        self.shared_middleware_manager = Some(shared_middleware_manager.clone());

        // 설정 감시자 초기화
        let watcher = Self::initialize_watcher(&watcher_config).await?;
        self.config_watcher = Some(watcher.clone());
        
        // 감시자 태스크 시작
        let config_path = watcher_config.config_path.clone();
        let task_handle = tokio::spawn(async move {
            let mut watcher = watcher;
            
            info!(path = %config_path.display(), "설정 감시자 시작");
            
            while let Some(event) = watcher.watch().await {
                let events = vec![event];
                if events.is_empty() {
                    continue;
                }
                
                Self::log_config_events(&events);
                
                // 이벤트 분류
                let (file_paths, has_remove_event) = Self::classify_events(events);
                
                if file_paths.is_empty() && !has_remove_event {
                    continue;
                }
                
                // 설정 파일 처리
                let notify_tx = tx.clone();
                
                // TODO: 설정 파일 처리 로직 구현
                // 현재는 임시로 항상 업데이트가 있다고 가정
                let updated = true;
                
                if updated {
                    if let Err(e) = Self::send_config_update_notification(&notify_tx, true).await {
                        error!("설정 업데이트 알림 전송 실패: {}", e);
                    }
                }
            }
        });

        Ok((rx, task_handle))
    }
}

// HTTPS용 구현
impl ServerManager<HttpsEnabled> {
    // HTTPS 서버 생성
    #[instrument(skip(settings), level = "debug", err)]
    pub async fn create_https(settings: Settings<HttpsEnabled>) -> Result<Self> {
        let docker_manager = DockerManager::with_defaults(settings.docker.clone()).await?;
        
        // 초기 상태 체크 설정
        if let Err(e) = docker_manager.setup_initial_health_checks().await {
            error!(error = %e, "초기 상태 체크 설정 실패");
        }
        
        // 설정 소스 병합 (환경 변수, JSON, Docker 라벨)
        let mut settings = settings;
        if let Ok(labels) = docker_manager.get_container_labels().await {
            // 모든 설정 소스 병합
            settings.merge_all_config_sources(&labels).await?;
        } else {
            // Docker 라벨을 사용할 수 없는 경우 환경 변수에서만 JSON 설정 로드
            settings.load_json_from_env().await?;
        }
        
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        
        // 초기 라우트 설정
        let initial_routes = docker_manager.get_container_routes().await?;
        {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(initial_routes);
        }
        
        let middleware_manager = MiddlewareManager::new(&settings.middleware, &settings.router_middlewares);
        
        Ok(Self::new(settings, docker_manager, routing_table, middleware_manager))
    }
    
    // HTTPS 서버 실행
    #[instrument(skip(self), level = "info", err)]
    pub async fn start_https(&mut self) -> Result<()> {
        // 설정 파일 감시 시작
        if let Err(e) = self.start_config_watcher().await {
            error!("설정 감시자 시작 실패: {}", e);
        }

        // Docker 이벤트 구독
        let mut event_rx = self.docker_manager.subscribe_to_events().await;
        let event_handler = DockerEventHandler::new(
            self.routing_table.clone(),
            Arc::new(RwLock::new(self.middleware_manager.clone())),
        );

        // Docker 이벤트 처리 태스크 시작
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Err(e) = event_handler.handle_event(event).await {
                    error!("이벤트 처리 오류: {}", e);
                }
            }
            warn!("Docker 이벤트 스트림 종료");
        });

        // HTTPS 리스너 생성
        let listener = ServerListener::new(&self.config).await?;
        
        // RequestHandler 생성
        let handler = Arc::new(RequestHandler::new(
            self.routing_table.clone(),
            self.middleware_manager.clone(),
        ));

        // 리스너 실행
        listener.run(handler).await
    }
    
    // 설정 감시자 시작 (HTTP와 동일한 구현)
    #[instrument(skip(self), level = "info", err)]
    pub async fn start_config_watcher(&mut self) -> Result<(mpsc::Receiver<()>, tokio::task::JoinHandle<()>)> {
        let watcher_config = WatcherConfig::from_env();
        
        if !watcher_config.enabled {
            return Err(Error::ConfigWatch { 
                message: "설정 감시자가 비활성화되어 있습니다".into(),
                file_path: None,
                source: None,
            });
        }

        // 설정 업데이트 알림을 위한 채널 생성
        let (tx, rx) = mpsc::channel(8);
        
        // 공유 설정 생성
        let shared_config = Arc::new(RwLock::new(self.config.clone()));
        self.shared_config = Some(shared_config.clone());
        
        // 공유 미들웨어 매니저 생성
        let shared_middleware_manager = Arc::new(RwLock::new(self.middleware_manager.clone()));
        self.shared_middleware_manager = Some(shared_middleware_manager.clone());

        // 설정 감시자 초기화
        let watcher = Self::initialize_watcher(&watcher_config).await?;
        self.config_watcher = Some(watcher.clone());
        
        // 감시자 태스크 시작
        let config_path = watcher_config.config_path.clone();
        let task_handle = tokio::spawn(async move {
            let mut watcher = watcher;
            
            info!(path = %config_path.display(), "설정 감시자 시작");
            
            while let Some(event) = watcher.watch().await {
                let events = vec![event];
                if events.is_empty() {
                    continue;
                }
                
                Self::log_config_events(&events);
                
                // 이벤트 분류
                let (file_paths, has_remove_event) = Self::classify_events(events);
                
                if file_paths.is_empty() && !has_remove_event {
                    continue;
                }
                
                // 설정 파일 처리
                let notify_tx = tx.clone();
                
                // TODO: 설정 파일 처리 로직 구현
                // 현재는 임시로 항상 업데이트가 있다고 가정
                let updated = true;
                
                if updated {
                    if let Err(e) = Self::send_config_update_notification(&notify_tx, true).await {
                        error!("설정 업데이트 알림 전송 실패: {}", e);
                    }
                }
            }
        });

        Ok((rx, task_handle))
    }
}

// 인터페이스 구현 (HTTP)
impl ServerInterface for ServerManager<HttpsDisabled> {
    async fn start(&mut self) -> Result<()> {
        self.start_http().await
    }
    
    async fn start_config_watcher(&mut self) -> Result<(mpsc::Receiver<()>, tokio::task::JoinHandle<()>)> {
        <ServerManager<HttpsDisabled>>::start_config_watcher(self).await
    }
}

// 인터페이스 구현 (HTTPS)
impl ServerInterface for ServerManager<HttpsEnabled> {
    async fn start(&mut self) -> Result<()> {
        self.start_https().await
    }
    
    async fn start_config_watcher(&mut self) -> Result<(mpsc::Receiver<()>, tokio::task::JoinHandle<()>)> {
        <ServerManager<HttpsEnabled>>::start_config_watcher(self).await
    }
}

// 두 가지 타입의 ServerManager를 묶는 열거형
pub enum ServerManagerEnum {
    Http(ServerManager<HttpsDisabled>),
    Https(ServerManager<HttpsEnabled>),
}

// ServerInterface 구현
impl ServerInterface for ServerManagerEnum {
    async fn start(&mut self) -> Result<()> {
        match self {
            Self::Http(manager) => manager.start_http().await,
            Self::Https(manager) => manager.start_https().await,
        }
    }

    async fn start_config_watcher(&mut self) -> Result<(mpsc::Receiver<()>, tokio::task::JoinHandle<()>)> {
        match self {
            Self::Http(manager) => manager.start_config_watcher().await,
            Self::Https(manager) => manager.start_config_watcher().await,
        }
    }
}

impl ServerManager {
    // Either에서 적절한 ServerManager 생성
    #[instrument(skip(either), level = "debug", err)]
    pub async fn from_either(either: Either<Settings<HttpsDisabled>, Settings<HttpsEnabled>>) -> Result<ServerManagerEnum> {
        match either {
           Either::Left(http_settings) => {
                info!("HTTP 모드로 서버 매니저 생성");
                let manager = ServerManager::create_http(http_settings).await?;
                Ok(ServerManagerEnum::Http(manager))
            }
           Either::Right(https_settings) => {
                info!("HTTPS 모드로 서버 매니저 생성");
                let manager = ServerManager::create_https(https_settings).await?;
                Ok(ServerManagerEnum::Https(manager))
            }
        }
    }
}

// 설정 파일 처리
async fn process_config_files<HttpsState>(
    paths: Vec<PathBuf>,
    shared_config: Arc<RwLock<Settings<HttpsState>>>,
    shared_middleware_manager: Arc<RwLock<MiddlewareManager>>
) -> Result<bool> 
where
    HttpsState: Clone + Send + Sync + 'static,
{
    if paths.is_empty() {
        return Ok(false);
    }
    
    info!("설정 파일 처리 중: {} 파일", paths.len());
    
    // 1. 유효한 JSON 설정 파일만 수집
    let config_files = collect_json_files(&paths);
    if config_files.is_empty() {
        debug!("처리할 JSON 설정 파일이 없습니다");
        return Ok(false);
    }
    
    // 2. 설정 파일 처리 및 업데이트
    let updated = process_json_configs(config_files, &shared_config).await?;
    
    // 3. 미들웨어 매니저 업데이트 (필요한 경우)
    if updated {
        update_middleware_manager(&shared_config, &shared_middleware_manager).await?;
    }
    
    Ok(updated)
}

// JSON 파일만 수집
fn collect_json_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths.iter()
        .filter(|path| {
            path.extension()
                .map_or(false, |ext| ext == "json")
        })
        .cloned()
        .collect()
}

// JSON 설정 파일 처리
async fn process_json_configs<HttpsState>(
    config_files: Vec<PathBuf>,
    shared_config: &Arc<RwLock<Settings<HttpsState>>>
) -> Result<bool> 
where
    HttpsState: Clone + Send + Sync + 'static,
{
    let mut config_updated = false;
    
    for path in config_files {
        info!(path = %path.display(), "JSON 설정 파일 로드 중");
        
        // 설정 파일 로드 및 검증
        let json_config = match load_and_validate_json_config(&path).await {
            Ok(config) => config,
            Err(e) => {
                error!(error = %e, path = %path.display(), "JSON 설정 파일 로드 실패");
                continue;
            }
        };
        
        // 설정 ID 추출
        let config_id = json_config.get_id(&path);
        
        // 단일 설정 파일 처리
        if process_single_config(json_config, config_id, shared_config).await? {
            config_updated = true;
        }
    }
    
    Ok(config_updated)
}

// 단일 설정 파일 처리
async fn process_single_config<HttpsState>(
    json_config: crate::settings::JsonConfig,
    config_id: String,
    shared_config: &Arc<RwLock<Settings<HttpsState>>>
) -> Result<bool> 
where
    HttpsState: Clone + Send + Sync + 'static,
{
    let mut config_lock = shared_config.write().await;
    
    // 설정 백업
    let config_backup = config_lock.clone();
    
    // 미들웨어 설정 업데이트
    let middleware_updated = update_middleware_settings(&mut config_lock, &json_config, &config_id);
    
    // 라우터-미들웨어 매핑 업데이트
    let router_updated = update_router_middleware_mappings(&mut config_lock, &json_config, &config_id);
    
    // 설정 유효성 검증
    if middleware_updated || router_updated {
        if !validate_middleware_manager(&mut config_lock, &config_backup, middleware_updated) {
            // 유효성 검증 실패 시 설정 복원
            *config_lock = config_backup;
            error!(config_id = %config_id, "미들웨어 설정 유효성 검증 실패");
            return Ok(false);
        }
        
        info!(
            config_id = %config_id, 
            middleware_updated = %middleware_updated,
            router_updated = %router_updated,
            "설정 업데이트 성공"
        );
        return Ok(true);
    } else {
        debug!(config_id = %config_id, "설정 변경 없음");
        return Ok(false);
    }
}

// 설정 파일 로드 및 유효성 검증
async fn load_and_validate_json_config(path: &Path) -> Result<crate::settings::JsonConfig> {
    use crate::settings::JsonConfig;
    
    // JSON 설정 파일 로드
    let mut json_config = JsonConfig::from_file(path)
        .map_err(|e| Error::Config { 
            message: format!("JSON 설정 파일 로드 실패: {}", e),
            file_path: Some(path.to_path_buf()),
            source: Some(Box::new(e)),
        })?;
    
    // 설정 유효성 검증
    json_config.validate()
        .map_err(|e| Error::Config { 
            message: format!("JSON 설정 유효성 검증 실패: {}", e),
            file_path: Some(path.to_path_buf()),
            source: Some(Box::new(e)),
        })?;
    
    Ok(json_config)
}

// 미들웨어 설정 업데이트
fn update_middleware_settings<HttpsState>(
    config_lock: &mut Settings<HttpsState>,
    json_config: &crate::settings::JsonConfig,
    config_id: &str
) -> bool {
    let mut updated = false;
    
    // 미들웨어 설정 추출
    for (name, middleware_config) in &json_config.middlewares {
        let qualified_name = format!("{}_{}", config_id, name);
        debug!(name = %qualified_name, "미들웨어 설정 업데이트");
        
        // 설정에 미들웨어 추가 (기존 설정 유지)
        if let Err(e) = config_lock.add_middleware(qualified_name, middleware_config.clone()) {
            warn!(error = %e, name = %name, "미들웨어 추가 실패");
            continue;
        }
        
        updated = true;
    }
    
    updated
}

// 라우터-미들웨어 매핑 업데이트
fn update_router_middleware_mappings<HttpsState>(
    config_lock: &mut Settings<HttpsState>,
    json_config: &crate::settings::JsonConfig,
    config_id: &str
) -> bool {
    use crate::settings::types::ValidMiddlewareId;
    let mut updated = false;
    
    // 라우터 설정 추출
    for (router_name, router_config) in &json_config.routers {
        if let Some(middleware_names) = &router_config.middlewares {
            let qualified_router = format!("{}_{}", config_id, router_name);
            
            // 미들웨어 ID를 적격한 이름으로 변환
            let qualified_middlewares: Vec<ValidMiddlewareId> = middleware_names
                .iter()
                .filter_map(|name| {
                    let qualified_name = format!("{}_{}", config_id, name);
                    ValidMiddlewareId::new(qualified_name).or_else(|| {
                        warn!(name = %name, "미들웨어 ID 유효성 검증 실패");
                        None
                    })
                })
                .collect();
            
            if !qualified_middlewares.is_empty() {
                // 라우터-미들웨어 매핑 업데이트
                config_lock.router_middlewares.insert(qualified_router, qualified_middlewares);
                updated = true;
            }
        }
    }
    
    updated
}

// 미들웨어 매니저 업데이트
async fn update_middleware_manager<HttpsState>(
    shared_config: &Arc<RwLock<Settings<HttpsState>>>,
    shared_middleware_manager: &Arc<RwLock<MiddlewareManager>>
) -> Result<()>
where
    HttpsState: Clone + Send + Sync + 'static,
{
    // 설정에서 미들웨어 목록 가져오기
    let config = shared_config.read().await;
    let middleware_configs: Vec<(String, MiddlewareConfig)> = config.middleware
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    
    // 미들웨어 매니저 업데이트 - 기존 체인 유지하면서 설정만 업데이트
    if !middleware_configs.is_empty() {
        let mut manager = shared_middleware_manager.write().await;
        manager.update_configs(&middleware_configs);
        debug!("미들웨어 매니저 설정 업데이트 완료: {} 항목", middleware_configs.len());
    }
    
    Ok(())
}

// 미들웨어 매니저 유효성 검증
fn validate_middleware_manager<HttpsState>(
    config_lock: &mut Settings<HttpsState>,
    config_backup: &Settings<HttpsState>,
    config_updated: bool
) -> bool {
    // 설정 유효성 검증
    if config_updated {
        // validate_middlewares 메서드 대신 모든 미들웨어 ID가 유효한지 확인
        for (_, middlewares) in &config_lock.router_middlewares {
            for middleware_id in middlewares {
                if !config_lock.middleware.contains_key(&middleware_id.to_string()) {
                    error!(id = %middleware_id, "미들웨어 ID가 존재하지 않습니다");
                    return false;
                }
            }
        }
    }
    
    true
}
