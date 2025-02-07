use bollard::models::ContainerSummary;
use crate::{docker::DockerError, routing_v2::{BackendService, PathMatcher}};
use std::net::SocketAddr;

// 불변 데이터 구조
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub host: String,
    pub ip: String,
    pub port: u16,
    pub path_matcher: Option<PathMatcher>,
}

// 순수 함수들의 모음
pub trait ContainerInfoExtractor: Send + Sync {
    fn clone_box(&self) -> Box<dyn ContainerInfoExtractor>;
    // 부수 효과가 없는 순수 함수들
    fn extract_info(&self, container: &ContainerSummary) -> Result<ContainerInfo, DockerError>;
    fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, DockerError>;
}

impl Clone for Box<dyn ContainerInfoExtractor> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Clone)]
pub struct DefaultExtractor {
    network_name: String,
    label_prefix: String,
}

impl  DefaultExtractor {
    // 순수 함수들
    fn extract_host(&self, labels: &Option<std::collections::HashMap<String, String>>) -> Result<String, DockerError> {
        labels
            .as_ref()
            .and_then(|l| l.get(&format!("{}host", self.label_prefix)))
            .map(String::from)
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: "host label missing".to_string(),
                context: None,
            })
    }

    fn extract_port(&self, labels: &Option<std::collections::HashMap<String, String>>) -> u16 {
        labels
            .as_ref()
            .and_then(|l| l.get(&format!("{}port", self.label_prefix)))
            .and_then(|p| p.parse().ok())
            .unwrap_or(80)
    }

    fn parse_socket_addr(&self, ip: &str, port: u16) -> Result<SocketAddr, DockerError> {
        format!("{}:{}", ip, port)
            .parse()
            .map_err(|_| DockerError::AddressParseError {
                container_id: "unknown".to_string(),
                address: format!("{}:{}", ip, port),
                network: self.network_name.clone(),
                context: None,
            })
    }
    
    pub(crate) fn new(network_name: String, label_prefix: String) -> Self {
        
        Self {
            network_name,
            label_prefix,
        }
    }

    fn extract_path_matcher(&self, labels: &Option<std::collections::HashMap<String, String>>) -> Option<PathMatcher> {
        let labels = labels.as_ref()?;
        let path = labels.get(&format!("{}path", self.label_prefix))?;
        
        // 경로 패턴 타입 확인
        let pattern = match labels.get(&format!("{}path.type", self.label_prefix)).map(String::as_str) {
            Some("regex") => format!("^{}", path),
            Some("prefix") => format!("{}*", path),
            _ => path.to_string(),
        };

        PathMatcher::from_str(&pattern).ok()
    }
}

impl ContainerInfoExtractor for DefaultExtractor {
    fn clone_box(&self) -> Box<dyn ContainerInfoExtractor> {
        Box::new(self.clone())
    }

    fn extract_info(&self, container: &ContainerSummary) -> Result<ContainerInfo, DockerError> {
        let labels = &container.labels;
        let host = self.extract_host(labels)?;
        let port = self.extract_port(labels);
        
        let ip = container
            .network_settings
            .as_ref()
            .and_then(|settings| settings.networks.as_ref())
            .and_then(|networks| networks.get(&self.network_name))
            .and_then(|network| network.ip_address.as_ref())
            .ok_or_else(|| DockerError::NetworkError {
                container_id: container.id.as_deref().unwrap_or("unknown").to_string(),
                network: self.network_name.clone(),
                reason: "IP 주소를 찾을 수 없음".to_string(),
                context: None,
            })?;

        Ok(ContainerInfo {
            host,
            ip: ip.clone(),
            port,
            path_matcher: self.extract_path_matcher(labels),
        })
    }

    fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, DockerError> {
        let addr = self.parse_socket_addr(&info.ip, info.port)?;
        Ok(BackendService::new(addr))
    }
} 