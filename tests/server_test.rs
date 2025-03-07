use reverse_proxy_traefik::{
    settings::{Settings, HttpsDisabled, HttpsEnabled},
    server::manager_v2::ServerManager,
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
use serial_test::serial;
use std::collections::HashMap;
use std::sync::Mutex;
use bollard::secret::{ContainerSummaryNetworkSettings, EndpointSettings};

// Mock Docker 클라이언트 구현
struct MockDockerClient {
    containers: Arc<Mutex<Vec<ContainerSummary>>>,
}

impl MockDockerClient {
    fn empty() -> Self {
        Self {
            containers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn with_containers(containers: Vec<ContainerSummary>) -> Self {
        Self {
            containers: Arc::new(Mutex::new(containers)),
        }
    }
}

#[async_trait]
impl DockerClient for MockDockerClient {
    fn clone_box(&self) -> Box<dyn DockerClient> {
        Box::new(Self {
            containers: self.containers.clone(),
        })
    }

    async fn list_containers(&self, _options: Option<ListContainersOptions<String>>) 
        -> Result<Vec<ContainerSummary>, DockerError> 
    {
        Ok(self.containers.lock().unwrap().clone())
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
    
    // 테스트 환경 정리 (환경 변수 초기화)
    fn cleanup_env() {
        let vars = vec![
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
    }

    // 테스트 환경 설정
    async fn setup() {
        // 기존 환경변수 초기화
        cleanup_env();
        
        // 기본 설정
        std::env::set_var("PROXY_HTTP_PORT", "9090");
    }

    // 테스트 환경 정리
    fn teardown() {
        cleanup_env();
    }

    // 기본 서버 생성 테스트
    #[tokio::test]
    #[serial]
    async fn test_server_creation() {
        setup().await;
        
        // HTTP 설정 로드
        let settings_either = Settings::<HttpsDisabled>::load().await.unwrap();
        let settings = match settings_either {
            reverse_proxy_traefik::settings::Either::Left(settings) => settings,
            reverse_proxy_traefik::settings::Either::Right(_) => panic!("Expected HTTP settings")
        };
        
        // Mock 객체 생성
        let mock_client = MockDockerClient::empty();
        
        // Docker 매니저 생성
        let docker_manager = DockerManager::new(
            Box::new(mock_client),
            Box::new(DefaultExtractor::new(
                "default-network".to_string(),
                "rproxy.".to_string(),
            )),
            settings.docker.clone(),
        ).await;
        
        // 라우팅 테이블 생성
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        
        // 미들웨어 매니저 생성
        let middleware_manager = MiddlewareManager::new(
            &HashMap::new(), 
            &HashMap::new()
        );
        
        // 서버 매니저 생성
        let server = ServerManager::<HttpsDisabled>::new(
            settings,
            docker_manager,
            routing_table,
            middleware_manager,
        );
        
        // 설정 검증
        assert_eq!(server.config.server.http_port(), 9090);
        assert!(!server.config.server.https_enabled());
        
        teardown();
    }

    // HTTPS 서버 생성 테스트
    #[tokio::test]
    #[serial]
    async fn test_server_with_https_settings() {
        setup().await;
        
        // HTTPS 환경 변수 설정
        std::env::set_var("PROXY_HTTPS_ENABLED", "true");
        std::env::set_var("PROXY_HTTPS_PORT", "8443");
        std::env::set_var("PROXY_TLS_CERT", "/path/to/cert.pem");
        std::env::set_var("PROXY_TLS_KEY", "/path/to/key.pem");
        
        // HTTPS 설정 로드
        let settings_either = Settings::<HttpsDisabled>::load().await.unwrap();
        
        // HTTPS 설정 추출
        let settings = match settings_either {
            reverse_proxy_traefik::settings::Either::Right(settings) => settings,
            reverse_proxy_traefik::settings::Either::Left(_) => panic!("Expected HTTPS settings")
        };
        
        // Mock 객체 생성
        let mock_client = MockDockerClient::empty();
        
        // Docker 매니저 생성
        let docker_manager = DockerManager::new(
            Box::new(mock_client),
            Box::new(DefaultExtractor::new(
                "default-network".to_string(), 
                "rproxy.".to_string()
            )),
            settings.docker.clone(),
        ).await;
        
        // 라우팅 테이블 생성
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        
        // 미들웨어 매니저 생성
        let middleware_manager = MiddlewareManager::new(
            &HashMap::new(), 
            &HashMap::new()
        );
        
        // HTTPS 서버 매니저 생성
        let server = ServerManager::<HttpsEnabled>::new(
            settings,
            docker_manager,
            routing_table,
            middleware_manager,
        );
        
        // HTTPS 설정 검증
        assert!(server.config.server.https_enabled());
        assert_eq!(server.config.server.http_port(), 9090);
        assert_eq!(server.config.server.https_port(), 8443);
        assert_eq!(server.config.server.tls_cert_path().unwrap(), "/path/to/cert.pem");
        assert_eq!(server.config.server.tls_key_path().unwrap(), "/path/to/key.pem");
        
        teardown();
    }

    // 컨테이너 기반 라우팅 테스트
    #[tokio::test]
    #[serial]
    async fn test_server_with_container_routes() {
        setup().await;
        
        // 테스트용 컨테이너 설정
        let mut labels = HashMap::new();
        labels.insert(
            "rproxy.http.routers.test.rule".to_string(), 
            "Host(`test.local`)".to_string()
        );
        labels.insert(
            "rproxy.http.services.test.loadbalancer.server.port".to_string(),
            "8080".to_string()
        );
        
        // 가상의 컨테이너 생성
        let container = ContainerSummary {
            id: Some("test-1".to_string()),
            labels: Some(labels),
            network_settings: Some(ContainerSummaryNetworkSettings {
                networks: Some({
                    let mut networks = HashMap::new();
                    networks.insert("test-network".to_string(), EndpointSettings {
                        ip_address: Some("172.17.0.2".to_string()),
                        ..Default::default()
                    });
                    networks
                }),
            }),
            ..Default::default()
        };
        
        // Mock Docker 클라이언트 생성
        let mock_client = MockDockerClient::with_containers(vec![container]);
        
        // Settings 로드
        let settings_either = Settings::<HttpsDisabled>::load().await.unwrap();
        let mut settings = match settings_either {
            reverse_proxy_traefik::settings::Either::Left(settings) => settings,
            reverse_proxy_traefik::settings::Either::Right(_) => panic!("Expected HTTP settings")
        };
        
        // 네트워크 설정
        settings.docker.network = "test-network".to_string();
        
        // Docker 매니저 생성
        let docker_manager = DockerManager::new(
            Box::new(mock_client),
            Box::new(DefaultExtractor::new(
                settings.docker.network.clone(),
                settings.docker.label_prefix.clone(),
            )),
            settings.docker.clone(),
        ).await;
        
        // 라우팅 테이블 생성
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        
        // 미들웨어 매니저 생성
        let middleware_manager = MiddlewareManager::new(
            &settings.middleware,
            &settings.router_middlewares
        );
        
        // 서버 매니저 생성
        let server = ServerManager::<HttpsDisabled>::new(
            settings,
            docker_manager,
            routing_table.clone(),
            middleware_manager,
        );
        
        // Docker 이벤트 처리 - 컨테이너 라우트 가져오기
        let container_routes = server.docker_manager.get_container_routes().await.unwrap();
        
        // 라우팅 테이블에 수동으로 추가
        let mut routing_table_write = routing_table.write().await;
        routing_table_write.sync_docker_routes(container_routes);
        drop(routing_table_write);
        
        // 라우팅 테이블 검증
        let routing_table_read = routing_table.read().await;
        let routes = &routing_table_read.routes;
        assert_eq!(routes.len(), 1, "Expected 1 route, found {}", routes.len());
        
        // 라우트 검증 - 키가 (host, path_matcher) 구조로 되어 있음
        for ((host, _), service) in routes {
            assert_eq!(host, "test.local", "Expected host to be 'test.local'");
            assert_eq!(service.address.ip().to_string(), "172.17.0.2");
            assert_eq!(service.address.port(), 8080);
        }
        
        teardown();
    }
} 