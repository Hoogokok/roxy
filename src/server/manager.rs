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
        // 환경 변수에서 설정 파일 경로 가져오기
        let config_path = match env::var("PROXY_JSON_CONFIG") {
            Ok(path) => PathBuf::from(path),
            Err(_) => {
                debug!("PROXY_JSON_CONFIG 환경 변수가 설정되지 않았습니다");
                return Err(Error::ConfigError("설정 파일 경로가 지정되지 않았습니다".to_string()));
            }
        };
        
        // 파일 존재 확인
        if !config_path.exists() {
            return Err(Error::ConfigError(format!("설정 파일을 찾을 수 없습니다: {}", config_path.display())));
        }
        
        // 파일 감시 설정
        let mut watcher = ConfigWatcher::new();
        watcher.add_path(config_path.clone());
        
        if let Err(e) = watcher.start().await {
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
            debug!("설정 감시 태스크 시작됨");
            
            while let Some(event) = watcher.watch().await {
                debug!("설정 파일 이벤트 감지: {:?}", event);
                
                match event {
                    ConfigEvent::Created(path) | ConfigEvent::Modified(path) => {
                        info!("설정 파일 변경됨: {}", path.display());
                        
                        // 파일 내용 직접 읽어서 확인
                        match tokio::fs::read_to_string(&path).await {
                            Ok(content) => {
                                println!("파일 내용: {}", content);
                            }
                            Err(e) => {
                                println!("파일 읽기 오류: {}", e);
                            }
                        }
                        
                        // JsonConfig 직접 로드
                        match JsonConfig::from_file(&path).await {
                            Ok(json_config) => {
                                debug!("JSON 설정 로드됨");
                                println!("새 JSON 설정 로드됨: {:?}", json_config);
                                
                                // 설정 ID 추출
                                let config_id = json_config.get_id(&path);
                                debug!("설정 ID: {}", config_id);
                                
                                // 공유 설정 업데이트
                                let mut config_lock = shared_config.write().await;
                                
                                // 미들웨어 설정 업데이트
                                for (name, middleware_config) in json_config.middlewares {
                                    let full_name = if name.contains('.') {
                                        name
                                    } else {
                                        format!("{}.{}", config_id, name)
                                    };
                                    
                                    println!("📢 미들웨어 업데이트: {}, 설정: {:?}", full_name, middleware_config.settings);
                                    if let Some(settings) = &middleware_config.settings.get("users") {
                                        println!("📢 미들웨어 users 설정 값: {}", settings);
                                    }
                                    
                                    // 업데이트 전 현재 값 확인 (특히 test1.auth 키를 주시)
                                    if full_name == "test1.auth" {
                                        if let Some(old_config) = config_lock.middleware.get(&full_name) {
                                            if let Some(old_users) = old_config.settings.get("users") {
                                                println!("업데이트 전 test1.auth 값: {}", old_users);
                                            }
                                        }
                                    }
                                    
                                    // 기존 설정 항목 제거 후 새 설정으로 교체
                                    config_lock.middleware.remove(&full_name);
                                    
                                    // 미들웨어 설정 삽입 (full_name을 클론하여 사용)
                                    let key_for_logging = full_name.clone();
                                    println!("미들웨어 설정 삽입: {}", full_name);
                                    config_lock.middleware.insert(full_name, middleware_config);
                                    
                                    // 업데이트 후 새 값 확인 (특히 test1.auth 키)
                                    if key_for_logging == "test1.auth" {
                                        if let Some(new_config) = config_lock.middleware.get(&key_for_logging) {
                                            if let Some(new_users) = new_config.settings.get("users") {
                                                println!("업데이트 후 test1.auth 값: {}", new_users);
                                            }
                                        }
                                    }
                                }
                                
                                debug!("공유 설정 업데이트 후 미들웨어 수: {}", config_lock.middleware.len());
                                for (key, value) in &config_lock.middleware {
                                    debug!("공유 설정 미들웨어: {}, 설정: {:?}", key, value.settings);
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
                                    }
                                }
                                
                                drop(config_lock);
                                
                                // 새 설정으로 미들웨어 매니저 갱신
                                let config = shared_config.read().await;
                                let mut middleware_lock = shared_middleware_manager.write().await;
                                *middleware_lock = MiddlewareManager::new(
                                    &config.middleware,
                                    &config.router_middlewares
                                );
                                drop(middleware_lock);
                                
                                // 설정 변경 알림
                                debug!("설정 변경 알림 전송 시작");
                                match notify_tx.send(()).await {
                                    Ok(_) => debug!("설정 변경 알림 전송 성공"),
                                    Err(e) => error!("설정 변경 알림 전송 실패: {}", e),
                                }
                                
                                info!("설정 리로드 완료");
                            },
                            Err(e) => {
                                error!("설정 파일 로드 실패: {}", e);
                            }
                        }
                    }
                    ConfigEvent::Deleted(path) => {
                        warn!("설정 파일 삭제됨: {}", path.display());
                    }
                }
            }
            
            debug!("설정 감시 태스크 종료");
        });
        
        Ok((notify_rx, handle))
    }

    /// 설정 변경 완료 대기
    pub async fn wait_for_config_update(&mut self, mut notify_rx: tokio::sync::mpsc::Receiver<()>) -> Result<()> {
        if let Some(_) = notify_rx.recv().await {
            debug!("설정 변경 알림 수신됨");
            
            // ========== 파일 변경이 감지되지 않으므로 직접 파일 다시 읽기 ==========
            println!("직접 설정 파일 다시 읽기");
            
            // 환경변수에서 설정 파일 경로 가져오기
            if let Ok(config_path) = env::var("PROXY_JSON_CONFIG") {
                let path = PathBuf::from(&config_path);
                
                // 파일 존재 확인
                if !path.exists() {
                    println!("설정 파일이 존재하지 않습니다: {}", path.display());
                } else {
                    // 파일 내용 확인
                    match tokio::fs::read_to_string(&path).await {
                        Ok(content) => {
                            println!("현재 설정 파일 내용: {}", content);
                            
                            // 파일에서 JSON 설정 다시 로드
                            match JsonConfig::from_file(&path).await {
                                Ok(json_config) => {
                                    println!("설정 파일 다시 로드됨: {:?}", json_config);
                                    
                                    // 미들웨어 설정 직접 업데이트
                                    let config_id = json_config.get_id(&path);
                                    
                                    // 공유 설정 업데이트
                                    if let Some(shared_config) = &self.shared_config {
                                        let mut config_lock = shared_config.write().await;
                                        
                                        // 미들웨어 설정 업데이트
                                        for (name, middleware_config) in json_config.middlewares {
                                            let full_name = if name.contains('.') {
                                                name
                                            } else {
                                                format!("{}.{}", config_id, name)
                                            };
                                            
                                            println!("직접 미들웨어 업데이트: {}, 설정: {:?}", full_name, middleware_config.settings);
                                            if let Some(settings) = &middleware_config.settings.get("users") {
                                                println!("직접 미들웨어 users 설정 값: {}", settings);
                                            }
                                            
                                            // 기존 설정 제거 후 새 설정 삽입
                                            config_lock.middleware.remove(&full_name);
                                            config_lock.middleware.insert(full_name, middleware_config);
                                        }
                                        
                                        // 설정 변경 후 상태 확인
                                        println!("직접 업데이트 후 미들웨어 수: {}", config_lock.middleware.len());
                                        for (key, value) in &config_lock.middleware {
                                            println!("직접 업데이트 후 미들웨어: {}, 설정: {:?}", key, value.settings);
                                        }
                                    } else {
                                        println!("공유 설정이 없어 직접 업데이트할 수 없습니다");
                                    }
                                },
                                Err(e) => println!("설정 파일 로드 실패: {}", e),
                            }
                        },
                        Err(e) => println!("설정 파일 읽기 실패: {}", e),
                    }
                }
            } else {
                println!("PROXY_JSON_CONFIG 환경 변수가 설정되지 않았습니다");
            }
            // ===================================================================
            
            // 현재 설정 상태 로깅
            debug!("현재 self.config 미들웨어 수: {}", self.config.middleware.len());
            for (key, value) in &self.config.middleware {
                debug!("현재 self.config 미들웨어: {}, 설정: {:?}", key, value.settings);
                if key == "test1.auth" {
                    if let Some(users) = value.settings.get("users") {
                        debug!("현재 test1.auth users 값: {}", users);
                    }
                }
            }
            
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
                println!("공유 설정 미들웨어 수: {}", config_lock.middleware.len());
                for (key, value) in &config_lock.middleware {
                    println!("공유 설정 미들웨어: {}, 설정: {:?}", key, value.settings);
                    if key == "test1.auth" {
                        if let Some(users) = value.settings.get("users") {
                            println!("🔍 공유 설정 test1.auth users 값: {}", users);
                        }
                    }
                }
                
                // 여기가 핵심 - 미들웨어 설정 모두 비우고 새로 복사
                self.config.middleware.clear();
                for (key, value) in &config_lock.middleware {
                    self.config.middleware.insert(key.clone(), value.clone());
                }
                
                // 라우터-미들웨어 매핑도 업데이트
                self.config.router_middlewares = config_lock.router_middlewares.clone();
                
                config_lock.clone()
            };
            
            self.config = config_clone;
            
            // 업데이트 후 설정 상태 로깅
            println!("📌 업데이트 후 self.config 미들웨어 수: {}", self.config.middleware.len());
            for (key, value) in &self.config.middleware {
                println!("📌 업데이트 후 self.config 미들웨어: {}, 설정: {:?}", key, value.settings);
                if key == "test1.auth" {
                    if let Some(users) = value.settings.get("users") {
                        println!("📌 업데이트 후 test1.auth users 값: {}", users);
                    }
                }
            }
            
            debug!("설정 업데이트 완료");
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

