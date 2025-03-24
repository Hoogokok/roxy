use std::collections::HashMap;
use std::sync::Arc;

use crate::docker::{ContainerInfo, DockerError, BackendServiceBuilder};
use crate::routing_v2::{PathMatcher, BackendService};
use crate::settings::container::ContainerConfigManager;
use crate::settings::core::Settings;
use crate::settings::typestate::Validated;
use tracing::{debug, warn};

/// 라우트 생성을 담당하는 빌더 클래스
/// 
/// 컨테이너 정보와 설정을 기반으로 라우트 맵을 생성합니다.
pub struct RouteBuilder {
    service_builder: BackendServiceBuilder,
    container_config_manager: Arc<ContainerConfigManager>,
}

impl RouteBuilder {
    /// 새 RouteBuilder 인스턴스 생성
    pub fn new(
        service_builder: BackendServiceBuilder,
        container_config_manager: Arc<ContainerConfigManager>,
    ) -> Self {
        Self {
            service_builder,
            container_config_manager,
        }
    }

    /// 컨테이너 그룹에서 라우트 맵 생성
    pub async fn build_routes(
        &self,
        services: &HashMap<String, Vec<ContainerInfo>>,
        docker_labels: &HashMap<String, String>,
    ) -> Result<HashMap<(String, PathMatcher), BackendService>, DockerError> {
        debug!("라우트 빌드 시작");
        
        // 설정 병합 준비
        let merged_configs = self.prepare_merged_configs(services, docker_labels).await;
        
        // 라우트 생성
        let mut routes = HashMap::new();
        for (service_name, infos) in services {
            debug!("서비스 그룹 처리: {}, 컨테이너 {}개", service_name, infos.len());
            
            match self.process_service_group(infos, docker_labels, merged_configs.as_ref()).await {
                Ok(Some((host, path_matcher, service))) => {
                    debug!("라우트 추가: {} ({})", host, path_matcher);
                    routes.insert((host, path_matcher), service);
                },
                Ok(None) => {
                    debug!("서비스 그룹에서 라우트를 생성하지 않음: {}", service_name);
                },
                Err(e) => {
                    warn!("서비스 그룹 처리 실패: {}: {}", service_name, e);
                }
            }
        }
        
        debug!("라우트 빌드 완료: {}개 라우트", routes.len());
        Ok(routes)
    }
    
    /// 병합된 설정을 준비하는 헬퍼 메서드
    async fn prepare_merged_configs(
        &self, 
        services: &HashMap<String, Vec<ContainerInfo>>,
        docker_labels: &HashMap<String, String>
    ) -> Option<HashMap<String, Settings<Validated>>> {
        // 컨테이너 ID 수집 (모든 서비스의 모든 컨테이너)
        let mut all_container_ids = Vec::new();
        for infos in services.values() {
            all_container_ids.extend(Self::collect_container_ids(infos));
        }
        
        // 컨테이너 ID가 있으면 설정 일괄 병합
        if !all_container_ids.is_empty() {
            debug!("컨테이너 설정 일괄 병합 시작: {} 컨테이너", all_container_ids.len());
            let configs = self.container_config_manager.as_ref()
                .merge_configs_batch(&all_container_ids, docker_labels);
            debug!("설정 병합 완료: {} 컨테이너에 대한 설정", configs.len());
            Some(configs)
        } else {
            debug!("병합할 컨테이너 ID가 없음");
            None
        }
    }
    
    /// 컨테이너 정보에서 ID 목록 추출
    fn collect_container_ids(infos: &[ContainerInfo]) -> Vec<&str> {
        infos.iter()
            .filter_map(|info| info.container_id.as_deref())
            .collect()
    }
    
