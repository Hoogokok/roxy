use bollard::models::ContainerSummary;
use crate::{docker::DockerError, routing_v2::{BackendService, LoadBalancerStrategy, PathMatcher}};
use std::net::SocketAddr;
use crate::settings::docker::HealthCheckType;
use std::sync::atomic::AtomicUsize;
use tracing::debug;

// 불변 데이터 구조
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub host: String,
    pub ip: String,
    pub port: u16,
    pub path_matcher: Option<PathMatcher>,
    pub middlewares: Option<Vec<String>>,
    pub router_name: Option<String>,
    /// 헬스 체크 설정
    pub health_check: Option<ContainerHealthCheck>,
    pub load_balancer: Option<LoadBalancerStrategy>,
}

#[derive(Debug, Clone)]
pub struct ContainerHealthCheck {
    pub enabled: bool,
    pub check_type: HealthCheckType,
    pub interval: u64,
    pub timeout: u64,
}

// 순수 함수들의 모음
pub trait ContainerInfoExtractor: Send + Sync {
    fn clone_box(&self) -> Box<dyn ContainerInfoExtractor>;
    // 부수 효과가 없는 순수 함수들
    fn extract_info(&self, container: &ContainerSummary) -> Result<ContainerInfo, DockerError>;
    fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, DockerError>;
    
    // 새로운 메서드 추가 (반환 타입 명시)
    fn parse_socket_addr(&self, ip: &str, port: u16) -> Result<SocketAddr, DockerError> {
        let addr: SocketAddr = format!("{}:{}", ip, port)
            .parse::<SocketAddr>()
            .map_err(|e: std::net::AddrParseError| DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: format!("잘못된 소켓 주소: {}:{}", ip, port),
                context: Some(e.to_string()),
            })?;
        Ok(addr)
    }
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

    fn extract_health_check(&self, labels: &Option<std::collections::HashMap<String, String>>) -> Option<ContainerHealthCheck> {
        let labels = labels.as_ref()?;
        
        debug!("헬스체크 설정 추출 시작, 라벨: {:?}", labels);
        
        let enabled = labels.get(&format!("{}health.enabled", self.label_prefix))
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        debug!("헬스체크 활성화 여부: {}", enabled);
        
        if !enabled {
            return None;
        }

        // 체크 타입 결정
        let check_type = if let Some(path) = labels.get(&format!("{}health.http.path", self.label_prefix)) {
            // HTTP 체크
            HealthCheckType::Http {
                path: path.clone(),
                method: labels.get(&format!("{}health.http.method", self.label_prefix))
                    .cloned()
                    .unwrap_or_else(|| "GET".to_string()),
                expected_status: labels.get(&format!("{}health.http.expected_status", self.label_prefix))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(200),
            }
        } else if let Some(port) = labels.get(&format!("{}health.tcp.port", self.label_prefix))
            .and_then(|v| v.parse().ok()) 
        {
            // TCP 체크
            HealthCheckType::Tcp { port }
        } else {
            // 기본값으로 HTTP 체크
            HealthCheckType::Http {
                path: "/health".to_string(),
                method: "GET".to_string(),
                expected_status: 200,
            }
        };

        Some(ContainerHealthCheck {
            enabled,
            check_type,
            interval: labels.get(&format!("{}health.interval", self.label_prefix))
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            timeout: labels.get(&format!("{}health.timeout", self.label_prefix))
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
        })
    }

    // 서비스 이름으로 로드밸런서 활성화 여부 확인
    fn is_load_balancer_enabled(&self, labels: &Option<std::collections::HashMap<String, String>>) -> bool {
        // 서비스 이름이 있으면 자동으로 로드밸런싱 활성화
        labels.as_ref()
            .and_then(|l| l.get(&format!("{}http.services", self.label_prefix)))
            .is_some()
    }

    // 기본적으로 라운드 로빈 전략 사용
    fn extract_load_balancer(&self, _labels: &Option<std::collections::HashMap<String, String>>, _service_name: &str) -> Option<LoadBalancerStrategy> {
        Some(LoadBalancerStrategy::RoundRobin {
            current_index: AtomicUsize::new(0),
        })
    }

    fn extract_info(&self, container: &ContainerSummary) -> Result<ContainerInfo, DockerError> {
        let labels = &container.labels;
        
        // 먼저 로드밸런서 활성화 여부 확인
        let load_balancer_enabled = self.is_load_balancer_enabled(labels);
        
        let host = self.extract_host(labels)?;
        let port = self.extract_port(labels);
        let router_name = self.extract_router_name(labels);
        let middlewares = router_name
            .as_ref()
            .and_then(|name| self.extract_middlewares(labels, name));
        
        let ip = self.extract_container_ip(container)?;

        // 로드밸런서가 활성화된 경우에만 설정 추출
        let load_balancer = if load_balancer_enabled {
            router_name.as_ref()
                .and_then(|name| self.extract_load_balancer(labels, name))
        } else {
            None
        };

        Ok(ContainerInfo {
            host,
            ip,
            port,
            path_matcher: self.extract_path_matcher(labels),
            middlewares,
            router_name,
            health_check: self.extract_health_check(labels),
            load_balancer,
        })
    }

    fn extract_container_ip(&self, container: &ContainerSummary) -> Result<String, DockerError> {
        let networks = container.network_settings
            .as_ref()
            .and_then(|settings| settings.networks.as_ref())
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: container.id.clone().unwrap_or_default(),
                reason: "네트워크 설정을 찾을 수 없음".to_string(),
                context: None,
            })?;

        // 지정된 네트워크의 IP 주소 찾기
        if let Some(network) = networks.get(&self.network_name) {
            if let Some(ip) = &network.ip_address {
                return Ok(ip.clone());
            }
        }

        // 대체 IP 주소 찾기 (첫 번째 사용 가능한 IP)
        for network in networks.values() {
            if let Some(ip) = &network.ip_address {
                return Ok(ip.clone());
            }
        }

        Err(DockerError::ContainerConfigError {
            container_id: container.id.clone().unwrap_or_default(),
            reason: format!("네트워크 {}에서 IP 주소를 찾을 수 없음", self.network_name),
            context: None,
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
        DefaultExtractor::extract_info(self, container)
    }

    fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, DockerError> {
        let addr = self.parse_socket_addr(&info.ip, info.port)?;
        let mut service = BackendService::with_router(addr, info.router_name.clone());
        
        // 미들웨어 설정
        if let Some(middlewares) = &info.middlewares {
            service.set_middlewares(middlewares.clone());
        }

        // 로드밸런서 설정이 있으면 활성화
        if let Some(strategy) = &info.load_balancer {
            service.enable_load_balancer(strategy.clone());
        }

        Ok(service)
    }
} 
