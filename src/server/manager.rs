use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, warn};
use crate::{
    settings::Settings,
    docker::DockerManager,
    routing_v2::RoutingTable,
    middleware::MiddlewareManager,
};
use super::{
    handler::RequestHandler,
    listener::ServerListener,
    docker::DockerEventHandler,
    Result,
};

pub struct ServerManager {
    pub config: Settings,
    pub docker_manager: DockerManager,
    pub routing_table: Arc<RwLock<RoutingTable>>,
    middleware_manager: MiddlewareManager,
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

    pub async fn run(self) -> Result<()> {
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