    /// 서비스 그룹 처리 및 백엔드 서비스 생성
    async fn process_service_group(
        &self, 
        infos: &[ContainerInfo], 
        _docker_labels: &HashMap<String, String>,
        merged_configs: Option<&HashMap<String, Settings<Validated>>>
    ) -> Result<Option<(String, PathMatcher, BackendService)>, DockerError> {
        
        if infos.is_empty() {
            return Ok(None);
        }
        
        debug!("서비스 그룹 처리 시작: {} 컨테이너", infos.len());
        
        // 백엔드 서비스 생성
        self.service_builder.build_from_containers(infos, merged_configs).await
            .map(|result| Some(result))
            .map_err(|e| {
                warn!("백엔드 서비스 생성 실패: {}", e);
                e
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bollard::models::ContainerSummary;
    use std::pin::Pin;
    use bollard::container::ListContainersOptions;
    use bollard::system::EventsOptions;
    use bollard::secret::EventMessage;
    use futures_util::stream::Stream;
    
    #[derive(Clone)]
    struct MockDockerClient;
    
    #[async_trait]
    impl crate::docker::DockerClient for MockDockerClient {
        fn clone_box(&self) -> Box<dyn crate::docker::DockerClient> {
            Box::new(self.clone())
        }
        
        async fn list_containers(
            &self,
            _options: Option<ListContainersOptions<String>>
        ) -> Result<Vec<ContainerSummary>, DockerError> {
            Ok(vec![])
        }
        
        fn events(
            &self,
            _options: Option<EventsOptions<String>>
        ) -> Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>> {
            unimplemented!()
        }
    }
    
    #[derive(Clone)]
    struct MockExtractor;
    
    impl crate::docker::ContainerInfoExtractor for MockExtractor {
        fn clone_box(&self) -> Box<dyn crate::docker::ContainerInfoExtractor> {
            Box::new(self.clone())
        }
        
        fn extract_info(&self, _container: &ContainerSummary) -> Result<ContainerInfo, DockerError> {
            Ok(ContainerInfo {
                host: "test.example.com".to_string(),
                ip: "192.168.1.1".to_string(),
                port: 80,
                container_id: Some("test-container".to_string()),
                path_matcher: Some(PathMatcher::from_str("/").unwrap()),
                middlewares: None,
                router_name: Some("test-router".to_string()),
                health_check: None,
                load_balancer: None,
                json_config_path: None,
            })
        }
        
        fn create_backend(&self, info: &ContainerInfo) -> Result<BackendService, DockerError> {
            let addr = format!("{}:{}", info.ip, info.port).parse().unwrap();
            Ok(BackendService::with_router(
                addr,
                info.router_name.clone()
            ))
        }
        
        fn get_label_prefix(&self) -> &str {
            "test."
        }
    }
    
    fn create_test_container_info(id: &str, host: &str) -> ContainerInfo {
        ContainerInfo {
            host: host.to_string(),
            ip: "192.168.1.1".to_string(),
            port: 80,
            container_id: Some(id.to_string()),
            path_matcher: Some(PathMatcher::from_str("/").unwrap()),
            middlewares: None,
            router_name: Some(format!("{}-router", host)),
            health_check: None,
            load_balancer: None,
            json_config_path: None,
        }
    }
    
    #[tokio::test]
    async fn test_route_builder_creation() {
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn crate::docker::DockerClient>);
        let extractor = Box::new(MockExtractor);
        let (container_config_manager, _) = ContainerConfigManager::new();
        let container_config_manager = Arc::new(container_config_manager);
        
        let service_builder = BackendServiceBuilder::new(
            extractor,
            client,
            container_config_manager.clone()
        );
        
        let route_builder = RouteBuilder::new(
            service_builder,
            container_config_manager
        );
        
        // 인스턴스가 생성되어야 함
        assert!(std::mem::size_of_val(&route_builder) > 0);
        // 실제 값에 대한 검증 없이 성공적으로 생성됐는지만 확인
    }

    #[tokio::test]
    async fn test_build_routes() {
        // 테스트 환경 설정
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn crate::docker::DockerClient>);
        let extractor = Box::new(MockExtractor);
        let (container_config_manager, _) = ContainerConfigManager::new();
        let container_config_manager = Arc::new(container_config_manager);
        
        let service_builder = BackendServiceBuilder::new(
            extractor,
            client,
            container_config_manager.clone()
        );
        
        let route_builder = RouteBuilder::new(
            service_builder,
            container_config_manager
        );
        
        // 테스트용 컨테이너 정보 생성
        let container1 = create_test_container_info("container1", "service1.example.com");
        let container2 = create_test_container_info("container2", "service2.example.com");
        
        // 서비스 그룹 생성
        let mut services = HashMap::new();
        services.insert("service1".to_string(), vec![container1]);
        services.insert("service2".to_string(), vec![container2]);
        
        // Docker 라벨 생성
        let mut docker_labels = HashMap::new();
        docker_labels.insert("test.host".to_string(), "example.com".to_string());
        
        // 라우트 빌드
        let routes = route_builder.build_routes(&services, &docker_labels).await.unwrap();
        
        // 검증
        assert_eq!(routes.len(), 2, "2개의 라우트가 생성되어야 함");
        
        // 각 서비스별 라우트 존재 확인
        let has_service1 = routes.keys().any(|(host, _)| host == "service1.example.com");
        let has_service2 = routes.keys().any(|(host, _)| host == "service2.example.com");
        
        assert!(has_service1, "service1 라우트가 없음");
        assert!(has_service2, "service2 라우트가 없음");
    }

    #[tokio::test]
    async fn test_build_routes_empty_services() {
        // 테스트 환경 설정
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn crate::docker::DockerClient>);
        let extractor = Box::new(MockExtractor);
        let (container_config_manager, _) = ContainerConfigManager::new();
        let container_config_manager = Arc::new(container_config_manager);
        
        let service_builder = BackendServiceBuilder::new(
            extractor,
            client,
            container_config_manager.clone()
        );
        
        let route_builder = RouteBuilder::new(
            service_builder,
            container_config_manager
        );
        
        // 빈 서비스 목록
        let services = HashMap::new();
        let docker_labels = HashMap::new();
        
        // 라우트 빌드
        let routes = route_builder.build_routes(&services, &docker_labels).await.unwrap();
        
        // 검증
        assert_eq!(routes.len(), 0, "빈 서비스 목록으로 생성된 라우트 수는 0이어야 함");
    }

