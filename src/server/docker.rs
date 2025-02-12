use std::sync::Arc;
use tokio::sync::RwLock;
use crate::{
    routing_v2::{RoutingTable, PathMatcher, BackendService},
    docker::DockerEvent,
};
use tracing::{info, error};

pub struct DockerEventHandler {
    routing_table: Arc<RwLock<RoutingTable>>,
}

impl DockerEventHandler {
    pub fn new(routing_table: Arc<RwLock<RoutingTable>>) -> Self {
        Self { routing_table }
    }

    pub async fn handle_event(&self, event: DockerEvent) -> Result<(), Box<dyn std::error::Error>> {
        match event {
            DockerEvent::ContainerStarted { container_id, host, service, path_matcher } => {
                match service.get_next_address() {
                    Ok(addr) => {
                        self.routing_table.write().await.add_route(
                            host.clone(), 
                            service, 
                            path_matcher.clone()
                        );
                        info!(
                            container_id = %container_id,
                            host = %host,
                            address = ?addr,
                            path_matcher = ?path_matcher,
                            "컨테이너 시작"
                        );
                    }
                    Err(e) => {
                        error!(
                            error = %e,
                            container_id = %container_id,
                            host = %host,
                            "컨테이너 시작 실패: 백엔드 주소 획득 실패"
                        );
                    }
                }
            }
            DockerEvent::ContainerStopped { container_id, host } => {
                self.routing_table.write().await.remove_route(&host);
                info!(container_id = %container_id, host = %host, "컨테이너 중지");
            }
            DockerEvent::RoutesUpdated(routes) => {
                self.routing_table.write().await.sync_docker_routes(routes);
                info!("라우팅 테이블 업데이트");
            }
            DockerEvent::ContainerUpdated { container_id, old_host, new_host, service, path_matcher } => {
                let mut table = self.routing_table.write().await;
                if let Some(old) = old_host {
                    table.remove_route(&old);
                }
                if let Some(host) = new_host {
                    if let Some(svc) = service {
                        table.add_route(host.clone(), svc, path_matcher.clone());
                        info!(
                            container_id = %container_id,
                            host = %host,
                            path_matcher = ?path_matcher,
                            "컨테이너 설정 변경"
                        );
                    }
                }
            }
            DockerEvent::Error(e) => {
                error!(error = %e, "Docker 이벤트 처리 오류");
                return Err(e.into());
            }
        }
        Ok(())
    }
} 