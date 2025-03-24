use bollard::secret::{ContainerSummaryNetworkSettings, EndpointSettings};
use reverse_proxy_traefik::docker::container::ContainerInfo;
use reverse_proxy_traefik::docker::{DockerManager, DockerError, DockerClient, ContainerInfoExtractor};
use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage};
use futures_util::Stream;
use reverse_proxy_traefik::routing_v2::{BackendService, PathMatcher};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use reverse_proxy_traefik::settings::DockerSettings;

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
        let id = container.id.as_ref().ok_or_else(|| DockerError::ContainerConfigError {
            container_id: "unknown".to_string(),
            reason: "컨테이너 ID 없음".to_string(),
            context: None,
        })?;
        
        // 라벨 확인
        let labels = &container.labels;
        
        let host = if let Some(labels) = labels {
            if let Some(host) = labels.get(&format!("{}host", self.label_prefix)) {
                host.clone()
            } else {
                // host 라벨이 없는 경우 router rule에서 추출 시도
                if let Some(rule) = labels.get(&format!("{}http.routers.test.rule", self.label_prefix)) {
                    if rule.contains("Host(`") {
                        let host_start = rule.find("Host(`").unwrap() + 6;
                        let host_end = rule[host_start..].find("`").unwrap() + host_start;
                        rule[host_start..host_end].to_string()
                    } else {
                        return Err(DockerError::ContainerConfigError {
                            container_id: id.clone(),
                            reason: "host label missing and no Host rule found".to_string(),
                            context: None,
                        });
                    }
                } else {
                    return Err(DockerError::ContainerConfigError {
                        container_id: id.clone(),
                        reason: "host label missing".to_string(),
                        context: None,
                    });
                }
            }
        } else {
            return Err(DockerError::ContainerConfigError {
                container_id: id.clone(),
                reason: "no labels found".to_string(),
                context: None,
            });
        };
        
        // IP 주소 추출
        let ip = if let Some(networks) = &container.network_settings {
            if let Some(networks) = &networks.networks {
                if let Some(network) = networks.get(&self.network_name) {
                    if let Some(ip) = &network.ip_address {
                        ip.clone()
                    } else {
                        return Err(DockerError::NetworkError {
                            container_id: id.clone(),
                            network: self.network_name.clone(),
                            reason: "no IP address found".to_string(),
                            context: None,
                        });
                    }
                } else {
                    return Err(DockerError::NetworkError {
                        container_id: id.clone(),
                        network: self.network_name.clone(),
                        reason: "container not connected to network".to_string(),
                        context: None,
                    });
                }
            } else {
                return Err(DockerError::NetworkError {
                    container_id: id.clone(),
                    network: self.network_name.clone(),
                    reason: "container has no networks".to_string(),
                    context: None,
                });
            }
        } else {
            return Err(DockerError::NetworkError {
                container_id: id.clone(),
                network: self.network_name.clone(),
                reason: "container has no network settings".to_string(),
                context: None,
            });
        };
        
        // 경로 매칭 패턴
        let path_matcher = if let Some(labels) = labels {
            if let Some(path) = labels.get(&format!("{}path", self.label_prefix)) {
                let pattern = if let Some(path_type) = labels.get(&format!("{}path.type", self.label_prefix)) {
                    match path_type.as_str() {
                        "regex" => format!("^{}", path),
                        "prefix" => format!("{}*", path),
                        _ => path.clone(),
                    }
                } else {
                    path.clone()
                };
                PathMatcher::from_str(&pattern).ok()
            } else {
                // 기본 경로 매처
                Some(PathMatcher::from_str("/").unwrap())
            }
        } else {
            Some(PathMatcher::from_str("/").unwrap())
        };
        
        // 포트 - 기본값 80
        let port = if let Some(labels) = labels {
            labels.iter()
                .find(|(k, _)| k.contains(".loadbalancer.server.port"))
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(80)
        } else {
            80
        };
        
        // 라우터 이름
        let router_name = if let Some(labels) = labels {
            labels.iter()
                .find(|(k, _)| k.starts_with(&format!("{}http.routers.", self.label_prefix)))
                .map(|(k, _)| {
                    let parts: Vec<&str> = k.split('.').collect();
                    parts.get(3).map(|&name| name.to_string())
                })
                .flatten()
        } else {
            None
        };
        
        // 미들웨어 목록
        let middlewares = if let (Some(r_name), Some(labels)) = (&router_name, labels) {
            let middleware_key = format!("{}http.routers.{}.middlewares", self.label_prefix, r_name);
            labels.get(&middleware_key)
                .map(|v| v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect())
        } else {
            None
        };
        
        // JSON 설정 경로 추출
        let json_config_path = None;
        
        Ok(ContainerInfo {
            host,
            ip,
            port,
            container_id: Some(id.clone()),
            path_matcher,
            middlewares,
            router_name,
            health_check: None,
            load_balancer: None,
            json_config_path,
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

    fn get_label_prefix(&self) -> &str {
        &self.label_prefix
    }
}

// Config::new_for_test() 대신 사용할 함수
fn create_test_settings() -> DockerSettings {
    let mut settings = DockerSettings::default();
    settings.network = "reverse-proxy-network".to_string();
    settings.label_prefix = "reverse-proxy.".to_string();
    settings
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
            labels.insert("reverse-proxy.http.routers.default.rule".to_string(), "Host(`test.localhost`)".to_string());
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
                // 새로운 라벨 형식으로 변경
                labels.insert("reverse-proxy.http.routers.api.rule".to_string(), "Host(`test.localhost`) && PathPrefix(`/api`)".to_string());
                // 서비스 포트 설정
                labels.insert(
                    "reverse-proxy.http.services.api.loadbalancer.server.port".to_string(), 
                    "80".to_string()
                );
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
                // 새로운 라벨 형식으로 변경
                labels.insert("reverse-proxy.http.routers.web.rule".to_string(), "Host(`test.localhost`) && PathPrefix(`/web`)".to_string());
                // 서비스 포트 설정
                labels.insert(
                    "reverse-proxy.http.services.web.loadbalancer.server.port".to_string(), 
                    "80".to_string()
                );
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
    
    // 디버그 정보 출력
    println!("라우트 수: {}", routes.len());
    for ((host, path), service) in &routes {
        println!("라우트: {}:{:?}에 대한 서비스: {:?}", host, path, service);
    }
    
    // 라우트 검증
    assert_eq!(routes.len(), 2, "두 개의 라우트가 있어야 함");
    
    // API 경로 검증 - prefix 매처 사용
    let api_matcher = PathMatcher::prefix("/api");
    let api_route_key = ("test.localhost".to_string(), api_matcher);
    let api_backend = routes.get(&api_route_key);
    assert!(api_backend.is_some(), "API 경로에 대한 백엔드가 존재해야 함");
    
    if let Some(api_backend) = api_backend {
        let addr = api_backend.get_next_address().expect("API 백엔드 주소를 가져올 수 있어야 함");
        assert_eq!(addr.to_string(), "172.17.0.2:80", "API 백엔드 주소가 일치해야 함");
    }
    
    // 웹 경로 검증 - prefix 매처 사용
    let web_matcher = PathMatcher::prefix("/web");
    let web_route_key = ("test.localhost".to_string(), web_matcher);
    let web_backend = routes.get(&web_route_key);
    assert!(web_backend.is_some(), "웹 경로에 대한 백엔드가 존재해야 함");
    
    if let Some(web_backend) = web_backend {
        let addr = web_backend.get_next_address().expect("웹 백엔드 주소를 가져올 수 있어야 함");
        assert_eq!(addr.to_string(), "172.17.0.3:80", "웹 백엔드 주소가 일치해야 함");
    }
}

#[test]
fn test_container_path_patterns() {
    // 컨테이너 설정
    let container = ContainerSummary {
        id: Some("test_container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            // 호스트 라벨 직접 지정
            labels.insert("rproxy.host".to_string(), "example.com".to_string());
            // 포트
            labels.insert("rproxy.http.services.default.loadbalancer.server.port".to_string(), "8080".to_string());
            // 경로 관련 레이블
            labels.insert("rproxy.path".to_string(), "/api".to_string());
            labels.insert("rproxy.path.type".to_string(), "prefix".to_string());
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

    let extractor = MockExtractor::new("test_network".to_string(), "rproxy.".to_string());

    let info = extractor.extract_info(&container).unwrap();
    
    // 기본 정보 확인
    assert_eq!(info.host, "example.com");
    assert_eq!(info.port, 8080);
    assert_eq!(info.ip, "172.17.0.2");

    // 경로 매처 확인 - 디버그 정보 추가
    let path_matcher = info.path_matcher.unwrap();
    println!("경로 매처: {:?}", path_matcher);
    
    // 경로 매칭 테스트 - 더 명확한 검증
    assert!(path_matcher.matches("/api"), "매처는 /api와 일치해야 함");
    
    // 하위 경로 매칭 테스트
    let api_users_path = "/api/users";
    let matches_api_users = path_matcher.matches(api_users_path);
    assert!(matches_api_users, "매처는 {}와 일치해야 함 (현재: {})", api_users_path, matches_api_users);
    
    // 다른 경로는 일치하지 않아야 함
    assert!(!path_matcher.matches("/web"), "매처는 /web과 일치하지 않아야 함");
}

#[test]
fn test_container_path_patterns_regex() {
    // 컨테이너 설정
    let container = ContainerSummary {
        id: Some("test_container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            // 호스트 라벨 직접 지정
            labels.insert("rproxy.host".to_string(), "example.com".to_string());
            // 포트
            labels.insert("rproxy.http.services.default.loadbalancer.server.port".to_string(), "8080".to_string());
            // 정규식 경로 패턴 지정
            labels.insert("rproxy.path".to_string(), "/api/.*".to_string());
            labels.insert("rproxy.path.type".to_string(), "regex".to_string());
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

    let extractor = MockExtractor::new("test_network".to_string(), "rproxy.".to_string());

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
            // 구형 라벨 대신 새로운 형식 사용
            labels.insert("reverse-proxy.http.routers.default.rule".to_string(), "Host(`test.localhost`)".to_string());
            // 미들웨어 라벨 업데이트
            labels.insert("reverse-proxy.http.routers.default.middlewares".to_string(), "auth,compress".to_string());
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
    // 테스트 컨테이너 설정
    let containers = vec![
        ContainerSummary {
            id: Some("web1".to_string()),
            names: Some(vec!["/web1".to_string()]),
            labels: Some({
                let mut labels = HashMap::new();
                labels.insert("reverse-proxy.http.routers.web.rule".to_string(), "Host(`web.example.com`)".to_string());
                labels.insert("reverse-proxy.http.services.web.loadbalancer.server.port".to_string(), "80".to_string());
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
                labels.insert("reverse-proxy.http.routers.web.rule".to_string(), "Host(`web.example.com`)".to_string());
                labels.insert("reverse-proxy.http.services.web.loadbalancer.server.port".to_string(), "80".to_string());
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

    // 라우트 가져오기
    let routes = manager.get_container_routes().await.unwrap();
    
    // 디버그 정보 출력
    println!("라우트 수: {}", routes.len());
    for ((host, path), service) in &routes {
        println!("라우트: {}:{} 서비스: {:?}", host, path, service);
    }
    
    // 라우트 수 검증
    assert_eq!(routes.len(), 1, "하나의 라우트로 그룹화되어야 함");

    // 특정 호스트에 대한 라우트 존재 여부 검증 (HashMap 순서에 의존하지 않음)
    let expected_host = "web.example.com";
    let default_path = PathMatcher::from_str("/").unwrap();
    
    let service = routes.get(&(expected_host.to_string(), default_path));
    assert!(service.is_some(), "web.example.com 호스트에 대한 서비스가 존재해야 함");
    
    if let Some(service) = service {
        // 백엔드 주소 검증 - 두 개의 백엔드가 있어야 함
        let mut backend_addresses = Vec::new();
        
        // 첫 번째 백엔드 주소 가져오기
        if let Ok(addr1) = service.get_next_address() {
            backend_addresses.push(addr1.to_string());
        }
        
        // 두 번째 백엔드 주소 가져오기
        if let Ok(addr2) = service.get_next_address() {
            backend_addresses.push(addr2.to_string());
        }
        
        // 백엔드 수 검증
        assert_eq!(backend_addresses.len(), 2, "두 개의 백엔드가 있어야 함");
        
        // 백엔드 주소 검증 (순서는 중요하지 않음)
        assert!(backend_addresses.contains(&"10.0.0.1:80".to_string()), "10.0.0.1:80 백엔드가 포함되어야 함");
        assert!(backend_addresses.contains(&"10.0.0.2:80".to_string()), "10.0.0.2:80 백엔드가 포함되어야 함");
    }
}
