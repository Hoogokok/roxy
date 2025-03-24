use std::sync::Arc;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;

use crate::docker::DockerError;
use crate::docker::ContainerInfo;
use crate::docker::ContainerInfoExtractor;
use crate::docker::DockerClient;
use crate::routing_v2::{BackendService, PathMatcher, LoadBalancerStrategy};
use crate::settings::container::ContainerConfigManager;
use bollard::container::ListContainersOptions;
use tracing::debug;


/// 백엔드 서비스 생성을 담당하는 빌더 클래스입니다.
/// Docker Manager에서 서비스 생성 로직을 분리하여 단일 책임 원칙을 준수합니다.
#[derive(Clone)]
pub struct BackendServiceBuilder {
    extractor: Box<dyn ContainerInfoExtractor>,
    client: Arc<Box<dyn DockerClient>>,
    container_config_manager: Arc<ContainerConfigManager>,
}

impl BackendServiceBuilder {
    /// 새로운 백엔드 서비스 빌더를 생성합니다.
    pub fn new(
        extractor: Box<dyn ContainerInfoExtractor>,
        client: Arc<Box<dyn DockerClient>>,
        container_config_manager: Arc<ContainerConfigManager>,
    ) -> Self {
        Self {
            extractor,
            client,
            container_config_manager,
        }
    }

    /// 컨테이너 정보 배열에서 백엔드 서비스를 구축합니다.
    pub async fn build_from_containers(&self, infos: &[ContainerInfo]) -> Result<(String, PathMatcher, BackendService), DockerError> {
        debug!("백엔드 서비스 생성 시작");
        
        // 1. 입력 검증
        self.validate_container_infos(infos)?;
        
        // 2. 첫 번째 컨테이너에서 기본 정보 추출
        let first = &infos[0];
        let (host, path_matcher) = self.extract_service_info(first);
        
        // 3. 기본 백엔드 서비스 생성
        let mut service = self.extractor.create_backend(first)?;
        
        // 4. JSON 설정 적용
        self.apply_json_settings(first, &mut service).await?;
        
        // 5. 로드밸런서 설정
        if infos.len() > 1 {
            self.configure_load_balancer(infos, &mut service).await?;
        }
        
        // 6. 미들웨어 설정
        self.apply_middlewares(first, &mut service);
        
        Ok((host, path_matcher, service))
    }
    
    // 이하 내부 구현 메서드들...
    
    /// 입력된 컨테이너 정보 배열이 유효한지 검증합니다.
    fn validate_container_infos(&self, infos: &[ContainerInfo]) -> Result<(), DockerError> {
        if infos.is_empty() {
            return Err(DockerError::ContainerConfigError {
                container_id: "unknown".to_string(),
                reason: "서비스 정의가 비어 있습니다".to_string(),
                context: None,
            });
        }
        Ok(())
    }
    
    /// 컨테이너 정보에서 서비스 호스트와 경로 매처를 추출합니다.
    fn extract_service_info(&self, container: &ContainerInfo) -> (String, PathMatcher) {
        let host = container.host.clone();
        let path_matcher = container.path_matcher.clone()
            .unwrap_or_else(|| PathMatcher::from_str("/").unwrap());
        (host, path_matcher)
    }
    
