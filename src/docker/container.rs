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
    pub middlewares: Option<Vec<String>>,
    pub router_name: Option<String>,
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
        // 1. 먼저 router rule에서 호스트를 찾아봄
        if let Ok(router_rule) = self.find_router_rule(labels) {
            if let Some(host) = self.parse_host_from_rule(router_rule) {
                return Ok(host);
            }
        }
        
        // 2. router rule이 없으면 host 라벨에서 직접 찾음
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

    fn find_router_rule<'a>(&self, labels: &'a Option<std::collections::HashMap<String, String>>) -> Result<&'a String, DockerError> {
        labels
            .as_ref()
            .and_then(|l| l.iter()
                .find(|(k, _)| k.starts_with(&format!("{}http.routers.", self.label_prefix)) && k.ends_with(".rule"))
                .map(|(_, v)| v))
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: "router rule not found".to_string(),
                context: None,
            })
    }

    fn parse_host_from_rule(&self, rule: &str) -> Option<String> {
        let host_pattern = "Host(`";
        rule.find(host_pattern)
            .map(|start| start + host_pattern.len())
            .and_then(|start| {
                rule[start..].find('`')
                    .map(|end| rule[start..start+end].to_string())
            })
    }

    fn extract_path_matcher(&self, labels: &Option<std::collections::HashMap<String, String>>) -> Option<PathMatcher> {
        labels.as_ref()
            .and_then(|l| {
                // 1. 먼저 router rule에서 PathPrefix를 찾아봄
                if let Ok(rule) = self.find_router_rule(labels) {
                    if let Some(path) = self.parse_path_prefix(rule) {
                        return Some(path);
                    }
                }
                
                // 2. router rule에 없으면 path 라벨에서 찾아봄
                let path = l.get(&format!("{}path", self.label_prefix));
                match path {
                    Some(p) => {
                        let pattern = match l.get(&format!("{}path.type", self.label_prefix))
                            .map(String::as_str) 
                        {
                            Some("regex") => format!("^{}", p),
                            Some("prefix") => format!("{}*", p),
                            _ => p.to_string(),
                        };
                        PathMatcher::from_str(&pattern).ok()
                    }
                    None => Some(PathMatcher::from_str("/").unwrap())
                }
            })
            .or_else(|| Some(PathMatcher::from_str("/").unwrap()))
    }

    fn parse_path_prefix(&self, rule: &str) -> Option<PathMatcher> {
        let prefix_pattern = "PathPrefix(`";
        rule.find(prefix_pattern)
            .map(|start| start + prefix_pattern.len())
            .and_then(|start| {
                rule[start..].find('`')
                    .map(|end| &rule[start..start+end])
            })
            .and_then(|path| PathMatcher::from_str(&format!("{}*", path)).ok())
    }

    fn extract_port(&self, labels: &Option<std::collections::HashMap<String, String>>) -> u16 {
        labels
            .as_ref()
            .and_then(|l| l.iter()
                .find(|(k, _)| k.contains(".loadbalancer.server.port"))
                .and_then(|(_, v)| v.parse().ok()))
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

    fn extract_router_name(&self, labels: &Option<std::collections::HashMap<String, String>>) -> Option<String> {
        labels.as_ref()
            .and_then(|l| l.iter()
                .find(|(k, _)| k.starts_with(&format!("{}http.routers.", self.label_prefix)))
                .map(|(k, _)| {
                    let parts: Vec<&str> = k.split('.').collect();
                    parts.get(3).map(|&name| name.to_string())
                })
                .flatten())
    }

    fn extract_middlewares(&self, labels: &Option<std::collections::HashMap<String, String>>, router_name: &str) -> Option<Vec<String>> {
        labels
            .as_ref()
            .and_then(|l| {
                let middleware_key = format!("{}http.routers.{}.middlewares", self.label_prefix, router_name);
                l.get(&middleware_key)
                    .map(|v| v.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect())
            })
    }
    
    pub fn new(network_name: String, label_prefix: String) -> Self {
        
        Self {
            network_name,
            label_prefix,
        }
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
        let router_name = self.extract_router_name(labels);
        let middlewares = router_name
            .as_ref()
            .and_then(|name| self.extract_middlewares(labels, name));
        
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
            middlewares,
            router_name,
        })
    }

    fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, DockerError> {
        let addr = self.parse_socket_addr(&info.ip, info.port)?;
        let mut service = BackendService::with_router(addr, info.router_name.clone());
        if let Some(middlewares) = &info.middlewares {
            service.set_middlewares(middlewares.clone());
        }
        Ok(service)
    }
} 
