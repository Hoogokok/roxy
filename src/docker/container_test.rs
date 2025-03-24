#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use bollard::models::{ContainerSummary, ContainerSummaryNetworkSettings, EndpointSettings};
    use crate::docker::container::{ContainerInfo, DefaultExtractor, ContainerInfoExtractor};
    use crate::settings::types::ConfigPath;
    use crate::settings::typestate::{Raw, Validatable, Validated};

    // ConfigPath 헬퍼 함수 (테스트용)
    fn create_validated_path(path: &str) -> ConfigPath<Validated> {
        ConfigPath::<Raw>::new(path.to_string())
            .validate()
            .expect("유효한 경로여야 함")
    }
    
    // 테스트용 컨테이너 생성 함수
    fn create_test_container_with_labels(labels: HashMap<String, String>) -> ContainerSummary {
        let mut container = ContainerSummary::default();
        container.id = Some("test-container".to_string());
        
        container.network_settings = Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                "bridge".to_string(),
                EndpointSettings {
                    ip_address: Some("192.168.1.2".to_string()),
                    ..Default::default()
                }
            )])),
            ..Default::default()
        });
        
        container.labels = Some(labels);
        container
    }

    // ContainerInfo에 json_config_path 필드가 추가되었는지 확인하는 테스트
    #[test]
    fn test_container_info_has_json_config_field() {
        let validated_path = create_validated_path("/path/to/config.json");
        
        let container_info = ContainerInfo {
            host: "example.com".to_string(),
            ip: "172.0.0.1".to_string(),
            port: 80,
            container_id: Some("test-container".to_string()),
            path_matcher: None,
            middlewares: None,
            router_name: None,
            health_check: None,
            load_balancer: None,
            json_config_path: Some(validated_path),
        };

        assert_eq!(container_info.json_config_path.as_ref().map(|p| p.as_str()), 
                  Some("/path/to/config.json"));
    }

    // DefaultExtractor가 ContainerInfo에 JSON 설정 경로를 추출하여 포함하는지 확인
    #[test]
    fn test_extract_info_includes_json_config_path() {
        let extractor = DefaultExtractor::new("bridge".to_string(), "rproxy.".to_string());
        
        // 테스트용 컨테이너 생성
        let mut container = ContainerSummary::default();
        container.id = Some("test-container".to_string());
        
        // 기본 네트워크 설정 추가
        container.network_settings = Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                "bridge".to_string(),
                EndpointSettings {
                    ip_address: Some("192.168.1.2".to_string()),
                    ..Default::default()
                }
            )])),
            ..Default::default()
        });
        
        // 필수 라벨 추가
        let mut labels = HashMap::new();
        labels.insert("rproxy.host".to_string(), "example.com".to_string());
        labels.insert("rproxy.config.json".to_string(), "/config/service.json".to_string());
        container.labels = Some(labels);
        
        // ContainerInfo 추출 (ContainerInfoExtractor 트레이트 메서드 호출)
        let result = extractor.extract_info(&container);
        assert!(result.is_ok());
        
        let info = result.unwrap();
        assert_eq!(info.json_config_path.as_ref().map(|p| p.as_str()), Some("/config/service.json"));
    }

    // 설정 파일 경로가 제공되지 않은 경우 None으로 설정되는지 확인
    #[test]
    fn test_extract_info_without_json_config_path() {
        let extractor = DefaultExtractor::new("bridge".to_string(), "rproxy.".to_string());
        
        // 테스트용 컨테이너 생성
        let mut container = ContainerSummary::default();
        container.id = Some("test-container".to_string());
        
        // 기본 네트워크 설정 추가
        container.network_settings = Some(ContainerSummaryNetworkSettings {
            networks: Some(HashMap::from([(
                "bridge".to_string(),
                EndpointSettings {
                    ip_address: Some("192.168.1.2".to_string()),
                    ..Default::default()
                }
            )])),
            ..Default::default()
        });
        
        // JSON 설정 경로가 없는 라벨 추가
        let mut labels = HashMap::new();
        labels.insert("rproxy.host".to_string(), "example.com".to_string());
        container.labels = Some(labels);
        
        // ContainerInfo 추출
        let result = extractor.extract_info(&container);
        assert!(result.is_ok());
        
        let info = result.unwrap();
        assert!(info.json_config_path.is_none());
    }

    #[test]
    fn test_extract_info_with_invalid_path() {
        let extractor = DefaultExtractor::new("bridge".to_string(), "rproxy.".to_string());
        
        let mut labels = HashMap::new();
        labels.insert("rproxy.host".to_string(), "example.com".to_string());
        labels.insert("rproxy.config.json".to_string(), "invalid/path.json".to_string());
        
        let container = create_test_container_with_labels(labels);
        let result = extractor.extract_info(&container);
        
        assert!(result.is_err());
    }
} 