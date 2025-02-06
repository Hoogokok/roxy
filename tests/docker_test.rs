use bollard::secret::{ContainerSummaryNetworkSettings, EndpointSettings};
use reverse_proxy_traefik::docker::container::ContainerInfo;
use reverse_proxy_traefik::docker::{DockerManager, DockerError, DockerClient, ContainerInfoExtractor};
use reverse_proxy_traefik::config::Config;
use reverse_proxy_traefik::routing::BackendService;
use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage};
use futures_util::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::str::FromStr;
use reverse_proxy_traefik::routing::PathMatcher;

// Mock Docker Client
#[derive(Clone)]
struct MockDockerClient {
    containers: Arc<Mutex<Vec<ContainerSummary>>>,
}

#[async_trait::async_trait]
impl DockerClient for MockDockerClient {
    fn clone_box(&self) -> Box<dyn DockerClient> {
        Box::new(self.clone())
    }

    async fn list_containers(
        &self,
        _options: Option<ListContainersOptions<String>>,
    ) -> Result<Vec<ContainerSummary>, DockerError> {
        Ok(self.containers.lock().await.clone())
    }

    fn events(
        &self,
        _options: Option<bollard::system::EventsOptions<String>>,
    ) -> Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>> {
        Box::pin(futures_util::stream::empty())
    }
}

// Mock Container Info Extractor
#[derive(Clone)]
struct MockExtractor {
    network_name: String,
    label_prefix: String,
}

impl MockExtractor {
    fn new(network_name: String, label_prefix: String) -> Self {
        Self {
            network_name,
            label_prefix,
        }
    }
}

impl ContainerInfoExtractor for MockExtractor {
    fn clone_box(&self) -> Box<dyn ContainerInfoExtractor> {
        Box::new(self.clone())
    }

    fn extract_info(&self, container: &ContainerSummary) -> Result<ContainerInfo, DockerError> {
        let labels = container.labels.as_ref();
        
        // 호스트 추출
        let host = labels
            .and_then(|l| l.get(&format!("{}host", self.label_prefix)))
            .map(String::from)
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: container.id.as_deref().unwrap_or("unknown").to_string(),
                reason: "host label missing".to_string(),
                context: None,
            })?;

        // IP 주소 추출
        let ip = container.network_settings.as_ref()
            .and_then(|s| s.networks.as_ref())
            .and_then(|n| n.get(&self.network_name))
            .and_then(|n| n.ip_address.as_ref())
            .ok_or_else(|| DockerError::NetworkError {
                container_id: container.id.as_deref().unwrap_or("unknown").to_string(),
                network: self.network_name.clone(),
                reason: "IP 주소를 찾을 수 없음".to_string(),
                context: None,
            })?;

        // 포트 추출 (기본값 80)
        let port = labels
            .and_then(|l| l.get(&format!("{}port", self.label_prefix)))
            .and_then(|p| p.parse().ok())
            .unwrap_or(80);

        // 경로 패턴 추출
        let path_matcher = labels.and_then(|l| {
            let path = l.get(&format!("{}path", self.label_prefix))?;
            let pattern = match l.get(&format!("{}path.type", self.label_prefix)).map(String::as_str) {
                Some("regex") => format!("^{}", path),
                Some("prefix") => format!("{}*", path),
                _ => path.to_string(),
            };
            PathMatcher::from_str(&pattern).ok()
        });

        Ok(ContainerInfo {
            host,
            ip: ip.clone(),
            port,
            path_matcher,
        })
    }

    fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, 
    DockerError> {
        Ok(BackendService::new(
            format!("{}:{}", info.ip, info.port).parse().unwrap()
        ))
    }
}

#[tokio::test]
async fn test_docker_manager_initialization() {
    let config = Config::new_for_test();
    let client = MockDockerClient {
        containers: Arc::new(Mutex::new(vec![])),
    };
    let extractor = MockExtractor::new("reverse-proxy-network".to_string(), "reverse-proxy.".to_string());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        config,
    ).await;

    assert!(manager.get_container_routes().await.is_ok());
}

#[tokio::test]
async fn test_container_routes() {
    let config = Config::new_for_test();
    let containers = vec![ContainerSummary {
        id: Some("test-container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            labels.insert("reverse-proxy.host".to_string(), "test.localhost".to_string());
            labels
        }),
        network_settings: Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                "reverse-proxy-network".to_string(),
                EndpointSettings {
                    ip_address: Some("172.17.0.2".to_string()),
                    ..Default::default()
                },
            )])),
            ..Default::default()
        }),
        ..Default::default()   }];

    let client = MockDockerClient {
        containers: Arc::new(Mutex::new(containers)),
    };
    let extractor = MockExtractor::new("reverse-proxy-network".to_string(), "reverse-proxy.".to_string());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        config,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    assert_eq!(routes.len(), 1);
    assert!(routes.contains_key(&("test.localhost".to_string(), None)));
}

#[tokio::test]
async fn test_container_routes_with_missing_host() {
    let config = Config::new_for_test();
    let containers = vec![ContainerSummary {
        id: Some("test-container".to_string()),
        labels: Some(HashMap::new()),  // 호스트 라벨 없음
        network_settings: Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                "reverse-proxy-network".to_string(),
                EndpointSettings {
                    ip_address: Some("172.17.0.2".to_string()),
                    ..Default::default()
                },
            )])),
            ..Default::default()
        }),
        ..Default::default()
    }];

    let client = MockDockerClient {
        containers: Arc::new(Mutex::new(containers)),
    };
    let extractor = MockExtractor::new("reverse-proxy-network".to_string(), "reverse-proxy.".to_string());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        config,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    assert_eq!(routes.len(), 0);  // 호스트 라벨이 없으므로 라우트가 없어야 함
}

