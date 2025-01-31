use bollard::Docker;
use bollard::container::ListContainersOptions;
use std::collections::HashMap;
use std::net::SocketAddr;
use crate::routing::BackendService;

pub struct DockerManager {
    docker: Docker,
}

impl DockerManager {
    /// Docker 클라이언트를 초기화합니다.
    pub async fn new() -> Result<Self, bollard::errors::Error> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(DockerManager { docker })
    }

    /// reverse-proxy.host 라벨이 있는 컨테이너를 찾고 
    /// 호스트-백엔드 서비스 매핑을 반환합니다.
    pub async fn get_container_routes(&self) -> Result<HashMap<String, BackendService>, bollard::errors::Error> {
        let mut options = ListContainersOptions::<String>::default();
        options.all = true;
        
        let containers = self.docker.list_containers(Some(options)).await?;
        let mut routes = HashMap::new();

        for container in containers {
            if let Some(labels) = container.labels {
                if let Some(host) = labels.get("reverse-proxy.host") {
                    if let Some(ports) = container.ports {
                        // 첫 번째 포트 매핑을 사용
                        if let Some(port_mapping) = ports.first() {
                            if let (Some(ip), Some(port)) = (port_mapping.ip.clone(), port_mapping.public_port) {
                                let addr = format!("{}:{}", ip, port).parse::<SocketAddr>()
                                    .expect("Invalid socket address");
                                
                                routes.insert(
                                    host.clone(),
                                    BackendService { address: addr }
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(routes)
    }
} 