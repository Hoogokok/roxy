use bollard::secret::{ContainerSummaryNetworkSettings, EndpointSettings, NetworkSettings};
use reverse_proxy_traefik::docker::container::ContainerInfo;
use reverse_proxy_traefik::docker::{DockerManager, DockerError, DockerClient, ContainerInfoExtractor};
use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage};
use futures_util::Stream;
use reverse_proxy_traefik::routing_v2::{BackendService, PathMatcher};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use std::collections::HashMap;
use reverse_proxy_traefik::settings::DockerSettings;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;
use tokio::time::{sleep, Duration, timeout};
use reverse_proxy_traefik::settings::Settings;
use reverse_proxy_traefik::server::ServerManager;
use reverse_proxy_traefik::middleware::MiddlewareManager;
use reverse_proxy_traefik::routing_v2::RoutingTable;

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
        
        // 라우터 규칙에서 호스트 추출
        let host = labels
            .and_then(|l| l.get("rproxy.http.routers.web.rule"))
            .and_then(|rule| {
                let rule = rule.trim_start_matches("Host(`").trim_end_matches("`)");
                Some(rule.to_string())
            })
            .ok_or_else(|| DockerError::ContainerConfigError {
                container_id: container.id.as_deref().unwrap_or("unknown").to_string(),
                reason: "host rule missing".to_string(),
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

        // 포트 추출
        let port = labels
            .and_then(|l| l.get("rproxy.http.services.web.loadbalancer.server.port"))
            .and_then(|p| p.parse().ok())
            .unwrap_or(80);

        Ok(ContainerInfo {
            host,
            ip: ip.to_string(),
            port,
            path_matcher: None,
            middlewares: None,
            router_name: Some("web".to_string()),  // 테스트용 고정 라우터 이름
            health_check: None,
            load_balancer: None,
        })
    }

    fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, DockerError> {
        let addr = format!("{}:{}", info.ip, info.port).parse().unwrap();
        let mut service = BackendService::with_router(addr, info.router_name.clone());
        
        if let Some(middlewares) = &info.middlewares {
            service.set_middlewares(middlewares.clone());
        }
        
        Ok(service)
    }
}