#[tokio::test]
async fn test_container_routes_with_missing_network() {
    let config = Config::new_for_test();
    let containers = vec![ContainerSummary {
        id: Some("test-container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            labels.insert("reverse-proxy.host".to_string(), "test.localhost".to_string());
            labels
        }),
        network_settings: Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::new()),  // 네트워크 정보 없음
            ..Default::default()
        }),
        ..Default::default()
    }];

    let client = MockDockerClient {
        containers: Arc::new(Mutex::new(containers)),
    };
    let extractor = MockExtractor::new("reverse-proxy-network".to_string(), "reverse-proxy.".to_string());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        config,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    assert_eq!(routes.len(), 0);  // 네트워크 정보가 없으므로 라우트가 없어야 함
}

#[tokio::test]
async fn test_path_based_routing() {
    let config = Config::new_for_test();
    let containers = vec![
        ContainerSummary {
            id: Some("api-container".to_string()),
            labels: Some({
                let mut labels = HashMap::new();
                labels.insert("reverse-proxy.host".to_string(), "test.localhost".to_string());
                labels.insert("reverse-proxy.path".to_string(), "/api".to_string());
                labels
            }),
            network_settings: Some(ContainerSummaryNetworkSettings {
                networks: Some(HashMap::from([(
                    "reverse-proxy-network".to_string(),
                    EndpointSettings {
                        ip_address: Some("172.17.0.2".to_string()),
                        ..Default::default()
                    },
                )])),
                ..Default::default()
            }),
            ..Default::default()
        },
        ContainerSummary {
            id: Some("web-container".to_string()),
            labels: Some({
                let mut labels = HashMap::new();
                labels.insert("reverse-proxy.host".to_string(), "test.localhost".to_string());
                labels.insert("reverse-proxy.path".to_string(), "/web".to_string());
                labels
            }),
            network_settings: Some(ContainerSummaryNetworkSettings {
                networks: Some(HashMap::from([(
                    "reverse-proxy-network".to_string(),
                    EndpointSettings {
                        ip_address: Some("172.17.0.3".to_string()),
                        ..Default::default()
                    },
                )])),
                ..Default::default()
            }),
            ..Default::default()
        },
    ];

    let client = MockDockerClient {
        containers: Arc::new(Mutex::new(containers)),
    };
    let extractor = MockExtractor::new("reverse-proxy-network".to_string(), "reverse-proxy.".to_string());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        config,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    
    // 라우트 검증
    assert_eq!(routes.len(), 2);
    
    // API 경로 검증
    let api_backend = routes.get(&("test.localhost".to_string(), Some("/api".to_string()))).unwrap();
    assert_eq!(api_backend.get_next_address().unwrap().to_string(), "172.17.0.2:80");
    
    // 웹 경로 검증
    let web_backend = routes.get(&("test.localhost".to_string(), Some("/web".to_string()))).unwrap();
    assert_eq!(web_backend.get_next_address().unwrap().to_string(), "172.17.0.3:80");
}

#[test]
fn test_container_path_patterns() {
    let container = ContainerSummary {
        id: Some("test_container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            labels.insert("traefik.host".to_string(), "example.com".to_string());
            labels.insert("traefik.port".to_string(), "8080".to_string());
            labels.insert("traefik.path".to_string(), "/api".to_string());
            labels.insert("traefik.path.type".to_string(), "prefix".to_string());
            labels
        }),
        network_settings: Some(ContainerSummaryNetworkSettings {
            networks: Some({
                let mut networks = HashMap::new();
                networks.insert(
                    "test_network".to_string(),
                    EndpointSettings {
                        ip_address: Some("172.17.0.2".to_string()),
                        ..Default::default()
                    },
                );
                networks
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let extractor = MockExtractor::new("test_network".to_string(), "traefik.".to_string());

    let info = extractor.extract_info(&container).unwrap();
    
    // 기본 정보 확인
    assert_eq!(info.host, "example.com");
    assert_eq!(info.port, 8080);
    assert_eq!(info.ip, "172.17.0.2");

    // 경로 매처 확인
    let path_matcher = info.path_matcher.unwrap();
    assert!(path_matcher.matches("/api"));
    assert!(path_matcher.matches("/api/users"));
    assert!(!path_matcher.matches("/web"));
}

#[test]
fn test_container_path_patterns_regex() {
    let container = ContainerSummary {
        id: Some("test_container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            labels.insert("traefik.host".to_string(), "example.com".to_string());
            labels.insert("traefik.port".to_string(), "8080".to_string());
            labels.insert("traefik.path".to_string(), "/api/.*".to_string());
            labels.insert("traefik.path.type".to_string(), "regex".to_string());
            labels
        }),
        network_settings: Some(ContainerSummaryNetworkSettings {
            networks: Some({
                let mut networks = HashMap::new();
                networks.insert(
                    "test_network".to_string(),
                    EndpointSettings {
                        ip_address: Some("172.17.0.2".to_string()),
                        ..Default::default()
                    },
                );
                networks
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    let extractor = MockExtractor::new("test_network".to_string(), "traefik.".to_string());

    let info = extractor.extract_info(&container).unwrap();
    let path_matcher = info.path_matcher.unwrap();
    
    // 정규식 매칭 확인
    assert!(path_matcher.matches("/api/users"));
    assert!(path_matcher.matches("/api/123"));
    assert!(!path_matcher.matches("/web/api"));
} 