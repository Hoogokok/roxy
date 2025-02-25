use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use crate::{
    docker::{DockerEvent, HealthStatus},
    routing_v2::RoutingTable,
    middleware::MiddlewareManager,
};

pub struct DockerEventHandler {
    routing_table: Arc<RwLock<RoutingTable>>,
    middleware_manager: Arc<RwLock<MiddlewareManager>>,
}

impl DockerEventHandler {
    pub fn new(
        routing_table: Arc<RwLock<RoutingTable>>,
        middleware_manager: Arc<RwLock<MiddlewareManager>>,
    ) -> Self {
        Self { 
            routing_table,
            middleware_manager,
        }
    }

    pub async fn handle_event(&self, event: DockerEvent) -> Result<(), Box<dyn std::error::Error>> {
        let mut table = self.routing_table.write().await;
        
        match event {
            DockerEvent::ContainerStarted { container_id, host, service, path_matcher } => {
                match service.get_next_address() {
                    Ok(addr) => {
                        table.add_route(host.clone(), service, path_matcher.clone());
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
                table.remove_route(&host);
                info!(container_id = %container_id, host = %host, "컨테이너 중지");
            }
            
            DockerEvent::RoutesUpdated(routes) => {
                table.sync_docker_routes(routes);
                info!("라우팅 테이블 업데이트");
            }
            
            DockerEvent::ContainerUpdated { container_id, old_host, new_host, service, path_matcher } => {
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
            
            DockerEvent::MiddlewareConfigsUpdated(configs) => {
                let mut manager = self.middleware_manager.write().await;
                manager.update_configs(&configs);
                manager.print_chain_status();
                info!("미들웨어 설정 업데이트 완료");
            }
            
            DockerEvent::ContainerHealthChanged { container_id, status, message, timestamp } => {
                match status {
                    HealthStatus::Healthy => {
                        info!(
                            container_id = %container_id,
                            status = ?status,
                            message = %message,
                            "컨테이너 헬스 체크 성공"
                        );
                    }
                    HealthStatus::Unhealthy => {
                        warn!(
                            container_id = %container_id,
                            status = ?status,
                            message = %message,
                            "컨테이너 헬스 체크 실패"
                        );
                        // TODO: 나중에 비정상 컨테이너 처리 로직 추가
                    }
                    _ => {
                        info!(
                            container_id = %container_id,
                            status = ?status,
                            message = %message,
                            "컨테이너 헬스 상태 변경"
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