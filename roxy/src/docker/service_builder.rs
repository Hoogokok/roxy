use std::sync::Arc;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;

use crate::docker::DockerError;
use crate::docker::ContainerInfo;
use crate::docker::ContainerInfoExtractor;
use crate::docker::DockerClient;
use crate::routing_v2::{BackendService, PathMatcher, LoadBalancerStrategy};
use crate::settings::container::ContainerConfigManager;
use crate::settings::core::AnyLoadBalancerSettings;
use crate::settings::core::Settings;
use crate::settings::typestate::Validated;
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
    /// 
    /// 미리 병합된 설정을 전달하여 성능을 최적화할 수 있습니다.
    pub async fn build_from_containers(
        &self, 
        infos: &[ContainerInfo],
        merged_configs: Option<&HashMap<String, Settings<Validated>>>
    ) -> Result<(String, PathMatcher, BackendService), DockerError> {
        debug!("백엔드 서비스 생성 시작 (병합된 설정 사용: {})",
            if merged_configs.is_some() { "예" } else { "아니오" });
        
        // 1. 입력 검증
        self.validate_container_infos(infos)?;
        
        // 2. 첫 번째 컨테이너에서 기본 정보 추출
        let first = &infos[0];
        let (host, path_matcher) = self.extract_service_info(first);
        
        // 3. 기본 백엔드 서비스 생성
        let mut service = self.extractor.create_backend(first)?;
        
        // 4. JSON 설정 적용 (미리 병합된 설정 활용)
        self.apply_json_settings(first, &mut service, merged_configs).await?;
        
        // 5. 로드밸런서 설정 (미리 병합된 설정 전달)
        if infos.len() > 1 {
            self.configure_load_balancer(infos, &mut service, merged_configs).await?;
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
    async fn apply_json_settings(
        &self, 
        container: &ContainerInfo, 
        service: &mut BackendService,
        merged_configs: Option<&HashMap<String, Settings<Validated>>>
    ) -> Result<(), DockerError> {
        if let Some(container_id) = &container.container_id {
            // 1. 미리 병합된 설정이 있으면 사용
            if let Some(configs) = merged_configs {
                if let Some(settings) = configs.get(container_id) {
                    debug!(
                        container_id = %container_id,
                        "미리 병합된 설정 사용"
                    );
                    
                    // HTTP 포트 적용
                    let http_port = settings.server.http_port.value();
                    if http_port != container.port {
                        debug!(
                            container_id = %container_id,
                            original_port = %container.port,
                            new_port = %http_port,
                            "미리 병합된 설정에서 HTTP 포트 업데이트"
                        );
                        service.update_port(http_port);
                    }
                    return Ok(());
                }
            }
            
            // 2. 미리 병합된 설정이 없으면 기존 방식으로 계속 진행
            let labels = self.get_container_labels(container_id).await?;
            
            // 2.1. 라벨에서 직접 포트 추출
            let router_name = container.router_name.as_deref().unwrap_or("test");
            let label_prefix = self.extractor.get_label_prefix();
            let port_label_key = format!("{}http.services.{}.loadbalancer.server.port", label_prefix, router_name);
            debug!("라벨 키 생성: {}", port_label_key);
            
            if let Some(port_str) = labels.get(&port_label_key) {
                if let Ok(port) = port_str.parse::<u16>() {
                    if port != container.port {
                        service.update_port(port);
                        return Ok(());
                    }
                }
            }
            
            // 2.2. JSON 설정과 라벨 병합 (기존 코드)
            let settings = self.container_config_manager.merge_config(container_id, &labels);
            
            // HTTP 포트 적용
            let http_port = settings.server.http_port.value();
            
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
                );
            }
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
    async fn configure_load_balancer(
        &self, 
        infos: &[ContainerInfo], 
        service: &mut BackendService,
        merged_configs: Option<&HashMap<String, Settings<Validated>>>
    ) -> Result<(), DockerError> {
        debug!("로드밸런서 설정: {} 컨테이너", infos.len());
        
        // 로드밸런서 활성화
        let strategy = infos[0].load_balancer.clone()
            .unwrap_or(LoadBalancerStrategy::RoundRobin {
                current_index: AtomicUsize::new(0),
            });
        
        service.enable_load_balancer(strategy);
        
        // 로드밸런서의 addresses 필드에 첫번째 주소와 가중치도 추가해야 함
        let first_weight = self.get_weight_from_config(infos[0].container_id.as_deref(), merged_configs).unwrap_or(1);
        if let Some(lb) = &mut service.load_balancer {
             // enable_load_balancer에서 이미 첫 번째 주소가 (기본 가중치 1로) 추가되었으므로,
             // 첫 번째 주소의 가중치를 업데이트합니다.
             if let Some(first_addr_entry) = lb.addresses.first_mut() {
                 first_addr_entry.1 = first_weight;
             }
             // 가중치 전략인 경우 total_weight 업데이트
             if let LoadBalancerStrategy::Weighted { total_weight, .. } = &mut lb.strategy {
                *total_weight = first_weight; // 첫 번째 주소의 가중치로 초기화
             }
        }


        // 추가 컨테이너 처리
        for info in &infos[1..] {
            // 기본 IP:PORT 주소
            let addr = self.extractor.parse_socket_addr(&info.ip, info.port)?;

            // *** 가중치 가져오기 로직 수정 ***
            let weight = self.get_weight_from_config(info.container_id.as_deref(), merged_configs).unwrap_or(1);
            debug!("컨테이너 {} 가중치: {}", info.container_id.as_deref().unwrap_or("unknown"), weight);

            // 가져온 가중치로 주소 추가
            service.add_address(addr, weight)?; 
            
            // 추가 컨테이너 JSON 설정 적용 (병합된 설정 전달)
            // update_backend_port는 포트만 업데이트하므로 가중치 관련 수정은 불필요
            self.update_backend_port(info, service, merged_configs).await?;
        }
        
        Ok(())
    }
    
    // *** 가중치 추출 헬퍼 함수 추가 ***
    fn get_weight_from_config(
        &self,
        container_id: Option<&str>,
        merged_configs: Option<&HashMap<String, Settings<Validated>>>
    ) -> Option<usize> {
        container_id.and_then(|id| {
            merged_configs.and_then(|configs| {
                configs.get(id).and_then(|settings| {
                    match &settings.load_balancer {
                        AnyLoadBalancerSettings::Weighted(weighted_settings) => {
                            Some(weighted_settings.weight()) // WeightedStrategy의 weight() 메서드 사용
                        }
                        _ => None, // 다른 전략이거나 설정이 없으면 None 반환
                    }
                })
            })
        })
    }
    
    /// 백엔드 포트를 업데이트합니다.
    async fn update_backend_port(
        &self, 
        info: &ContainerInfo, 
        service: &mut BackendService,
        merged_configs: Option<&HashMap<String, Settings<Validated>>>
    ) -> Result<(), DockerError> {
        if let Some(container_id) = &info.container_id {
            // 미리 병합된 설정 있으면 사용
            let http_port = if let Some(configs) = merged_configs {
                if let Some(settings) = configs.get(container_id) {
                    settings.server.http_port.value()
                } else {
                    // 병합된 설정에 없으면 기존 방식으로 처리
                    let labels = self.get_container_labels(container_id).await?;
                    let settings = self.container_config_manager.merge_config(container_id, &labels);
                    settings.server.http_port.value()
                }
            } else {
                // 병합된 설정 없으면 기존 방식으로 처리
                let labels = self.get_container_labels(container_id).await?;
                let settings = self.container_config_manager.merge_config(container_id, &labels);
                settings.server.http_port.value()
            };
            
            // HTTP 포트가 설정되어 있으면 마지막 추가된 주소의 포트 업데이트
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
    use crate::settings::load_balancer::{LoadBalancerSettings, WeightedStrategy};
    use crate::settings::types::ValidPort;
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
            labels.insert("roxy.enable".to_string(), "true".to_string());
            labels.insert("roxy.http.routers.test.rule".to_string(), "Host(`test-host.com`)".to_string());
            labels.insert("roxy.http.services.test.loadbalancer.server.port".to_string(), "9090".to_string());
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
            "roxy.".to_string(),
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
        let result = builder.build_from_containers(&[container_info], None).await;
        
        // 검증
        assert!(result.is_ok());
        let (host, path_matcher, service) = result.unwrap();
        assert_eq!(host, "test-host.com");
        assert_eq!(path_matcher.to_string(), "Exact(/)");
        
        // HTTP 포트 검증 - 9090으로 업데이트되었는지 확인
        let addr = service.get_next_address().unwrap();
        assert_eq!(addr.port(), 9090);
    }
    
    #[tokio::test]
    async fn test_build_from_containers_with_merged_configs() {
        // 테스트 준비
        let extractor = Box::new(DefaultExtractor::new(
            "bridge".to_string(),
            "roxy.".to_string(),
        ));
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn DockerClient>);
        let (container_config_manager, _) = ContainerConfigManager::new();
        
        // 테스트용 컨테이너 ID
        let container_id = "test-container";
        
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
        
        // 미리 병합된 설정 준비 - 포트 8888 설정
        let mut merged_configs = HashMap::new();
        let mut settings = Settings::default();
        settings.server.http_port = ValidPort::new(8888).unwrap();
        merged_configs.insert(container_id.to_string(), settings);
        
        // 테스트 실행 (미리 병합된 설정 전달)
        let result = builder.build_from_containers(&[container_info], Some(&merged_configs)).await;
        
        // 검증
        assert!(result.is_ok());
        let (host, path_matcher, service) = result.unwrap();
        assert_eq!(host, "test-host.com");
        assert_eq!(path_matcher.to_string(), "Exact(/)");
        
        // HTTP 포트 검증 - 미리 병합된 설정의 8888 포트가 적용되었는지 확인
        let addr = service.get_next_address().unwrap();
        assert_eq!(addr.port(), 8888);
    }
    
    #[tokio::test]
    async fn test_build_from_containers_with_load_balancer() {
        // 테스트 준비
        let extractor = Box::new(DefaultExtractor::new(
            "bridge".to_string(),
            "roxy.".to_string(),
        ));
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn DockerClient>);
        let (container_config_manager, _) = ContainerConfigManager::new();
        
        let builder = BackendServiceBuilder::new(
            extractor,
            client,
            Arc::new(container_config_manager),
        );
        
        // 컨테이너 1
        let container1 = ContainerInfo {
            host: "test-host.com".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 80,
            container_id: Some("container1".to_string()),
            path_matcher: None,
            middlewares: None,
            router_name: Some("test".to_string()),
            health_check: None,
            load_balancer: Some(LoadBalancerStrategy::RoundRobin {
                current_index: AtomicUsize::new(0),
            }),
            json_config_path: None,
        };
        
        // 컨테이너 2
        let container2 = ContainerInfo {
            host: "test-host.com".to_string(),
            ip: "127.0.0.2".to_string(),
            port: 80,
            container_id: Some("container2".to_string()),
            path_matcher: None,
            middlewares: None,
            router_name: Some("test".to_string()),
            health_check: None,
            load_balancer: None,
            json_config_path: None,
        };
        
        // 미리 병합된 설정 준비 - 포트 8888과 9999 설정
        let mut merged_configs = HashMap::new();
        
        // 컨테이너 1의 설정
        let mut settings1 = Settings::default();
        settings1.server.http_port = ValidPort::new(8888).unwrap();
        merged_configs.insert("container1".to_string(), settings1);
        
        // 컨테이너 2의 설정
        let mut settings2 = Settings::default();
        settings2.server.http_port = ValidPort::new(9999).unwrap();
        merged_configs.insert("container2".to_string(), settings2);
        
        // 테스트 실행 (미리 병합된 설정 전달)
        let result = builder.build_from_containers(
            &[container1, container2], 
            Some(&merged_configs)
        ).await;
        
        // 검증
        assert!(result.is_ok());
        let (host, path_matcher, service) = result.unwrap();
        assert_eq!(host, "test-host.com");
        assert_eq!(path_matcher.to_string(), "Exact(/)");
        
        // 로드밸런서 설정 확인
        assert!(service.load_balancer.is_some());
        
        // 첫 번째 주소 검증 (컨테이너 1, 포트 8888)
        let addr1 = service.get_next_address().unwrap();
        assert_eq!(addr1.port(), 8888);
        assert_eq!(addr1.ip().to_string(), "127.0.0.1");
        
        // 두 번째 주소 검증 (컨테이너 2, 포트 9999)
        let addr2 = service.get_next_address().unwrap();
        assert_eq!(addr2.port(), 9999);
        assert_eq!(addr2.ip().to_string(), "127.0.0.2");
    }

    #[tokio::test]
    async fn test_build_from_containers_with_weighted_load_balancer() {
        // 테스트 준비
        let extractor = Box::new(DefaultExtractor::new(
            "bridge".to_string(),
            "roxy.".to_string(),
        ));
        let client = Arc::new(Box::new(MockDockerClient) as Box<dyn DockerClient>);
        let (container_config_manager, _) = ContainerConfigManager::new();
        
        let builder = BackendServiceBuilder::new(
            extractor,
            client,
            Arc::new(container_config_manager),
        );
        
        // 컨테이너 1 (가중치 5)
        let container1 = ContainerInfo {
            host: "weighted-host.com".to_string(),
            ip: "127.0.0.1".to_string(),
            port: 80,
            container_id: Some("container-w1".to_string()),
            path_matcher: None,
            middlewares: None,
            router_name: Some("weighted-service".to_string()),
            health_check: None,
             // 이 필드는 BackendService 생성 시점에 사용됨
            load_balancer: Some(LoadBalancerStrategy::Weighted { 
                current_index: AtomicUsize::new(0),
                total_weight: 0 // 빌더가 계산하므로 초기값은 중요하지 않음
            }),
            json_config_path: None,
        };
        
        // 컨테이너 2 (가중치 10)
        let container2 = ContainerInfo {
            host: "weighted-host.com".to_string(),
            ip: "127.0.0.2".to_string(),
            port: 80,
            container_id: Some("container-w2".to_string()),
            path_matcher: None,
            middlewares: None,
            router_name: Some("weighted-service".to_string()),
            health_check: None,
            load_balancer: None, // 첫 번째 컨테이너 정보만 사용됨
            json_config_path: None,
        };
        
        // 미리 병합된 설정 준비 - 가중치 설정
        let mut merged_configs = HashMap::new();
        
        // 컨테이너 1의 설정 (가중치 5)
        let mut settings1 = Settings::<Validated>::default(); // 기본 HttpsDisabled 사용
        settings1.load_balancer = AnyLoadBalancerSettings::Weighted(
            LoadBalancerSettings::<Validated, WeightedStrategy>::new(5)
        );
        merged_configs.insert("container-w1".to_string(), settings1);
        
        // 컨테이너 2의 설정 (가중치 10)
        let mut settings2 = Settings::<Validated>::default(); // 기본 HttpsDisabled 사용
        settings2.load_balancer = AnyLoadBalancerSettings::Weighted(
            LoadBalancerSettings::<Validated, WeightedStrategy>::new(10)
        );
        merged_configs.insert("container-w2".to_string(), settings2);
        
        // 테스트 실행 (미리 병합된 설정 전달)
        let result = builder.build_from_containers(
            &[container1, container2], 
            Some(&merged_configs)
        ).await;
        
        // 검증
        assert!(result.is_ok(), "Build failed: {:?}", result.err());
        let (host, _, service) = result.unwrap();
        assert_eq!(host, "weighted-host.com");
        
        // 로드밸런서 및 가중치 확인
        assert!(service.load_balancer.is_some(), "Load balancer should be enabled");
        let lb = service.load_balancer.unwrap();
        
        // BackendService는 addresses 벡터에 (SocketAddr, usize) 형태로 저장함
        assert_eq!(lb.addresses.len(), 2, "Should have 2 backend addresses");
        
        // 각 주소의 가중치 확인 (순서는 중요하지 않을 수 있음)
        let weight1 = lb.addresses.iter().find(|(addr, _)| addr.ip().to_string() == "127.0.0.1").map(|(_, w)| *w);
        let weight2 = lb.addresses.iter().find(|(addr, _)| addr.ip().to_string() == "127.0.0.2").map(|(_, w)| *w);
        
        // 이 단언문은 현재 구현에서 실패할 것임 (항상 1로 추가되기 때문)
        assert_eq!(weight1, Some(5), "Weight for container-w1 should be 5"); 
        assert_eq!(weight2, Some(10), "Weight for container-w2 should be 10"); 
        
        // total_weight 확인 (구현 수정 후 확인)
        // assert_eq!(lb.get_total_weight(), Some(15), "Total weight should be 15");
    }
} 