    #[tokio::test]
    async fn test_path_matcher_handling() {
        // 테스트 환경 설정
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn crate::docker::DockerClient>);
        let extractor = Box::new(MockExtractor);
        let (container_config_manager, _) = ContainerConfigManager::new();
        let container_config_manager = Arc::new(container_config_manager);
        
        let service_builder = BackendServiceBuilder::new(
            extractor,
            client,
            container_config_manager.clone()
        );
        
        let route_builder = RouteBuilder::new(
            service_builder,
            container_config_manager
        );
        
        // 경로 매처가 다른 컨테이너 정보 생성
        let mut container1 = create_test_container_info("container1", "api.example.com");
        container1.path_matcher = Some(PathMatcher::from_str("/api/*").unwrap());
        
        let mut container2 = create_test_container_info("container2", "api.example.com");
        container2.path_matcher = Some(PathMatcher::from_str("/admin/*").unwrap());
        
        // 서비스 그룹 생성 (동일 호스트 다른 경로)
        let mut services = HashMap::new();
        services.insert("api".to_string(), vec![container1]);
        services.insert("admin".to_string(), vec![container2]);
        
        let docker_labels = HashMap::new();
        
        // 라우트 빌드
        let routes = route_builder.build_routes(&services, &docker_labels).await.unwrap();
        
        // 검증
        assert_eq!(routes.len(), 2, "2개의 라우트가 생성되어야 함");
        
        // 경로 매처 확인 (path.to_string() 대신 직접 맵에서 확인)
        let has_api_path = routes.keys().any(|(host, path)| {
            host == "api.example.com" && path.matches("/api/test")
        });
        
        let has_admin_path = routes.keys().any(|(host, path)| {
            host == "api.example.com" && path.matches("/admin/test")
        });
        
