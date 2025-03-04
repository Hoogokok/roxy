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
use std::path::PathBuf;

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

    /// 설정 파일 변경 감시 시작
    pub async fn start_config_watcher(&mut self) -> Result<(tokio::sync::mpsc::Receiver<()>, tokio::task::JoinHandle<()>)> {
        // 환경 변수를 통해 감시 기능 활성화 여부 확인
        let watch_enabled = match env::var("PROXY_CONFIG_WATCH_ENABLED") {
            Ok(val) => val.to_lowercase() != "false",
            Err(_) => true, // 기본적으로 활성화
        };

        if !watch_enabled {
            return Err(Error::ConfigWatchError("설정 파일 감시 기능이 비활성화되었습니다".to_string()));
        }

        info!("설정 파일 감시 시작");
        
        // 환경변수에서 설정 파일 경로 가져오기
        let config_path = match env::var("PROXY_JSON_CONFIG") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                // 기본 경로 설정
                let mut config_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                config_dir.push("config");
                config_dir.push("config.json");
                config_dir
            }
        };
        
        // 파일 존재 확인
        if !config_path.exists() {
            return Err(Error::ConfigError(format!("설정 파일을 찾을 수 없습니다: {}", config_path.display())));
        }
        
        // 디바운싱 타임아웃 설정 (환경 변수에서 가져옴)
        let debounce_timeout_ms = match env::var("PROXY_CONFIG_WATCH_TIMEOUT") {
            Ok(val) => val.parse::<u64>().unwrap_or(300),
            Err(_) => 300, // 기본값 300ms
        };
        let debounce_timeout = std::time::Duration::from_millis(debounce_timeout_ms);
        
        // 폴링 간격 설정 (환경 변수에서 가져옴)
        let poll_interval_ms = match env::var("PROXY_CONFIG_WATCH_INTERVAL") {
            Ok(val) => val.parse::<u64>().unwrap_or(200),
            Err(_) => 200, // 기본값 200ms
        };
        
        // 파일 감시 설정
        let mut watcher = ConfigWatcher::new();
        watcher.add_path(config_path.clone());
        
        // 환경 변수로 설정된 폴링 간격 적용
        if let Err(e) = watcher.start_with_interval(std::time::Duration::from_millis(poll_interval_ms)).await {
            return Err(Error::ConfigWatchError(format!("파일 감시 시작 실패: {}", e)));
        }
        
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
                  debounce_timeout_ms, poll_interval_ms);
            
            while let Some(events) = watcher.watch_debounced(debounce_timeout).await {
                info!("설정 파일 이벤트 감지: {} 개의 이벤트", events.len());
                
                // 이벤트 타입 로깅
                for event in &events {
                    match event {
                        ConfigEvent::Created(path) => info!("설정 파일 생성됨: {}", path.display()),
                        ConfigEvent::Modified(path) => info!("설정 파일 수정됨: {}", path.display()),
                        ConfigEvent::Deleted(path) => warn!("설정 파일 삭제됨: {}", path.display()),
                    }
                }
                
                // 모든 경로와 파일 상태 추적
                let mut paths_to_process = Vec::new();
                let mut has_deleted = false;
                
                // 이벤트 분류 및 처리할 파일 목록 작성
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
                
                // 삭제된 파일이 있는 경우 처리 (필요한 경우)
                if has_deleted {
                    warn!("일부 설정 파일이 삭제되었습니다. 현재 이런 경우 특별한 처리는 하지 않습니다.");
                }
                
                // 변경된 파일이 있는 경우 처리
                if !paths_to_process.is_empty() {
                    let mut configs_updated = false;
                    
                    // 모든 변경된 파일에 대해 처리
                    for path in paths_to_process {
                        info!("설정 파일 처리 중: {}", path.display());
                        
                        // JsonConfig 로드
                        match JsonConfig::from_file(&path).await {
                            Ok(json_config) => {
                                info!("JSON 설정 로드됨: {}", path.display());
                                
                                // 설정 ID 추출
                                let config_id = json_config.get_id(&path);
                                debug!("설정 ID: {}", config_id);
                                
                                // 설정 유효성 검증
                                if let Err(e) = json_config.validate() {
                                    error!("설정 유효성 검증 실패: {}: {}", path.display(), e);
                                    continue;
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
                                        } else {
                                            configs_updated = true;
                                        }
                                    }
                                }
                                
                                if config_updated && configs_updated {
                                    // 새 설정으로 미들웨어 매니저 갱신
                                    let config = shared_config.read().await;
                                    let mut middleware_lock = shared_middleware_manager.write().await;
                                    *middleware_lock = MiddlewareManager::new(
                                        &config.middleware,
                                        &config.router_middlewares
                                    );
                                    
                                    debug!("미들웨어 매니저 업데이트 완료");
                                }
                            },
                            Err(e) => {
                                error!("설정 파일 로드 실패: {}: {}", path.display(), e);
                            }
                        }
                    }
                    
                    // 설정이 하나라도 업데이트 되었으면 알림 전송
                    if configs_updated {
                        // 설정 변경 알림
                        debug!("설정 변경 알림 전송 시작");
                        match notify_tx.send(()).await {
                            Ok(_) => debug!("설정 변경 알림 전송 성공"),
                            Err(e) => error!("설정 변경 알림 전송 실패: {}", e),
                        }
                        
                        info!("설정 리로드 완료");
                    } else {
                        debug!("유효한 설정 변경이 없어 알림을 전송하지 않습니다.");
                    }
                }
            }
            
            info!("설정 감시 태스크 종료");
        });
        
        Ok((notify_rx, handle))
    }

    /// 설정 변경 완료 대기
    pub async fn wait_for_config_update(&mut self, mut notify_rx: tokio::sync::mpsc::Receiver<()>) -> Result<()> {
        if let Some(_) = notify_rx.recv().await {
            info!("설정 변경 알림 수신됨");
            
            // 공유 설정에서 설정 복사
            let config_clone = {
                let shared_config = match &self.shared_config {
                    Some(config) => config,
                    None => {
                        error!("공유 설정이 없습니다");
                        return Err(Error::ConfigWatchError("공유 설정이 없습니다".to_string()));
                    }
                };
                
                let config_lock = shared_config.read().await;
                debug!("공유 설정 미들웨어 수: {}", config_lock.middleware.len());
                
                // 디버깅용 로그: 미들웨어 설정 상세 정보
                for (key, value) in &config_lock.middleware {
                    debug!("공유 설정 미들웨어: {}", key);
                    
                    // 특정 미들웨어의 경우 더 상세히 로깅
                    if key == "test1.auth" {
                        if let Some(users) = value.settings.get("users") {
                            info!("🔍 공유 설정 test1.auth users 값: {}", users);
                        }
                    }
                }
                
                // 미들웨어 설정 모두 비우고 새로 복사
                self.config.middleware.clear();
                for (key, value) in &config_lock.middleware {
                    self.config.middleware.insert(key.clone(), value.clone());
                }
                
                // 라우터-미들웨어 매핑도 업데이트
                self.config.router_middlewares = config_lock.router_middlewares.clone();
                
                config_lock.clone()
            };
            
            // 설정 업데이트
            self.config = config_clone;
            
            // 업데이트 후 설정 상태 로깅
            debug!("📌 업데이트 후 self.config 미들웨어 수: {}", self.config.middleware.len());
            
            // 테스트를 위한 특정 미들웨어 상세 정보 로깅
            if let Some(auth_middleware) = self.config.middleware.get("test1.auth") {
                if let Some(users) = auth_middleware.settings.get("users") {
                    info!("📌 업데이트 후 test1.auth users 값: {}", users);
                }
            }
            
            info!("설정 업데이트 완료");
            Ok(())
        } else {
            error!("설정 변경 알림 수신 실패");
            Err(Error::ConfigWatchError("설정 변경 알림 수신 실패".to_string()))
        }
    }

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

