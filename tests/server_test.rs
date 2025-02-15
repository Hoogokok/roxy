use reverse_proxy_traefik::{
    settings::Settings,
    server::ServerManager,
    docker::{DockerClient, DockerError, DockerManager, container::DefaultExtractor},
    routing_v2::RoutingTable,
    middleware::MiddlewareManager,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;
use bollard::container::ListContainersOptions;
use bollard::models::{ContainerSummary, EventMessage};
use bollard::system::EventsOptions;
use futures_util::stream::{self, Stream};
use std::pin::Pin;
use reverse_proxy_traefik::routing_v2::PathMatcher;
use serial_test::serial;
use std::collections::HashMap;
use std::sync::Mutex;
use bollard::secret::{ContainerSummaryNetworkSettings, EndpointSettings};

// Mock Docker 클라이언트 구현
#[derive(Clone)]
struct MockDockerClient {
    containers: Arc<Mutex<Vec<ContainerSummary>>>,
}

impl MockDockerClient {
    fn empty() -> Self {
        Self {
            containers: Arc::new(Mutex::new(vec![])),
        }
    }
}

#[async_trait]
impl DockerClient for MockDockerClient {
    fn clone_box(&self) -> Box<dyn DockerClient> {
        Box::new(self.clone())
    }

    async fn list_containers(&self, _options: Option<ListContainersOptions<String>>) 
        -> Result<Vec<ContainerSummary>, DockerError> 
    {
        Ok((*self.containers.lock().unwrap()).clone())
    }

    fn events(&self, _options: Option<EventsOptions<String>>) 
        -> Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>> 
    {
        Box::pin(stream::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleanup_env() {
        // 모든 환경 변수를 명시적으로 제거
        let vars = [
            "PROXY_HTTP_PORT",
            "PROXY_HTTPS_ENABLED",
            "PROXY_HTTPS_PORT",
            "PROXY_TLS_CERT",
            "PROXY_TLS_KEY",
            "PROXY_LOG_LEVEL",
            "PROXY_DOCKER_NETWORK",
            "PROXY_LABEL_PREFIX",
        ];

        for var in vars.iter() {
            std::env::remove_var(var);
        }

        // 환경 변수가 제대로 제거되었는지 확인
        for var in vars.iter() {
            assert!(std::env::var(var).is_err(), "Environment variable {} should be removed", var);
        }
    }

    async fn setup() {
        // 기존 환경변수 초기화
        cleanup_env();
        
        // 기본 설정
        std::env::set_var("PROXY_HTTP_PORT", "9090");
        
        // Settings 로드 및 검증
        let settings = Settings::from_env().await.expect("Failed to load settings");
        
        // Mock Docker 클라이언트로 Docker 매니저 생성
        let docker_manager = DockerManager::new(
            Box::new(MockDockerClient::empty()),
            Box::new(DefaultExtractor::new(
                settings.docker.network.clone(),
                settings.docker.label_prefix.clone(),
            )),
            settings.docker.clone(),
        ).await;

        // 나머지 컴포넌트 생성
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        let middleware_manager = MiddlewareManager::new(&settings.middleware);

        // ServerManager 생성
        let server = ServerManager::new(
            settings,
            docker_manager,
            routing_table,
            middleware_manager,
        );

        // 기본값이 제대로 설정되었는지 확인
        assert_eq!(server.config.server.http_port, 9090);
    }

    fn teardown() {
        cleanup_env();
    }

    #[tokio::test]
    #[serial]
    async fn test_server_creation() {
        setup().await;
        
        let settings = Settings::from_env().await.unwrap();
        let docker_manager = DockerManager::new(
            Box::new(MockDockerClient::empty()),
            Box::new(DefaultExtractor::new(
                settings.docker.network.clone(),
                settings.docker.label_prefix.clone(),
            )),
            settings.docker.clone(),
        ).await;

        let server = ServerManager::new(
            settings,
            docker_manager,
            Arc::new(RwLock::new(RoutingTable::new())),
            MiddlewareManager::new(&HashMap::new()),
        );

        // ... assertions
        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_server_with_container_routes() {
        setup().await;
        
        // 테스트용 컨테이너 생성
        let mut labels = std::collections::HashMap::new();
        labels.insert("rproxy.http.routers.test.rule".to_string(), "Host(`test.local`)".to_string());
        labels.insert("rproxy.http.services.test.loadbalancer.server.port".to_string(), "8080".to_string());
        
        let container = ContainerSummary {
            id: Some("test-1".to_string()),
            labels: Some(labels),
            network_settings: Some(bollard::models::ContainerSummaryNetworkSettings {
                networks: Some({
                    let mut networks = std::collections::HashMap::new();
                    networks.insert("test-network".to_string(), bollard::models::EndpointSettings {
                        ip_address: Some("172.17.0.2".to_string()),
                        ..Default::default()
                    });
                    networks
                }),
            }),
            ..Default::default()
        };

        let mock_client = MockDockerClient {
            containers: Arc::new(Mutex::new(vec![container])),
        };

        // 서버 생성 및 테스트
        let mut settings = Settings::from_env().await.unwrap();
        settings.docker.network = "test-network".to_string();
        
        let docker_manager = DockerManager::new(
            Box::new(mock_client),
            Box::new(DefaultExtractor::new(
                settings.docker.network.clone(),
                settings.docker.label_prefix.clone(),
            )),
            settings.docker.clone(),
        ).await;

        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        let middleware_manager = MiddlewareManager::new(&settings.middleware);

        let server = ServerManager::new(
            settings,
            docker_manager,
            routing_table.clone(),
            middleware_manager,
        );

        // 초기 라우트 설정
        let routes = server.docker_manager.get_container_routes().await.unwrap();
        {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(routes);
        }

        // 라우팅 테이블 검증
        let table = routing_table.read().await;
        assert_eq!(table.routes.len(), 1);
        assert!(table.routes.contains_key(&(
            "test.local".to_string(),
            PathMatcher::from_str("/").unwrap()
        )));

        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_server_with_https_settings() {
        setup().await;
        
        // HTTPS 설정
        std::env::set_var("PROXY_HTTPS_ENABLED", "true");
        std::env::set_var("PROXY_HTTPS_PORT", "8443");
        std::env::set_var("PROXY_TLS_CERT", "/path/to/cert.pem");
        std::env::set_var("PROXY_TLS_KEY", "/path/to/key.pem");
        
        let settings = Settings::from_env().await.unwrap();
        let docker_manager = DockerManager::new(
            Box::new(MockDockerClient::empty()),
            Box::new(DefaultExtractor::new(
                settings.docker.network.clone(),
                settings.docker.label_prefix.clone(),
            )),
            settings.docker.clone(),
        ).await;

        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        let middleware_manager = MiddlewareManager::new(&settings.middleware);

        let server = ServerManager::new(
            settings,
            docker_manager,
            routing_table,
            middleware_manager,
        );

        // HTTPS 설정 검증
        assert!(server.config.server.https_enabled);
        assert_eq!(server.config.server.https_port, 8443);
        assert_eq!(server.config.server.tls_cert_path.unwrap(), "/path/to/cert.pem");
        assert_eq!(server.config.server.tls_key_path.unwrap(), "/path/to/key.pem");
        
        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_server_with_path_based_routing() {
        setup().await;
        
        // 테스트용 컨테이너 생성 - 경로 기반 라우팅 규칙 추가
        let mut labels = std::collections::HashMap::new();
        labels.insert(
            "rproxy.http.routers.test.rule".to_string(), 
            "Host(`test.local`) && PathPrefix(`/api`)".to_string()
        );
        labels.insert(
            "rproxy.http.services.test.loadbalancer.server.port".to_string(),
            "8080".to_string()
        );
        
        let container = ContainerSummary {
            id: Some("test-1".to_string()),
            labels: Some(labels),
            network_settings: Some(bollard::models::ContainerSummaryNetworkSettings {
                networks: Some({
                    let mut networks = std::collections::HashMap::new();
                    networks.insert("test-network".to_string(), bollard::models::EndpointSettings {
                        ip_address: Some("172.17.0.2".to_string()),
                        ..Default::default()
                    });
                    networks
                }),
            }),
            ..Default::default()
        };

        let mock_client = MockDockerClient {
            containers: Arc::new(Mutex::new(vec![container])),
        };

        // 서버 생성 및 테스트
        let mut settings = Settings::from_env().await.unwrap();
        settings.docker.network = "test-network".to_string();
        
        let docker_manager = DockerManager::new(
            Box::new(mock_client),
            Box::new(DefaultExtractor::new(
                settings.docker.network.clone(),
                settings.docker.label_prefix.clone(),
            )),
            settings.docker.clone(),
        ).await;

        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        let middleware_manager = MiddlewareManager::new(&settings.middleware);

        let server = ServerManager::new(
            settings,
            docker_manager,
            routing_table.clone(),
            middleware_manager,
        );

        // 초기 라우트 설정
        let routes = server.docker_manager.get_container_routes().await.unwrap();
        {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(routes);
        }

        // 라우팅 테이블 검증
        let table = routing_table.read().await;
        assert_eq!(table.routes.len(), 1);
        assert!(table.routes.contains_key(&(
            "test.local".to_string(),
            PathMatcher::from_str("/api*").unwrap()
        )));

        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_server_with_middleware() {
        setup().await;
        
        std::env::set_var("PROXY_MIDDLEWARE_AUTH_TYPE", "basic-auth");
        std::env::set_var("PROXY_MIDDLEWARE_AUTH_ENABLED", "true");
        
        let settings = Settings::from_env().await.unwrap();
        
        // 컨테이너 설정
        let mut labels = std::collections::HashMap::new();
        labels.insert(
            "rproxy.http.routers.test.rule".to_string(), 
            "Host(`test.local`)".to_string()
        );
        labels.insert(
            "rproxy.http.middlewares.test-auth.type".to_string(),
            "basicauth".to_string()
        );
        labels.insert(
            "rproxy.http.middlewares.test-auth.basicAuth.users".to_string(),
            "test:$2y$05$123456789abcdef".to_string()
        );
        labels.insert(
            "rproxy.http.routers.test.middlewares".to_string(),
            "test-auth".to_string()
        );
        labels.insert(
            "rproxy.http.services.test.loadbalancer.server.port".to_string(),
            "8080".to_string()
        );

        // Mock Docker 클라이언트 생성
        let client = MockDockerClient {
            containers: Arc::new(Mutex::new(vec![ContainerSummary {
                id: Some("test-1".to_string()),
                labels: Some(labels),
                network_settings: Some(ContainerSummaryNetworkSettings {
                    networks: Some(HashMap::from([
                        ("test-network".to_string(), EndpointSettings {
                            ip_address: Some("172.17.0.2".to_string()),
                            ..Default::default()
                        })
                    ])),
                }),
                ..Default::default()
            }])),
        };

        let docker_manager = DockerManager::new(
            Box::new(client),
            Box::new(DefaultExtractor::new(
                settings.docker.network.clone(),
                settings.docker.label_prefix.clone(),
            )),
            settings.docker.clone(),
        ).await;

        let server = ServerManager::new(
            settings,
            docker_manager,
            Arc::new(RwLock::new(RoutingTable::new())),
            MiddlewareManager::new(&HashMap::new()),
        );

        // ... 나머지 테스트 코드는 동일
        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_server_with_invalid_middleware() {
        setup().await;
        
        let mut settings = Settings::from_env().await.unwrap();
        settings.docker.network = "test-network".to_string();
        
        let mut labels = HashMap::new();
        labels.insert(
            "rproxy.http.routers.test.rule".to_string(),
            "Host(`test.local`)".to_string()
        );
        labels.insert(
            "rproxy.http.routers.test.middlewares".to_string(),
            "non-existent-middleware".to_string()
        );
        labels.insert(
            "rproxy.http.services.test.loadbalancer.server.port".to_string(),
            "8080".to_string()
        );

        let client = MockDockerClient {
            containers: Arc::new(Mutex::new(vec![ContainerSummary {
                id: Some("test-container".to_string()),
                labels: Some(labels),
                network_settings: Some(ContainerSummaryNetworkSettings {
                    networks: Some(HashMap::from([
                        ("test-network".to_string(), EndpointSettings {
                            ip_address: Some("172.17.0.2".to_string()),
                            ..Default::default()
                        })
                    ])),
                }),
                ..Default::default()
            }])),
        };

        let docker_manager = DockerManager::new(
            Box::new(client),
            Box::new(DefaultExtractor::new(
                settings.docker.network.clone(),
                settings.docker.label_prefix.clone(),
            )),
            settings.docker.clone(),
        ).await;

        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        let middleware_manager = MiddlewareManager::new(&settings.middleware);

        let server = ServerManager::new(
            settings.clone(),
            docker_manager,
            routing_table.clone(),
            middleware_manager,
        );

        // 초기 라우트 설정
        let routes = server.docker_manager.get_container_routes().await.unwrap();
        {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(routes);
        }

        // 라우팅 테이블 검증
        let table = routing_table.read().await;
        let route = table.routes.get(&(
            "test.local".to_string(),
            PathMatcher::from_str("/").unwrap()
        )).unwrap();

        // 존재하지 않는 미들웨어가 설정되어 있는지 확인
        match &route.middlewares {
            Some(middlewares) => {
                assert!(middlewares.contains(&"non-existent-middleware".to_string()));
                // 미들웨어가 존재하지만 실제로는 설정되지 않은 상태
                assert!(settings.middleware.get("non-existent-middleware").is_none());
            },
            None => panic!("Expected middlewares to be configured"),
        }

        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_server_with_empty_middleware_chain() {
        setup().await;
        
        let mut settings = Settings::from_env().await.unwrap();
        settings.docker.network = "test-network".to_string();
        
        let mut labels = HashMap::new();
        labels.insert(
            "rproxy.http.routers.test.rule".to_string(),
            "Host(`test.local`)".to_string()
        );
        labels.insert(
            "rproxy.http.routers.test.middlewares".to_string(),
            "".to_string()
        );
        labels.insert(
            "rproxy.http.services.test.loadbalancer.server.port".to_string(),
            "8080".to_string()
        );

        let client = MockDockerClient {
            containers: Arc::new(Mutex::new(vec![ContainerSummary {
                id: Some("test-container".to_string()),
                labels: Some(labels),
                network_settings: Some(ContainerSummaryNetworkSettings {
                    networks: Some(HashMap::from([
                        ("test-network".to_string(), EndpointSettings {
                            ip_address: Some("172.17.0.2".to_string()),
                            ..Default::default()
                        })
                    ])),
                }),
                ..Default::default()
            }])),
        };

        let docker_manager = DockerManager::new(
            Box::new(client),
            Box::new(DefaultExtractor::new(
                settings.docker.network.clone(),
                settings.docker.label_prefix.clone(),
            )),
            settings.docker.clone(),
        ).await;

        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        let middleware_manager = MiddlewareManager::new(&settings.middleware);

        let server = ServerManager::new(
            settings,
            docker_manager,
            routing_table.clone(),
            middleware_manager,
        );

        // 초기 라우트 설정
        let routes = server.docker_manager.get_container_routes().await.unwrap();
        {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(routes);
        }

        // 라우팅 테이블 검증
        let table = routing_table.read().await;
        let route = table.routes.get(&(
            "test.local".to_string(),
            PathMatcher::from_str("/").unwrap()
        )).unwrap();

        // 빈 미들웨어 체인 확인
        match &route.middlewares {
            Some(middlewares) => {
                assert!(middlewares.is_empty(), "Expected empty middleware chain");
            },
            None => (), // 빈 문자열이므로 None이어도 괜찮음
        }

        teardown();
    }
} 