    /// JSON 설정을 서비스에 적용합니다.
    async fn apply_json_settings(&self, container: &ContainerInfo, service: &mut BackendService) -> Result<(), DockerError> {
        if let Some(container_id) = &container.container_id {
            let labels = self.get_container_labels(container_id).await?;
            
            // 1. 라벨에서 직접 포트 추출
            // 라우터 이름 가져오기 (기본값은 'test')
            let router_name = container.router_name.as_deref().unwrap_or("test");
            
            // extractor에서 라벨 프리픽스 가져오기
            let label_prefix = self.extractor.get_label_prefix();
            
            // 포트 라벨 키 구성
            let port_label_key = format!("{}http.services.{}.loadbalancer.server.port", label_prefix, router_name);
            println!("라벨 키 생성: {}", port_label_key);
            
            if let Some(port_str) = labels.get(&port_label_key) {
                if let Ok(port) = port_str.parse::<u16>() {
                    if port != container.port {
                        service.update_port(port);
                        return Ok(());
                    }
                }
            }
            
            // 2. JSON 설정과 라벨 병합 (기존 코드)
            let settings = self.container_config_manager.merge_config(container_id, &labels);
            
            // HTTP 포트 적용
            let http_port = settings.server.http_port();
            
            if http_port != container.port {
                debug!(
                    container_id = %container_id,
                    original_port = %container.port,
                    new_port = %http_port,
                    "JSON 설정에서 HTTP 포트 업데이트"
                );
                service.update_port(http_port);
            } else {
                debug!(
                    container_id = %container_id,
                    port = %http_port,
                    "JSON 설정에서 HTTP 포트 유지"
                );}
        }
        Ok(())
    }
    
    /// 컨테이너 라벨을 가져옵니다.
    async fn get_container_labels(&self, container_id: &str) -> Result<HashMap<String, String>, DockerError> {
        match self.client.list_containers(Some(ListContainersOptions::<String> {
            all: true,
            filters: {
                let mut filters = HashMap::new();
                filters.insert("id".to_string(), vec![container_id.to_string()]);
                filters
            },
            ..Default::default()
        })).await {
            Ok(containers) => {
                if let Some(container) = containers.first() {
                    Ok(container.labels.clone().unwrap_or_default())
                } else {
                    Ok(HashMap::new())
                }
            },
            Err(e) => {
                debug!("컨테이너 라벨 가져오기 실패: {}, 기본값 사용", e);
                Ok(HashMap::new())
            }
        }
    }
    
    /// 로드밸런서를 설정합니다.
    async fn configure_load_balancer(&self, infos: &[ContainerInfo], service: &mut BackendService) -> Result<(), DockerError> {
        debug!("로드밸런서 설정: {} 컨테이너", infos.len());
        
        // 로드밸런서 활성화
        let strategy = infos[0].load_balancer.clone()
            .unwrap_or(LoadBalancerStrategy::RoundRobin {
                current_index: AtomicUsize::new(0),
            });
        
        service.enable_load_balancer(strategy);
        
        // 추가 컨테이너 처리
        for info in &infos[1..] {
            // 기본 IP:PORT 주소
            let addr = self.extractor.parse_socket_addr(&info.ip, info.port)?;
            // weight는 기본값 1 사용
            service.add_address(addr, 1)?;
            
            // 추가 컨테이너 JSON 설정 적용
            self.update_backend_port(info, service).await?;
        }
        
        Ok(())
    }
    
    /// 백엔드 포트를 업데이트합니다.
    async fn update_backend_port(&self, info: &ContainerInfo, service: &mut BackendService) -> Result<(), DockerError> {
        if let Some(container_id) = &info.container_id {
            let labels = self.get_container_labels(container_id).await?;
            
            // JSON 설정과 Docker 라벨 병합
            let settings = self.container_config_manager.merge_config(container_id, &labels);
            
            // HTTP 포트가 설정되어 있으면 마지막 추가된 주소의 포트 업데이트
            let http_port = settings.server.http_port();
            if http_port != info.port {
                debug!("추가 컨테이너 {}의 HTTP 포트 업데이트: {}", container_id, http_port);
                
                // 로드밸런서에서 해당 백엔드 찾아서 업데이트
                if let Some(lb) = &mut service.load_balancer {
                    if let Some(last) = lb.addresses.last_mut() {
                        // 새 주소 생성
                        let new_addr = format!("{}:{}", last.0.ip().to_string(), http_port)
                            .parse()
                            .map_err(|_| DockerError::AddressParseError {
                                container_id: container_id.clone(),
                                address: format!("{}:{}", last.0.ip().to_string(), http_port),
                                network: "unknown".to_string(),
                                context: None,
                            })?;
                        
                        // 주소 업데이트
                        last.0 = new_addr;
                    }
                }
            }
        }
        Ok(())
    }
    
