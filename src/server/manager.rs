use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, warn, info, debug};
use crate::{
    docker::DockerManager, middleware::MiddlewareManager, routing_v2::RoutingTable, settings::{watcher::{ConfigEvent, ConfigWatcher}, JsonConfig, Settings}
};
use super::{
    handler::RequestHandler,
    listener::ServerListener,
    docker::DockerEventHandler,
    Result,
    error::Error,
};
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

/// 설정 파일 감시 설정
struct WatcherConfig {
    enabled: bool,
    debounce_timeout: Duration,
    poll_interval: Duration,
    config_path: PathBuf,
}

pub struct ServerManager {
    pub config: Settings,
    pub docker_manager: DockerManager,
    pub routing_table: Arc<RwLock<RoutingTable>>,
    middleware_manager: MiddlewareManager,
    config_watcher: Option<ConfigWatcher>,
    shared_config: Option<Arc<RwLock<Settings>>>,
    shared_middleware_manager: Option<Arc<RwLock<MiddlewareManager>>>,
}

impl ServerManager {
    // 순수 함수: 모든 의존성을 매개변수로 받음
    pub fn new(
        config: Settings,
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

    // 실제 애플리케이션에서 사용할 팩토리 메서드
    pub async fn with_defaults(mut settings: Settings) -> Result<Self> {
        // 1. Docker 매니저 초기화
        let docker_manager = DockerManager::with_defaults(settings.docker.clone())
            .await
            .map_err(|e| {
                error!(error = %e, "Docker 매니저 초기화 실패");
                e
            })?;

        // 2. 초기 헬스체크 설정
        if let Err(e) = docker_manager.setup_initial_health_checks().await {
            error!(error = %e, "초기 헬스체크 설정 실패");
        }

        // 3. 설정 소스 병합 (환경변수, JSON, Docker 라벨)
        if let Ok(labels) = docker_manager.get_container_labels().await {
            // 모든 설정 소스를 병합
            settings.merge_all_config_sources(&labels).await?;
        } else {
            // Docker 라벨이 없는 경우, 환경변수에서 JSON 설정만 로드
            settings.load_json_from_env().await?;
        }

        // 4. 라우팅 테이블 초기화
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        
        // 5. 초기 라우트 설정
        let initial_routes = docker_manager.get_container_routes().await?;
        
        {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(initial_routes);
        }

        // 6. 미들웨어 매니저 초기화
        let middleware_manager = MiddlewareManager::new(&settings.middleware, &settings.router_middlewares);

        Ok(Self::new(
            settings,
            docker_manager,
            routing_table,
            middleware_manager,
        ))
    }

    /// 환경 변수에서 설정 파일 감시 설정 가져오기
    fn get_watcher_config_from_env() -> WatcherConfig {
        // 감시 기능 활성화 여부 확인
        let enabled = env::var("PROXY_CONFIG_WATCH_ENABLED")
            .map(|val| val.to_lowercase() != "false")
            .unwrap_or(true);
        
        // 디바운싱 타임아웃 설정
        let debounce_timeout_ms = env::var("PROXY_CONFIG_WATCH_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300);
        
        // 폴링 간격 설정
        let poll_interval_ms = env::var("PROXY_CONFIG_WATCH_INTERVAL")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(200);
        
        // 설정 파일 경로
        let config_path = env::var("PROXY_JSON_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let mut path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                path.push("config");
                path.push("config.json");
                path
            });
        
        WatcherConfig {
            enabled,
            debounce_timeout: Duration::from_millis(debounce_timeout_ms),
            poll_interval: Duration::from_millis(poll_interval_ms),
            config_path,
        }
    }

    /// 파일 감시자 초기화
    async fn initialize_watcher(config: &WatcherConfig) -> Result<ConfigWatcher> {
        let mut watcher = ConfigWatcher::new();
        watcher.add_path(&config.config_path);
        watcher.start_with_interval(config.poll_interval).await
            .map_err(|e| Error::ConfigWatchError(format!("파일 감시 시작 실패: {}", e)))?;
        Ok(watcher)
    }

    /// 이벤트 로깅
    fn log_config_events(events: &[ConfigEvent]) {
        info!("설정 파일 이벤트 감지: {} 개의 이벤트", events.len());
        
        for event in events {
            match event {
                ConfigEvent::Created(path) => info!("설정 파일 생성됨: {}", path.display()),
                ConfigEvent::Modified(path) => info!("설정 파일 수정됨: {}", path.display()),
                ConfigEvent::Deleted(path) => warn!("설정 파일 삭제됨: {}", path.display()),
            }
        }
    }

    /// 이벤트 분류 및 처리할 파일 목록 작성
    fn classify_events(events: Vec<ConfigEvent>) -> (Vec<PathBuf>, bool) {
        let mut paths_to_process = Vec::new();
        let mut has_deleted = false;
        
        for event in events {
            match event {
                ConfigEvent::Created(path) | ConfigEvent::Modified(path) => {
                    if !paths_to_process.contains(&path) && path.exists() {
                        paths_to_process.push(path);
                    }
                },
                ConfigEvent::Deleted(_) => {
                    has_deleted = true;
                }
            }
        }
        
        (paths_to_process, has_deleted)
    }

    /// 미들웨어 매니저 업데이트
    async fn update_middleware_manager(
        shared_config: &Arc<RwLock<Settings>>,
        shared_middleware_manager: &Arc<RwLock<MiddlewareManager>>
    ) -> Result<()> {
        let config = shared_config.read().await;
        let mut middleware_lock = shared_middleware_manager.write().await;
        *middleware_lock = MiddlewareManager::new(
            &config.middleware,
            &config.router_middlewares
        );
        
        debug!("미들웨어 매니저 업데이트 완료");
        Ok(())
    }

    /// 단일 설정 파일 처리
    async fn process_config_file(
        path: &Path, 
        shared_config: &Arc<RwLock<Settings>>
    ) -> Result<bool> {
        info!("설정 파일 처리 중: {}", path.display());
        
        // JsonConfig 로드
        let json_config = JsonConfig::from_file(path).await
            .map_err(|e| Error::ConfigError(format!("설정 파일 로드 실패: {}: {}", path.display(), e)))?;
        
        info!("JSON 설정 로드됨: {}", path.display());
        
        // 설정 ID 추출
        let config_id = json_config.get_id(path);
        debug!("설정 ID: {}", config_id);
        
        // 설정 유효성 검증
        if let Err(e) = json_config.validate() {
            return Err(Error::ConfigError(format!("설정 유효성 검증 실패: {}: {}", path.display(), e)));
        }
        
        // 공유 설정 업데이트
        let mut config_updated = false;
        {
            // 설정 백업 (롤백용)
            let config_backup = {
                let config_lock = shared_config.read().await;
                config_lock.clone()
            };
            
            let mut config_lock = shared_config.write().await;
            
            // 미들웨어 설정 업데이트 시도
            let mut _update_success = true;
            
            // 미들웨어 설정 업데이트
            for (name, middleware_config) in json_config.middlewares {
                let full_name = if name.contains('.') {
                    name
                } else {
                    format!("{}.{}", config_id, name)
                };
                
                debug!("미들웨어 업데이트: {}, 설정: {:?}", full_name, middleware_config.settings);
                if let Some(settings) = &middleware_config.settings.get("users") {
                    debug!("미들웨어 users 설정 값: {}", settings);
                }
                
                // 기존 설정 항목 제거 후 새 설정으로 교체
                config_lock.middleware.remove(&full_name);
                config_lock.middleware.insert(full_name, middleware_config);
                config_updated = true;
            }
            
            // 라우터-미들웨어 매핑 업데이트
            for (router_name, router_config) in json_config.routers {
                if let Some(middlewares) = router_config.middlewares {
                    let full_name = if router_name.contains('.') {
                        router_name
                    } else {
                        format!("{}.{}", config_id, router_name)
                    };
                    
                    config_lock.router_middlewares.insert(full_name, middlewares);
                    config_updated = true;
                }
            }
            
            // 미들웨어 매니저 업데이트 시도
            if config_updated {
                // 새 설정으로 미들웨어 매니저 갱신 시도
                let new_middleware_manager = MiddlewareManager::new(
                    &config_lock.middleware,
                    &config_lock.router_middlewares
                );
                
                // 롤백 필요한지 검사 (실제 애플리케이션에서는 미들웨어 초기화 등에서 오류가 발생할 수 있음)
                if let Err(e) = new_middleware_manager.validate() {
                    error!("미들웨어 매니저 업데이트 실패, 롤백 수행: {}", e);
                    
                    // 롤백: 백업에서 설정 복원
                    *config_lock = config_backup;
                    _update_success = false;
                    config_updated = false;
                }
            }
        }
        
        Ok(config_updated)
    }

    /// 여러 설정 파일 처리
    async fn process_config_files(
        paths: Vec<PathBuf>,
        shared_config: Arc<RwLock<Settings>>,
        shared_middleware_manager: Arc<RwLock<MiddlewareManager>>
    ) -> Result<bool> {
        let mut configs_updated = false;
        
        // 모든 변경된 파일에 대해 처리
        for path in paths {
            match Self::process_config_file(&path, &shared_config).await {
                Ok(updated) => {
                    if updated {
                        configs_updated = true;
                    }
                },
                Err(e) => {
                    error!("{}", e.to_string());
                }
            }
        }
        
        // 설정이 업데이트되었으면 미들웨어 매니저도 업데이트
        if configs_updated {
            Self::update_middleware_manager(&shared_config, &shared_middleware_manager).await?;
        }
        
        Ok(configs_updated)
    }

    /// 설정 업데이트 알림 전송
    async fn send_config_update_notification(
        tx: &mpsc::Sender<()>, 
        updated: bool
    ) -> Result<()> {
        if updated {
            // 설정 변경 알림
            debug!("설정 변경 알림 전송 시작");
            tx.send(()).await
                .map_err(|e| Error::ConfigWatchError(format!("설정 변경 알림 전송 실패: {}", e)))?;
            
            debug!("설정 변경 알림 전송 성공");
            info!("설정 리로드 완료");
        } else {
            debug!("유효한 설정 변경이 없어 알림을 전송하지 않습니다.");
        }
        
        Ok(())
    }

    /// 설정 파일 변경 감시 시작
    pub async fn start_config_watcher(&mut self) -> Result<(tokio::sync::mpsc::Receiver<()>, tokio::task::JoinHandle<()>)> {
        // 환경 변수에서 설정 가져오기
        let watcher_config = Self::get_watcher_config_from_env();
        
        if !watcher_config.enabled {
            return Err(Error::ConfigWatchError("설정 파일 감시 기능이 비활성화되었습니다".to_string()));
        }

        // 파일 존재 확인
        if !watcher_config.config_path.exists() {
            return Err(Error::ConfigError(format!("설정 파일을 찾을 수 없습니다: {}", watcher_config.config_path.display())));
        }
        
        info!("설정 파일 감시 시작");
        
        // 파일 감시자 초기화
        let mut watcher = Self::initialize_watcher(&watcher_config).await?;
        
        // 설정 변경 알림 채널
        let (notify_tx, notify_rx) = tokio::sync::mpsc::channel(1);
        
        // 공유 설정 및 미들웨어 매니저 생성
        let shared_config = Arc::new(RwLock::new(self.config.clone()));
        let shared_middleware_manager = Arc::new(RwLock::new(self.middleware_manager.clone()));
        
        // 공유 설정을 ServerManager에 저장
        self.shared_config = Some(shared_config.clone());
        self.shared_middleware_manager = Some(shared_middleware_manager.clone());

        // ConfigWatcher를 클론해서 사용하지 않고, 소유권을 이전함
        self.config_watcher = None;
        
        // 설정 감시 태스크 시작
        let handle = tokio::spawn(async move {
            info!("설정 감시 태스크 시작됨 (디바운싱 타임아웃: {}ms, 폴링 간격: {}ms)", 
                  watcher_config.debounce_timeout.as_millis(), 
                  watcher_config.poll_interval.as_millis());
            
            while let Some(events) = watcher.watch_debounced(watcher_config.debounce_timeout).await {
                // 이벤트 로깅
                ServerManager::log_config_events(&events);
                
                // 이벤트 분류
                let (paths_to_process, has_deleted) = ServerManager::classify_events(events);
                
                if has_deleted {
                    warn!("일부 설정 파일이 삭제되었습니다. 현재 이런 경우 특별한 처리는 하지 않습니다.");
                }
                
                // 설정 파일 처리
                if !paths_to_process.is_empty() {
                    // 설정 파일 처리 - 데이터 흐름 문제 해결
                    let should_notify = match ServerManager::process_config_files(
                        paths_to_process, 
                        shared_config.clone(), 
                        shared_middleware_manager.clone()
                    ).await {
                        Ok(updated) => updated,
                        Err(e) => {
                            error!("설정 파일 처리 실패: {}", e.to_string());
                            false
                        }
                    };
                    
                    // 비동기 호출을 데이터 처리와 분리
                    if should_notify {
                        if let Err(e) = ServerManager::send_config_update_notification(&notify_tx, true).await {
                            error!("알림 전송 실패: {}", e.to_string());
                        }
                    }
                }
            }
            
            info!("설정 감시 태스크 종료");
        });
        
        Ok((notify_rx, handle))
    }

    /// 서버 실행
    pub async fn run(mut self) -> Result<()> {
        // 설정 파일 감시 시작
        if let Err(e) = self.start_config_watcher().await {
            error!("설정 파일 감시 시작 실패: {}", e);
        }

        // Docker 이벤트 구독 설정
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

        // 리스너 생성
        let listener = ServerListener::new(&self.config).await?;
        
        // RequestHandler 생성
        let handler = Arc::new(RequestHandler::new(
            self.routing_table,
            self.middleware_manager,
        ));

        // 리스너 실행
        listener.run(handler).await
    }
} 

