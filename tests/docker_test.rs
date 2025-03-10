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
        let labels = container.labels.as_ref();
        
        // 라우터 규칙에서 호스트와 경로 추출
        let (host, router_name, path_from_rule) = labels
            .and_then(|l| {
                // 여러 라우터 이름 시도 (default, web, api)
                let router_keys = [
                    format!("{}.http.routers.default.rule", self.label_prefix.trim_end_matches('.')),
                    format!("{}.http.routers.web.rule", self.label_prefix.trim_end_matches('.')),
                    format!("{}.http.routers.api.rule", self.label_prefix.trim_end_matches('.')),
                    // 이전 형식도 지원
                    format!("{}.host", self.label_prefix.trim_end_matches('.'))
                ];
                
                for key in router_keys.iter() {
                    if let Some(rule) = l.get(key) {
                        // 복합 규칙 파싱 (예: Host(`example.com`) && PathPrefix(`/api`))
                        let parts: Vec<&str> = rule.split(" && ").collect();
                        let mut host = None;
                        let mut path = None;
                        
                        for part in &parts {
                            if part.starts_with("Host(`") && part.ends_with("`)") {
                                host = Some(part.trim_start_matches("Host(`").trim_end_matches("`)").to_string());
                            } else if part.starts_with("PathPrefix(`") && part.ends_with("`)") {
                                let path_str = part.trim_start_matches("PathPrefix(`").trim_end_matches("`)");
                                // 항상 prefix 매처를 사용하여 일관성 확보
                                path = Some(PathMatcher::prefix(path_str));
                            }
                        }
                        
                        // 단순 호스트 규칙인 경우
                        if parts.len() == 1 && host.is_none() {
                            if rule.starts_with("Host(`") && rule.ends_with("`)") {
                                host = Some(rule.trim_start_matches("Host(`").trim_end_matches("`)").to_string());
                            } else if !key.contains("rule") {
                                // 이전 형식 지원: 직접 호스트 값
                                host = Some(rule.to_string());
                            }
                        }
                        
                        if let Some(host_value) = host {
                            let router_name = if key.contains(".default.") {
                                "default"
                            } else if key.contains(".web.") {
                                "web"
                            } else if key.contains(".api.") {
                                "api"
                            } else {
                                "default"
                            };
                            return Some((host_value, router_name.to_string(), path));
                        }
                    }
                }
                None
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

        // 포트 추출 - 여러 서비스 이름 시도
        let port = labels
            .and_then(|l| {
                let port_keys = [
                    format!("{}.http.services.{}.loadbalancer.server.port", self.label_prefix.trim_end_matches('.'), router_name),
                    format!("{}.http.services.default.loadbalancer.server.port", self.label_prefix.trim_end_matches('.')),
                    // 이전 형식도 지원
                    format!("{}.port", self.label_prefix.trim_end_matches('.'))
                ];
                
                for key in port_keys.iter() {
                    if let Some(port_value) = l.get(key) {
                        if let Ok(port) = port_value.parse::<u16>() {
                            return Some(port);
                        }
                    }
                }
                
                None
            })
            .unwrap_or(80);

        // 경로 매처 결정 - 룰에서 추출된 것 또는 추가 라벨에서 찾기
        let path_matcher = path_from_rule.or_else(|| {
            labels.and_then(|l| {
                // 개별 경로 라벨 시도
                let path_keys = [
                    format!("{}.http.routers.{}.rule.PathPrefix", self.label_prefix.trim_end_matches('.'), router_name),
                    format!("{}.http.routers.{}.rule.Path", self.label_prefix.trim_end_matches('.'), router_name),
                    format!("{}.http.routers.{}.rule.PathRegexp", self.label_prefix.trim_end_matches('.'), router_name),
                    // 이전 형식도 지원
                    format!("{}.path", self.label_prefix.trim_end_matches('.'))
                ];
                
                for key in path_keys.iter() {
                    if let Some(path_value) = l.get(key) {
                        println!("경로 키 발견: {}, 값: {}", key, path_value);
                        
                        // 경로 타입 결정 및 매처 생성
                        if key.contains("PathPrefix") {
                            // 항상 prefix 매처를 사용하여 일관성 확보
                            return Some(PathMatcher::prefix(path_value));
                        } else if key.contains("PathRegexp") {
                            return PathMatcher::from_str(&format!("^{}$", path_value)).ok();
                        } else {
                            let path_type = l.get(&format!("{}.path.type", self.label_prefix.trim_end_matches('.')))
                                .map(|t| t.as_str())
                                .unwrap_or("prefix");
                            
                            return match path_type {
                                "regex" => PathMatcher::from_str(&format!("^{}$", path_value)).ok(),
                                _ => Some(PathMatcher::prefix(path_value))
                            };
                        }
                    }
                }
                None
            })
        });
            
        // 미들웨어 추출
        let middlewares = labels
            .and_then(|l| {
                let middleware_keys = [
                    format!("{}.http.routers.{}.middlewares", self.label_prefix.trim_end_matches('.'), router_name),
                    // 이전 형식도 지원
                    format!("{}.middlewares", self.label_prefix.trim_end_matches('.'))
                ];
                
                for key in middleware_keys.iter() {
                    if let Some(middleware_value) = l.get(key) {
                        let middlewares = middleware_value
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .collect::<Vec<String>>();
                        
                        if !middlewares.is_empty() {
                            return Some(middlewares);
                        }
                    }
                }
                
                None
            });

        Ok(ContainerInfo {
            host,
            ip: ip.to_string(),
            port,
            path_matcher,
            middlewares,
            router_name: Some(router_name),
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
    let container = ContainerSummary {
        id: Some("test_container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            // 새로운 라벨 형식 사용
            labels.insert("rproxy.http.routers.default.rule".to_string(), "Host(`example.com`)".to_string());
            labels.insert("rproxy.http.services.default.loadbalancer.server.port".to_string(), "8080".to_string());
            // 경로 관련 레이블 - PathPrefix 사용 - 명시적으로 경로 지정
            labels.insert("rproxy.http.routers.default.rule.PathPrefix".to_string(), "/api".to_string());
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
    
    // 하위 경로 매칭 테스트 - 실패하는 부분
    let api_users_path = "/api/users";
    let matches_api_users = path_matcher.matches(api_users_path);
    assert!(matches_api_users, "매처는 {}와 일치해야 함 (현재: {})", api_users_path, matches_api_users);
    
    // 다른 경로는 일치하지 않아야 함
    assert!(!path_matcher.matches("/web"), "매처는 /web과 일치하지 않아야 함");
}

#[test]
fn test_container_path_patterns_regex() {
    let container = ContainerSummary {
        id: Some("test_container".to_string()),
        labels: Some({
            let mut labels = HashMap::new();
            labels.insert("rproxy.http.routers.default.rule".to_string(), "Host(`example.com`)".to_string());
            labels.insert("rproxy.http.services.default.loadbalancer.server.port".to_string(), "8080".to_string());
            labels.insert("rproxy.http.routers.default.rule.PathRegexp".to_string(), "/api/.*".to_string());
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
