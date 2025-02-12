use std::sync::Arc;
use tokio::sync::RwLock;
use crate::{
    config::Config,
    docker::DockerManager,
    routing_v2::RoutingTable,
    middleware::MiddlewareManager,
};
use super::Result;

pub struct ServerManager {
    config: Arc<Config>,
    docker_manager: DockerManager,
    routing_table: Arc<RwLock<RoutingTable>>,
    middleware_manager: MiddlewareManager,
}

impl ServerManager {
    pub async fn new(config: Config) -> Result<Self> {
        // 설정을 Arc로 감싸기
        let config = Arc::new(config);
        
        // Docker 매니저 초기화
        let docker_manager = DockerManager::with_defaults((*config).clone())
            .await
            .map_err(|e| {
                error!(error = %e, "Docker 매니저 초기화 실패");
                e
            })?;

        // 라우팅 테이블 초기화
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        
        // 초기 라우트 설정
        let initial_routes = docker_manager.get_container_routes().await
            .map_err(|e| {
                error!(error = %e, "초기 컨테이너 라우트 획득 실패");
                e
            })?;
        
        {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(initial_routes.clone());
            info!(routes = ?initial_routes, "초기 라우팅 테이블 설정 완료");
        }

        // 미들웨어 매니저 초기화
        let middleware_manager = MiddlewareManager::new(&config);

        Ok(Self {
            config,
            docker_manager,
            routing_table,
            middleware_manager,
        })
    }

    pub async fn run(self) -> Result<()> {
        // TODO: 구현
        unimplemented!()
    }
} 