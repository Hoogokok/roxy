#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use bollard::models::{ContainerSummary, ContainerSummaryNetworkSettings, EndpointSettings};
    use crate::docker::container::{ContainerInfo, DefaultExtractor, ContainerInfoExtractor};

    // ContainerInfo에 json_config_path 필드가 추가되었는지 확인하는 테스트
    #[test]
    fn test_container_info_has_json_config_field() {
        // 이 테스트는 컴파일 시 ContainerInfo에 json_config_path 필드가 있는지 확인합니다
        let container_info = ContainerInfo {
            host: "example.com".to_string(),
            ip: "172.0.0.1".to_string(),
            port: 80,
            path_matcher: None,
            middlewares: None,
            router_name: None,
            health_check: None,
            load_balancer: None,
            json_config_path: Some("/path/to/config.json".to_string()),
        };

        assert_eq!(container_info.json_config_path, Some("/path/to/config.json".to_string()));
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
        assert_eq!(info.json_config_path, Some("/config/service.json".to_string()));
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
        assert_eq!(info.json_config_path, None);
    }
} 