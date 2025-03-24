#[cfg(test)]
mod tests {
    use bollard::secret::{ContainerSummaryNetworkSettings, EndpointSettings};
    use crate::docker::ContainerInfo;
    use crate::docker::{DockerManager, DockerError, DockerClient, ContainerInfoExtractor};
    use crate::settings::typestate::Validatable;
    use bollard::container::ListContainersOptions;
    use bollard::models::{ContainerSummary, EventMessage};
    use futures_util::Stream;
    use crate::routing_v2::{BackendService, PathMatcher};
    use std::pin::Pin;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use std::collections::HashMap;
    use crate::settings::DockerSettings;
    use crate::settings::json::{ServiceConfig, LoadBalancerConfig, ServerConfig};
    use crate::settings::types::ValidUrl;
   

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
                    } else if let Some(router_label) = labels.iter().find(|(k, _)| k.contains(".routers.")) {
                        let key = router_label.0;
                        let router_parts: Vec<&str> = key.split('.').collect();
                        if router_parts.len() >= 4 && router_parts[2] == "routers" {
                            let router_name = router_parts[3];
                            
                            if let Some(rule) = labels.get(&format!("{}.http.routers.{}.rule", self.label_prefix, router_name)) {
                                if rule.contains("Host(`") {
                                    let host_start = rule.find("Host(`").unwrap() + 6;
                                    let host_end = rule[host_start..].find("`").unwrap() + host_start;
                                    rule[host_start..host_end].to_string()
                                } else {
                                    return Err(DockerError::ContainerConfigError {
                                        container_id: id.clone(),
                                        reason: format!("No Host rule in router rule: {}", rule),
                                        context: None,
                                    });
                                }
                            } else {
                                return Err(DockerError::ContainerConfigError {
                                    container_id: id.clone(),
                                    reason: format!("No rule found for router {}", router_name),
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
                    // 라우터 규칙에서 PathPrefix 추출 시도
                    let router_name = self.extract_router_name(labels);
                    if let Some(router_name) = &router_name {
                        if let Some(rule) = labels.get(&format!("{}http.routers.{}.rule", self.label_prefix, router_name)) {
                            if rule.contains("PathPrefix(`") {
                                let path_start = rule.find("PathPrefix(`").unwrap() + 12;
                                let path_end = rule[path_start..].find("`").unwrap() + path_start;
                                let path = &rule[path_start..path_end];
                                PathMatcher::from_str(path).ok()
                            } else {
                                // 기본 경로 매처
                                Some(PathMatcher::from_str("/").unwrap())
                            }
                        } else {
                            // 기본 경로 매처
                            Some(PathMatcher::from_str("/").unwrap())
                        }
                    } else {
                        // 기본 경로 매처
                        Some(PathMatcher::from_str("/").unwrap())
                    }
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
                self.extract_router_name(labels)
            } else {
                None
            };
            
            // 미들웨어 목록
            let middlewares = if let Some(labels) = labels {
                // 먼저 router_name.middlewares 형식의 키를 찾습니다
                if let Some(r_name) = &router_name {
                    let middleware_key = format!("{}http.routers.{}.middlewares", self.label_prefix, r_name);
                    if let Some(middleware_str) = labels.get(&middleware_key) {
                        Some(middleware_str.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect())
                    } else {
                        None
                    }
                } else {
                    // 기본(default) 라우터에 대한 미들웨어 확인
                    let default_middleware_key = format!("{}http.routers.default.middlewares", self.label_prefix);
                    labels.get(&default_middleware_key)
                        .map(|v| v.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect())
                }
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

        // 실제 백엔드 서비스 생성 - 이 부분이 중요합니다
        fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, DockerError> {
            use std::net::{IpAddr, Ipv4Addr, SocketAddr};
            use std::str::FromStr;
            
            // IP 주소 파싱
            let ip = match IpAddr::from_str(&info.ip) {
                Ok(ip) => ip,
                Err(_) => {
                    // 호스트 이름 또는 잘못된 IP 형식일 경우 기본값 사용
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
                }
            };
            
            // 소켓 주소 생성
            let addr = SocketAddr::new(ip, info.port);
            
            // 라우터 이름이 있으면 사용
            let mut service = BackendService::with_router(addr, info.router_name.clone());
            
            // 미들웨어 설정
            if let Some(middlewares) = &info.middlewares {
                service.set_middlewares(middlewares.clone());
            }
            
            Ok(service)
        }

        fn get_label_prefix(&self) -> &str {
            &self.label_prefix
        }
        
        fn extract_router_name(&self, labels: &HashMap<String, String>) -> Option<String> {
            labels.iter()
                .find(|(k, _)| k.starts_with(&format!("{}http.routers.", self.label_prefix)))
                .map(|(k, _)| {
                    let parts: Vec<&str> = k.split('.').collect();
                    parts.get(3).map(|&name| name.to_string())
                })
                .flatten()
        }
    }

    fn create_test_settings() -> DockerSettings {
        let mut settings = DockerSettings::default();
        settings.network = "bridge".to_string();
        settings.label_prefix = "reverse-proxy.".to_string();
        settings
    }

    // DockerManager에 with_config_manager 메서드 추가
    impl DockerManager {
        pub async fn with_config_manager(
            client: Box<dyn DockerClient>,
            extractor: Box<dyn ContainerInfoExtractor>,
            config: DockerSettings,
            container_config_manager: Arc<crate::settings::container::ContainerConfigManager>,
        ) -> Self {
            use crate::docker::BackendServiceBuilder;
            
            let service_builder = BackendServiceBuilder::new(
                extractor.clone(),
                Arc::new(client.clone()),
                container_config_manager.clone(),
            );
            
            Self {
                client: Arc::new(client),
                extractor,
                config,
                health_checks: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
                container_config_manager,
                service_builder,
            }
        }
    }

    // 테스트용 컨테이너 정보 생성 함수
    fn test_create_container_info(id: &str, host: &str, ip: &str, port: u16, config_path: Option<&str>) -> ContainerInfo {
        use crate::settings::types::ConfigPath;
        use crate::settings::typestate::Validated;
        
        ContainerInfo {
            host: host.to_string(),
            ip: ip.to_string(),
            port,
            container_id: Some(id.to_string()),
            path_matcher: Some(crate::routing_v2::PathMatcher::from_str("/").unwrap()),
            middlewares: None,
            router_name: Some(format!("{}-router", host)),
            health_check: None,
            load_balancer: None,
            json_config_path: config_path.map(|p| ConfigPath::new(p.to_string()).validate().unwrap()),
        }
    }

    // 테스트용 컨테이너 생성 함수
    fn test_create_container(id: &str, host: &str, ip: &str, port: u16) -> ContainerSummary {
        use bollard::models::{ContainerSummaryNetworkSettings, EndpointSettings};
        
        let mut container = ContainerSummary::default();
        container.id = Some(id.to_string());
        
        // 테스트용 라벨 설정
        let mut labels = HashMap::new();
        labels.insert("reverse-proxy.host".to_string(), host.to_string());
        container.labels = Some(labels);
        
        // 네트워크 설정
        let mut networks = HashMap::new();
        let endpoint = EndpointSettings {
            ip_address: Some(ip.to_string()),
            ..Default::default()
        };
        networks.insert("bridge".to_string(), endpoint);
        
        let network_settings = ContainerSummaryNetworkSettings {
            networks: Some(networks),
            ..Default::default()
        };
        container.network_settings = Some(network_settings);
        
        container
    }

    // 테스트용 DockerManager 생성 함수
    async fn test_create_docker_manager(container_info: ContainerInfo, containers: Vec<ContainerSummary>) -> DockerManager {
        let client = Box::new(MockDockerClient {
            containers: Arc::new(Mutex::new(containers))
        });
        
        let extractor = Box::new(MockExtractor::new(
            "bridge".to_string(),
            "reverse-proxy.".to_string()
        ));
        
        let docker_settings = create_test_settings();
        
        // ContainerConfigManager 생성 및 설정
        let (container_config_manager, _rx) = crate::settings::container::ContainerConfigManager::new();
        let container_config_manager = Arc::new(
            container_config_manager.with_shared_config(crate::settings::Settings::default())
        );
        
        // DockerManager 생성
        DockerManager::with_config_manager(
            client,
            extractor,
            docker_settings,
            container_config_manager
        ).await
    }

    #[tokio::test]
    async fn test_container_routes() {
        let settings = create_test_settings();
        let container_id = "test-container";
        let containers = vec![ContainerSummary {
            id: Some(container_id.to_string()),
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

        // ContainerConfigManager 생성 및 설정
        let (container_config_manager, _rx) = crate::settings::container::ContainerConfigManager::new();
        let container_config_manager = Arc::new(
            container_config_manager.with_shared_config(crate::settings::Settings::default())
        );
        
        // JSON 설정 추가
        let mut json_config = crate::settings::json::JsonConfig::default();
        let mut services = HashMap::new();
        services.insert("default".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                servers: vec![
                    ServerConfig {
                        url: ValidUrl::new("http://172.17.0.2:80").unwrap(),
                        weight: 1
                    }
                ]
            }
        });
        json_config.services = services;
        container_config_manager.container_configs.insert(container_id.to_string(), json_config);
        
        // 외부 설정 매니저로 DockerManager 생성
        let manager = DockerManager::with_config_manager(
            Box::new(client),
            Box::new(extractor),
            settings,
            container_config_manager,
        ).await;

        let routes = manager.get_container_routes().await.unwrap();
        assert_eq!(routes.len(), 1);
        
        // PathMatcher 생성
        let default_matcher = PathMatcher::from_str("/").unwrap();
        let backend = routes.get(&("test.localhost".to_string(), default_matcher)).unwrap();
        
        // 백엔드 주소 검증
        assert_eq!(backend.address.to_string(), "172.17.0.2:80");
    }

    #[tokio::test]
    async fn test_container_with_middleware() {
        let settings = create_test_settings();
        let container_id = "test-container";
        let containers = vec![ContainerSummary {
            id: Some(container_id.to_string()),
            labels: Some({
                let mut labels = HashMap::new();
                // 라우터 규칙은 라우터 경로에 맞게 설정
                labels.insert("reverse-proxy.http.routers.default.rule".to_string(), "Host(`test.localhost`)".to_string());
                // 미들웨어 정의도 동일한 라우터 경로에 설정
                labels.insert("reverse-proxy.http.routers.default.middlewares".to_string(), "auth,compress".to_string());
                // host 라벨도 추가
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

        // ContainerConfigManager 생성 및 설정
        let (container_config_manager, _rx) = crate::settings::container::ContainerConfigManager::new();
        let container_config_manager = Arc::new(
            container_config_manager.with_shared_config(crate::settings::Settings::default())
        );
        
        // JSON 설정 추가
        let mut json_config = crate::settings::json::JsonConfig::default();
        let mut services = HashMap::new();
        services.insert("default".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                servers: vec![
                    ServerConfig {
                        url: ValidUrl::new("http://172.17.0.2:80").unwrap(),
                        weight: 1
                    }
                ]
            }
        });
        json_config.services = services;
        container_config_manager.container_configs.insert(container_id.to_string(), json_config);
        
        // 외부 설정 매니저로 DockerManager 생성
        let manager = DockerManager::with_config_manager(
            Box::new(client),
            Box::new(extractor),
            settings,
            container_config_manager,
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
    async fn test_try_get_container_routes() {
        let settings = create_test_settings();
        
        // 두 개의 컨테이너 준비: 하나는 정상, 하나는 문제 있음(host 라벨 누락)
        let containers = vec![
            // 정상 컨테이너
            ContainerSummary {
                id: Some("valid-container".to_string()),
                labels: Some({
                    let mut labels = HashMap::new();
                    labels.insert("reverse-proxy.host".to_string(), "valid.localhost".to_string());
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
            // 문제 있는 컨테이너 (host 라벨 누락)
            ContainerSummary {
                id: Some("invalid-container".to_string()),
                labels: Some({
                    let labels = HashMap::new();
                    // host 라벨 누락
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

        // ContainerConfigManager 생성 및 설정
        let (container_config_manager, _rx) = crate::settings::container::ContainerConfigManager::new();
        let container_config_manager = Arc::new(
            container_config_manager.with_shared_config(crate::settings::Settings::default())
        );
        
        // JSON 설정 추가 (정상 컨테이너용)
        let mut json_config = crate::settings::json::JsonConfig::default();
        let mut services = HashMap::new();
        services.insert("default".to_string(), ServiceConfig {
            loadbalancer: LoadBalancerConfig {
                servers: vec![
                    ServerConfig {
                        url: ValidUrl::new("http://172.17.0.2:80").unwrap(),
                        weight: 1
                    }
                ]
            }
        });
        json_config.services = services;
        container_config_manager.container_configs.insert("valid-container".to_string(), json_config);
        
        // DockerManager 생성
        let manager = DockerManager::with_config_manager(
            Box::new(client),
            Box::new(extractor),
            settings,
            container_config_manager,
        ).await;

        // try_get_container_routes 호출 - 오류가 있더라도 정상 컨테이너의 결과는 반환해야 함
        let routes = manager.try_get_container_routes().await.unwrap();
        
        // 정상 컨테이너의 라우트만 결과에 포함되어야 함
        assert_eq!(routes.len(), 1);
        
        let default_matcher = PathMatcher::from_str("/").unwrap();
        let backend = routes.get(&("valid.localhost".to_string(), default_matcher)).unwrap();
        
        // 백엔드 주소 검증
        assert_eq!(backend.address.to_string(), "172.17.0.2:80");
    }

    #[tokio::test]
    async fn test_setup_health_check_with_settings() {
        use crate::settings::docker::{HealthCheckType, HealthCheckSettings};
        use crate::settings::core::Settings;
        use crate::settings::typestate::Validated;
        
        // 테스트용 컨테이너 정보 생성
        let container_id = "health-check-test";
        let host = "test.local";
        let ip = "127.0.0.1";
        let port = 8080;
        
        // 컨테이너 정보 생성 (헬스체크 설정 없음)
        let info = test_create_container_info(container_id, host, ip, port, None);
        
        // 병합된 설정에 헬스체크 설정 추가
        let mut settings = Settings::<Validated>::default();
        
        // 설정에서 헬스체크 활성화 및 설정
        settings.docker.health_check.enabled = true;
        
        // HTTP 타입 헬스체크 설정
        settings.docker.health_check.check_type = HealthCheckType::Http {
            path: "/api/health".to_string(),
            method: "GET".to_string(),
            expected_status: 200
        };
        settings.docker.health_check.interval = 10; // 10초
        settings.docker.health_check.timeout = 3;   // 3초
        
        // DockerManager 생성
        let manager = test_create_docker_manager(
            info.clone(),
            vec![test_create_container(container_id, host, ip, port)]
        ).await;
        
        // health_checks가 비어있는지 확인
        {
            let health_checks = manager.health_checks.read().await;
            assert_eq!(health_checks.len(), 0, "초기 health_checks는 비어있어야 함");
        }
        
        // setup_health_check_with_settings 호출
        let result = manager.setup_health_check_with_settings(
            container_id.to_string(), 
            &info, 
            Some(&settings)
        ).await;
        
        // 성공했는지 확인
        assert!(result.is_ok(), "설정 적용에 실패함: {:?}", result.err());
        
        // health_checks에 항목이 추가되었는지 확인
        {
            let health_checks = manager.health_checks.read().await;
            assert_eq!(health_checks.len(), 1, "헬스체크가 추가되어야 함");
            
            // 컨테이너 ID로 헬스체크 항목 확인
            let health_check = health_checks.get(container_id)
                .expect("헬스체크 항목을 찾을 수 없음");
            
            // 값 검증
            assert_eq!(health_check.container_id, container_id);
            assert_eq!(health_check.host, host);
        }
    }

    #[tokio::test]
    async fn test_setup_health_check_with_tcp_settings() {
        use crate::settings::docker::{HealthCheckType, HealthCheckSettings};
        use crate::settings::core::Settings;
        use crate::settings::typestate::Validated;
        
        // 테스트용 컨테이너 정보 생성
        let container_id = "tcp-health-check-test";
        let host = "test.local";
        let ip = "127.0.0.1";
        let port = 8080;
        
        // 컨테이너 정보 생성 (헬스체크 설정 없음)
        let info = test_create_container_info(container_id, host, ip, port, None);
        
        // 병합된 설정에 TCP 헬스체크 설정 추가
        let mut settings = Settings::<Validated>::default();
        
        // 설정에서 헬스체크 활성화 및 설정
        settings.docker.health_check.enabled = true;
        
        // TCP 타입 헬스체크 설정
        settings.docker.health_check.check_type = HealthCheckType::Tcp {
            port: 8085  // 다른 포트로 설정
        };
        settings.docker.health_check.timeout = 2;   // 2초
        
        // DockerManager 생성
        let manager = test_create_docker_manager(
            info.clone(),
            vec![test_create_container(container_id, host, ip, port)]
        ).await;
        
        // 초기 health_checks가 비어있는지 확인
        {
            let health_checks = manager.health_checks.read().await;
            assert_eq!(health_checks.len(), 0, "초기 health_checks는 비어있어야 함");
        }
        
        // setup_health_check_with_settings 호출
        let result = manager.setup_health_check_with_settings(
            container_id.to_string(), 
            &info, 
            Some(&settings)
        ).await;
        
        // 성공했는지 확인
        assert!(result.is_ok(), "설정 적용에 실패함: {:?}", result.err());
        
        // health_checks에 항목이 추가되었는지 확인
        {
            let health_checks = manager.health_checks.read().await;
            assert_eq!(health_checks.len(), 1, "헬스체크가 추가되어야 함");
        }
    }

    #[tokio::test]
    async fn test_setup_health_check_with_existing_info() {
        use crate::docker::container::ContainerHealthCheck;
        use crate::settings::docker::HealthCheckType;
        
        // 테스트용 컨테이너 정보 생성 (이미 헬스체크 설정 있음)
        let container_id = "existing-health-check-test";
        let host = "test.local";
        let ip = "127.0.0.1";
        let port = 8080;
        
        // 기존 헬스체크 설정 있는 컨테이너 정보 생성
        let mut info = test_create_container_info(container_id, host, ip, port, None);
        
        // 기존 헬스체크 설정 추가
        info.health_check = Some(ContainerHealthCheck {
            enabled: true,
            check_type: HealthCheckType::Http {
                path: "/existing/health".to_string(),
                method: "POST".to_string(),
                expected_status: 201
            },
            interval: 20,
            timeout: 4,
        });
        
        // DockerManager 생성
        let manager = test_create_docker_manager(
            info.clone(),
            vec![test_create_container(container_id, host, ip, port)]
        ).await;
        
        // 병합된 설정 없이 호출 (기존 헬스체크 설정 사용)
        let result = manager.setup_health_check_with_settings(
            container_id.to_string(), 
            &info, 
            None
        ).await;
        
        // 성공했는지 확인
        assert!(result.is_ok(), "설정 적용에 실패함: {:?}", result.err());
        
        // health_checks에 항목이 추가되었는지 확인
        {
            let health_checks = manager.health_checks.read().await;
            assert_eq!(health_checks.len(), 1, "헬스체크가 추가되어야 함");
        }
    }
} 