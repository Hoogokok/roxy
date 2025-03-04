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

#[tokio::test]
async fn test_config_reload() {
    // 임시 디렉토리 생성
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("test.json");
    
    println!("테스트 설정 경로: {}", config_path.display());
    
    // 초기 JSON 설정 파일 생성
    let initial_json = r#"{
        "version": "1.0",
        "id": "test1",
        "middlewares": {
            "auth": {
                "middleware_type": "basic-auth",
                "enabled": true,
                "settings": {
                    "users": "admin:password"
                }
            }
        }
    }"#;
    
    {
        let mut file = File::create(&config_path).unwrap();
        file.write_all(initial_json.as_bytes()).unwrap();
    }
    
    // 환경 변수 설정
    let config_path_str = config_path.to_str().unwrap();
    println!("환경 변수 PROXY_JSON_CONFIG 설정: {}", config_path_str);
    std::env::set_var("PROXY_JSON_CONFIG", config_path_str);
    
    // 설정 로드
    let mut settings = Settings::default();
    match settings.load_json_from_env().await {
        Ok(_) => println!("설정 로드 성공"),
        Err(e) => panic!("설정 로드 실패: {}", e),
    }
    
    println!("초기 미들웨어 수: {}", settings.middleware.len());
    println!("미들웨어 키: {:?}", settings.middleware.keys().collect::<Vec<_>>());
    
    let client = MockDockerClient {
        containers: Arc::new(Mutex::new(vec![])),
    };
    let extractor = MockExtractor::new(settings.docker.network.clone(), settings.docker.label_prefix.clone());
    
    let docker_manager = DockerManager::new(
        Box::new(client),
        Box::new(extractor),
        settings.docker.clone(),
    ).await;
    
    let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
    let middleware_manager = MiddlewareManager::new(&settings.middleware, &settings.router_middlewares);
    
    let mut server = ServerManager::new(
        settings,
        docker_manager,
        routing_table,
        middleware_manager,
    );
    
    // 설정 파일 감시 시작
    println!("설정 파일 감시 시작");
    let (notify_rx, handle) = match server.start_config_watcher().await {
        Ok(result) => result,
        Err(e) => panic!("설정 파일 감시 시작 실패: {}", e),
    };
    
    // 잠시 대기
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // 설정 파일 수정
    println!("설정 파일 수정");
    let updated_json = r#"{
        "version": "1.0",
        "id": "test1",
        "middlewares": {
            "auth": {
                "middleware_type": "basic-auth",
                "enabled": true,
                "settings": {
                    "users": "admin:newpassword"
                }
            }
        }
    }"#;
    
    {
        let mut file = File::create(&config_path).unwrap();
        file.write_all(updated_json.as_bytes()).unwrap();
        println!("업데이트된 설정 파일 내용: {}", updated_json);
        file.flush().unwrap(); // 파일 버퍼 플러시 확실히 하기
    }
    
    // 파일 시스템 이벤트 전파 대기
    println!("파일 시스템 이벤트 전파 대기 (1초)");
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // 설정 변경 대기 시작
    println!("설정 변경 대기 시작");
    
    // 타임아웃과 함께 설정 변경 대기 (5초)
    match timeout(Duration::from_secs(5), server.wait_for_config_update(notify_rx)).await {
        Ok(result) => match result {
            Ok(_) => println!("설정 업데이트 성공"),
            Err(e) => panic!("설정 업데이트 실패: {}", e),
        },
        Err(_) => panic!("설정 업데이트 타임아웃"),
    }
    
    // 설정이 올바르게 업데이트되었는지 확인
    println!("설정 확인: 미들웨어 수 = {}", server.config.middleware.len());
    println!("미들웨어 키: {:?}", server.config.middleware.keys().collect::<Vec<_>>());
    
    // 상세 미들웨어 설정 정보 출력
    for (key, value) in &server.config.middleware {
        println!("미들웨어 항목: {}, 설정: {:?}", key, value.settings);
    }
    
    assert!(server.config.middleware.contains_key("test1.auth"));
    
    if let Some(auth_middleware) = server.config.middleware.get("test1.auth") {
        assert_eq!(auth_middleware.settings.get("users").unwrap(), "admin:newpassword");
        println!("미들웨어 설정 확인 성공");
    }
    
    // 테스트 종료 전 감시 태스크 종료
    handle.abort();
    let _ = handle.await;
}