// Config::new_for_test() 대신 사용할 함수
fn create_test_settings() -> DockerSettings {
    DockerSettings {
        network: "reverse-proxy-network".to_string(),
        label_prefix: "reverse-proxy.".to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn test_docker_manager_initialization() {
    let settings = create_test_settings();
    let client = MockDockerClient {
        containers: Arc::new(Mutex::new(vec![])),
    };
    let extractor = MockExtractor::new(settings.network.clone(), settings.label_prefix.clone());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        settings,
    ).await;

    assert!(manager.get_container_routes().await.is_ok());
}

#[tokio::test]
async fn test_container_routes() {
    let settings = create_test_settings();
    let containers = vec![ContainerSummary {
        id: Some("test-container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            labels.insert("reverse-proxy.host".to_string(), "test.localhost".to_string());
            labels
        }),
        network_settings: Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                settings.network.clone(),
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
    let extractor = MockExtractor::new(settings.network.clone(), settings.label_prefix.clone());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        settings,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    assert_eq!(routes.len(), 1);
    let default_matcher = PathMatcher::from_str("/").unwrap();
    assert!(routes.contains_key(&("test.localhost".to_string(), default_matcher)));
}

#[tokio::test]
async fn test_container_routes_with_missing_host() {
    let settings = create_test_settings();
    let containers = vec![ContainerSummary {
        id: Some("test-container".to_string()),
        labels: Some(HashMap::new()),  // 호스트 라벨 없음
        network_settings: Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                settings.network.clone(),
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
    let extractor = MockExtractor::new(settings.network.clone(), settings.label_prefix.clone());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        settings,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    assert_eq!(routes.len(), 0);
}

#[tokio::test]
async fn test_container_routes_with_missing_network() {
    let settings = create_test_settings();
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
    let extractor = MockExtractor::new(settings.network.clone(), settings.label_prefix.clone());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        settings,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    assert_eq!(routes.len(), 0);  // 네트워크 정보가 없으므로 라우트가 없어야 함
}

#[tokio::test]
async fn test_path_based_routing() {
    let settings = create_test_settings();
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
                    settings.network.clone(),
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
                    settings.network.clone(),
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
    let extractor = MockExtractor::new(settings.network.clone(), settings.label_prefix.clone());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        settings,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    
    // 라우트 검증
    assert_eq!(routes.len(), 2);
    
    // API 경로 검증
    let api_matcher = PathMatcher::from_str("/api").unwrap();
    let api_backend = routes.get(&("test.localhost".to_string(), api_matcher)).unwrap();
    assert_eq!(api_backend.get_next_address().unwrap().to_string(), "172.17.0.2:80");
    
    // 웹 경로 검증
    let web_matcher = PathMatcher::from_str("/web").unwrap();
    let web_backend = routes.get(&("test.localhost".to_string(), web_matcher)).unwrap();
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

// 미들웨어 테스트 추가
#[tokio::test]
async fn test_container_with_middleware() {
    let settings = create_test_settings();
    let containers = vec![ContainerSummary {
        id: Some("test-container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            labels.insert("reverse-proxy.host".to_string(), "test.localhost".to_string());
            labels.insert("reverse-proxy.middlewares".to_string(), "auth,compress".to_string());
            labels
        }),
        network_settings: Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                settings.network.clone(),
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
    let extractor = MockExtractor::new(settings.network.clone(), settings.label_prefix.clone());

    let manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        settings,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    assert_eq!(routes.len(), 1);
    
    let default_matcher = PathMatcher::from_str("/").unwrap();
    let backend = routes.get(&("test.localhost".to_string(), default_matcher)).unwrap();
    
    // 미들웨어 검증
    assert!(backend.has_middlewares());
    assert_eq!(
        backend.middlewares.as_ref().unwrap(),
        &vec!["auth".to_string(), "compress".to_string()]
    );
}

#[tokio::test]
async fn test_load_balancer_grouping() {
    let containers = vec![
        ContainerSummary {
            id: Some("web1".to_string()),
            names: Some(vec!["/web1".to_string()]),
            labels: Some({
                let mut labels = HashMap::new();
                labels.insert("rproxy.http.routers.web.rule".to_string(), "Host(`web.example.com`)".to_string());
                labels.insert("rproxy.http.services.web.loadbalancer.server.port".to_string(), "80".to_string());
                labels
            }),
            network_settings: Some(ContainerSummaryNetworkSettings {
                networks: Some({
                    let mut networks = HashMap::new();
                    networks.insert("reverse-proxy-network".to_string(), EndpointSettings {
                        ip_address: Some("10.0.0.1".to_string()),
                        ..Default::default()
                    });
                    networks
                }),
            }),
            ..Default::default()
        },
        ContainerSummary {
            id: Some("web2".to_string()),
            names: Some(vec!["/web2".to_string()]),
            labels: Some({
                let mut labels = HashMap::new();
                labels.insert("rproxy.http.routers.web.rule".to_string(), "Host(`web.example.com`)".to_string());
                labels.insert("rproxy.http.services.web.loadbalancer.server.port".to_string(), "80".to_string());
                labels
            }),
            network_settings: Some(ContainerSummaryNetworkSettings {
                networks: Some({
                    let mut networks = HashMap::new();
                    networks.insert("reverse-proxy-network".to_string(), EndpointSettings {
                        ip_address: Some("10.0.0.2".to_string()),
                        ..Default::default()
                    });
                    networks
                }),
            }),
            ..Default::default()
        },
    ];

    let client = MockDockerClient {
        containers: Arc::new(Mutex::new(containers)),
    };

    let settings = create_test_settings();
    let manager = DockerManager::new(
        Box::new(client),
        Box::new(MockExtractor::new(settings.network.clone(), settings.label_prefix.clone())),
        settings,
    ).await;

    let routes = manager.get_container_routes().await.unwrap();
    assert_eq!(routes.len(), 1, "하나의 라우트로 그룹화되어야 함");

    let ((host, _), service) = routes.iter().next().unwrap();
    assert_eq!(host, "web.example.com");
}
