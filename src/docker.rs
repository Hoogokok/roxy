use bollard::Docker;
use bollard::container::ListContainersOptions;
use bollard::models::ContainerSummary;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::fmt;
use crate::routing::BackendService;

#[derive(Debug)]
pub enum DockerError {
    /// Docker 데몬 연결 실패
    ConnectionError(bollard::errors::Error),
    /// 컨테이너 목록 조회 실패
    ListContainersError(bollard::errors::Error),
    /// 컨테이너 설정 오류
    ContainerConfigError {
        container_id: String,
        reason: String,
    },
    /// 주소 파싱 오류
    AddressParseError {
        container_id: String,
        address: String,
    },
}

impl fmt::Display for DockerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DockerError::ConnectionError(e) => 
                write!(f, "Docker 데몬 연결 실패: {}", e),
            DockerError::ListContainersError(e) => 
                write!(f, "컨테이너 목록 조회 실패: {}", e),
            DockerError::ContainerConfigError { container_id, reason } => 
                write!(f, "컨테이너 {} 설정 오류: {}", container_id, reason),
            DockerError::AddressParseError { container_id, address } => 
                write!(f, "컨테이너 {}의 주소 {} 파싱 실패", container_id, address),
        }
    }
}

impl std::error::Error for DockerError {}

impl From<bollard::errors::Error> for DockerError {
    fn from(err: bollard::errors::Error) -> Self {
        DockerError::ConnectionError(err)
    }
}

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
    pub async fn new() -> Result<Self, DockerError> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(DockerManager { docker })
    }

    /// reverse-proxy.host 라벨이 있는 컨테이너를 찾고 
    /// 호스트-백엔드 서비스 매핑을 반환합니다.
    pub async fn get_container_routes(&self) -> Result<HashMap<String, BackendService>, DockerError> {
        let containers = self.list_containers().await?;
        Ok(self.extract_routes(&containers))
    }

    /// 모든 컨테이너 목록을 가져옵니다.
    async fn list_containers(&self) -> Result<Vec<ContainerSummary>, DockerError> {
        let mut options = ListContainersOptions::<String>::default();
        options.all = true;
        self.docker
            .list_containers(Some(options))
            .await
            .map_err(DockerError::ListContainersError)
    }

    /// 컨테이너 목록에서 라우팅 정보를 추출합니다.
    fn extract_routes(&self, containers: &[ContainerSummary]) -> HashMap<String, BackendService> {
        containers.iter()
            .filter_map(|container| self.container_to_route(container).ok())
            .collect()
    }

    /// 단일 컨테이너에서 라우팅 정보를 추출합니다.
    fn container_to_route(&self, container: &ContainerSummary) -> Result<(String, BackendService), DockerError> {
        let container_id = container.id.as_deref().unwrap_or("unknown").to_string();
        
        let host = self.extract_host_label(container, &container_id)?;
        let address = self.extract_address(container, &container_id)?;
        
        Ok((host, BackendService { address }))
    }

    /// 컨테이너에서 호스트 라벨을 추출합니다.
    fn extract_host_label(&self, container: &ContainerSummary, container_id: &str) -> Result<String, DockerError> {
        container
            .labels
            .as_ref()
            .and_then(|labels| labels.get("reverse-proxy.host"))
            .map(|s| s.to_string())
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: container_id.to_string(),
                reason: "reverse-proxy.host 라벨 누락".to_string(),
            })
    }

    /// 컨테이너에서 주소 정보를 추출합니다.
    fn extract_address(&self, container: &ContainerSummary, container_id: &str) -> Result<SocketAddr, DockerError> {
        let ports = container.ports.as_ref().ok_or_else(|| DockerError::ContainerConfigError {
            container_id: container_id.to_string(),
            reason: "포트 설정 누락".to_string(),
        })?;

        let port_mapping = ports.first().ok_or_else(|| DockerError::ContainerConfigError {
            container_id: container_id.to_string(),
            reason: "포트 매핑 누락".to_string(),
        })?;

        let ip = port_mapping.ip.as_ref().ok_or_else(|| DockerError::ContainerConfigError {
            container_id: container_id.to_string(),
            reason: "IP 주소 누락".to_string(),
        })?;

        let port = port_mapping.public_port.ok_or_else(|| DockerError::ContainerConfigError {
            container_id: container_id.to_string(),
            reason: "공개 포트 누락".to_string(),
        })?;

        let addr_str = format!("{}:{}", ip, port);
        addr_str.parse().map_err(|_| DockerError::AddressParseError {
            container_id: container_id.to_string(),
            address: addr_str,
        })
    }
} 