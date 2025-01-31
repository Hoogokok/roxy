use bollard::Docker;
use bollard::container::ListContainersOptions;
use bollard::models::ContainerSummary;
use std::collections::HashMap;
use std::net::SocketAddr;
use crate::routing::BackendService;

pub struct DockerManager {
    docker: Docker,
}

#[derive(Debug)]
pub struct ContainerInfo {
    host: String,
    address: SocketAddr,
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
        let containers = self.list_containers().await?;
        Ok(self.extract_routes(containers))
    }

    /// 모든 컨테이너 목록을 가져옵니다.
    async fn list_containers(&self) -> Result<Vec<ContainerSummary>, bollard::errors::Error> {
        let mut options = ListContainersOptions::<String>::default();
        options.all = true;
        self.docker.list_containers(Some(options)).await
    }

    /// 컨테이너 목록에서 라우팅 정보를 추출합니다.
    fn extract_routes(&self, containers: Vec<ContainerSummary>) -> HashMap<String, BackendService> {
        containers.into_iter()
            .filter_map(|container| self.container_to_route(container))
            .collect()
    }

    /// 단일 컨테이너에서 라우팅 정보를 추출합니다.
    fn container_to_route(&self, container: ContainerSummary) -> Option<(String, BackendService)> {
        let host = self.extract_host_label(&container)?;
        let address = self.extract_address(&container)?;
        
        Some((
            host,
            BackendService { address }
        ))
    }

    /// 컨테이너에서 호스트 라벨을 추출합니다.
    fn extract_host_label(&self, container: &ContainerSummary) -> Option<String> {
        container.labels.as_ref()?
            .get("reverse-proxy.host")
            .map(|s| s.to_string())
    }

    /// 컨테이너에서 주소 정보를 추출합니다.
    fn extract_address(&self, container: &ContainerSummary) -> Option<SocketAddr> {
        let port_mapping = container.ports.as_ref()?.first()?;
        let ip = port_mapping.ip.as_ref()?.clone();
        let port = port_mapping.public_port?;
        
        format!("{}:{}", ip, port)
            .parse()
            .ok()
    }
} 