    /// 미들웨어 설정을 적용합니다.
    fn apply_middlewares(&self, container: &ContainerInfo, service: &mut BackendService) {
        if let Some(middlewares) = &container.middlewares {
            service.set_middlewares(middlewares.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::DefaultExtractor;
    use std::pin::Pin;
    use futures_util::stream::Stream;
    use async_trait::async_trait;
    use bollard::models::{ContainerSummary, EventMessage, EndpointSettings, ContainerSummaryNetworkSettings};
    use bollard::system::EventsOptions;
    
    // 테스트용 DockerClient 구현
    struct MockDockerClient;
    
    #[async_trait]
    impl DockerClient for MockDockerClient {
        fn clone_box(&self) -> Box<dyn DockerClient> {
            Box::new(Self)
        }
        
        async fn list_containers(
            &self,
            _options: Option<ListContainersOptions<String>>
        ) -> Result<Vec<ContainerSummary>, DockerError> {
            let mut container = ContainerSummary::default();
            container.id = Some("test-container".to_string());
            
            // 테스트용 라벨 설정 - 포트 9090으로 설정
            let mut labels = HashMap::new();
            labels.insert("rproxy.enable".to_string(), "true".to_string());
            labels.insert("rproxy.http.routers.test.rule".to_string(), "Host(`test-host.com`)".to_string());
            labels.insert("rproxy.http.services.test.loadbalancer.server.port".to_string(), "9090".to_string());
            container.labels = Some(labels);
            
            // 네트워크 설정 추가
            let mut networks = HashMap::new();
            let endpoint = EndpointSettings {
                ip_address: Some("127.0.0.1".to_string()),
                ..Default::default()
            };
            networks.insert("bridge".to_string(), endpoint);
            
            let network_settings = ContainerSummaryNetworkSettings {
                networks: Some(networks),
                ..Default::default()
            };
            container.network_settings = Some(network_settings);
            
            Ok(vec![container])
        }
        
        fn events(
            &self,
            _options: Option<EventsOptions<String>>
        ) -> Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>> {
            Box::pin(futures_util::stream::empty())
        }
    }
    
    #[tokio::test]
    async fn test_build_from_containers() {
        // 테스트 준비
        let extractor = Box::new(DefaultExtractor::new(
            "bridge".to_string(),
            "rproxy.".to_string(),
        ));
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn DockerClient>);
        let (container_config_manager, _) = ContainerConfigManager::new();
        
        // 테스트용 컨테이너 ID
        let container_id = "test-container";
        
        // ContainerConfigManager에서는 기본 설정 값이 80인데,
        // Docker 라벨에 9090을 지정했으므로 9090으로 업데이트되어야 함
        
        let builder = BackendServiceBuilder::new(
            extractor,
            client,
            Arc::new(container_config_manager),
        );
        
        // 테스트 데이터 생성
        let container_info = ContainerInfo {
            host: "test-host.com".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 80,
            container_id: Some(container_id.to_string()),
            path_matcher: None,
            middlewares: None,
            router_name: Some("test".to_string()),
            health_check: None,
            load_balancer: None,
            json_config_path: None,
        };
        
        // 테스트 실행
        let result = builder.build_from_containers(&[container_info]).await;
        
        // 검증
        assert!(result.is_ok());
        let (host, path_matcher, service) = result.unwrap();
        assert_eq!(host, "test-host.com");
        assert_eq!(path_matcher.to_string(), "Exact(/)");
        
        // HTTP 포트 검증 - 9090으로 업데이트되었는지 확인
        let addr = service.get_next_address().unwrap();
        assert_eq!(addr.port(), 9090);
    }
} 