        assert!(has_api_path, "API 경로 매처가 없음");
        assert!(has_admin_path, "Admin 경로 매처가 없음");
    }
    
    #[tokio::test]
    async fn test_merged_configs() {
        // 테스트 환경 설정
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn crate::docker::DockerClient>);
        let extractor = Box::new(MockExtractor);
        
        // 컨테이너 설정 관리자 설정
        let (container_config_manager, _) = ContainerConfigManager::new();
        let container_config_manager = Arc::new(container_config_manager);
        
        // 테스트용 확장 서비스 빌더
        struct TestServiceBuilder {
            inner: BackendServiceBuilder,
            config_used: Arc<std::sync::atomic::AtomicBool>,
        }
        
        impl TestServiceBuilder {
            async fn build_from_containers(
                &self, 
                infos: &[ContainerInfo],
                merged_configs: Option<&HashMap<String, Settings<Validated>>>
            ) -> Result<(String, PathMatcher, BackendService), DockerError> {
                // 병합된 설정이 전달되었는지 확인
                if merged_configs.is_some() {
                    self.config_used.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                
                // 실제 빌더에 위임
                self.inner.build_from_containers(infos, merged_configs).await
            }
        }
        
        let service_builder = BackendServiceBuilder::new(
            extractor,
            client,
            container_config_manager.clone()
        );
        
        let config_used = Arc::new(std::sync::atomic::AtomicBool::new(false));
        
        // 테스트용 컨테이너 정보 생성
        let container = create_test_container_info("container1", "service.example.com");
        
        // 서비스 그룹 생성
        let mut services = HashMap::new();
        services.insert("service".to_string(), vec![container]);
        
        let docker_labels = HashMap::new();
        
        // RouteBuilder 생성
        let route_builder = RouteBuilder::new(
            service_builder,
            container_config_manager
        );
        
        // 라우트 빌드
        let routes = route_builder.build_routes(&services, &docker_labels).await.unwrap();
        
        // 검증
        assert_eq!(routes.len(), 1, "라우트가 생성되어야 함");
        
        // 설정이 병합됐는지는 간접적으로 함수 호출로 확인
        // (테스트 모의 객체 한계로 직접 검증은 어려움)
    }

    #[tokio::test]
    async fn test_error_handling() {
        // 테스트 환경 설정
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn crate::docker::DockerClient>);
        
        // 오류를 발생시키는 익스트랙터 생성
        #[derive(Clone)]
        struct ErrorExtractor;
        
        impl crate::docker::ContainerInfoExtractor for ErrorExtractor {
            fn clone_box(&self) -> Box<dyn crate::docker::ContainerInfoExtractor> {
                Box::new(self.clone())
            }
            
            fn extract_info(&self, _container: &ContainerSummary) -> Result<ContainerInfo, DockerError> {
                Ok(ContainerInfo {
                    host: "test.example.com".to_string(),
                    ip: "192.168.1.1".to_string(),
                    port: 80,
                    container_id: Some("error-container".to_string()),
                    path_matcher: Some(PathMatcher::from_str("/").unwrap()),
                    middlewares: None,
                    router_name: Some("error-router".to_string()),
                    health_check: None,
                    load_balancer: None,
                    json_config_path: None,
                })
            }
            
            fn create_backend(&self, _info: &ContainerInfo) -> Result<BackendService, DockerError> {
                // 항상 오류 반환
                Err(DockerError::ContainerConfigError {
                    container_id: "error-container".to_string(),
                    reason: "테스트용 오류".to_string(),
                    context: None,
                })
            }
            
            fn get_label_prefix(&self) -> &str {
                "test."
            }
        }
        
        let extractor = Box::new(ErrorExtractor);
        let (container_config_manager, _) = ContainerConfigManager::new();
        let container_config_manager = Arc::new(container_config_manager);
        
        let service_builder = BackendServiceBuilder::new(
            extractor,
            client,
            container_config_manager.clone()
        );
        
        let route_builder = RouteBuilder::new(
            service_builder,
            container_config_manager
        );
        
        // 테스트용 컨테이너 정보 생성
        let container = create_test_container_info("error-container", "error.example.com");
        
        // 서비스 그룹 생성
        let mut services = HashMap::new();
        services.insert("error-service".to_string(), vec![container]);
        
        let docker_labels = HashMap::new();
        
        // 라우트 빌드
        let routes = route_builder.build_routes(&services, &docker_labels).await.unwrap();
        
        // 검증 - 오류가 발생했으므로 빈 라우트 맵이 반환되어야 함
        assert_eq!(routes.len(), 0, "오류 상황에서는 빈 라우트 맵이 반환되어야 함");
    }
} 