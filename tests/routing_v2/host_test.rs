use reverse_proxy_traefik::routing_v2::{HostInfo, RoutingError};

#[test]
fn test_host_info_parsing() {
    // 기본 케이스 테스트
    let test_cases = vec![
        // (입력, 예상 결과)
        ("example.com", Ok(("example.com", None))),
        ("example.com:8080", Ok(("example.com", Some(8080)))),
        ("example.com:80", Ok(("example.com", Some(80)))),
        // 잘못된 입력 테스트
        ("example.com:invalid", Err(RoutingError::InvalidPort { 
            port: "invalid".to_string(),
            reason: "Invalid format".to_string(),
        })),
        ("example.com:", Err(RoutingError::InvalidHost { 
            host: "example.com:".to_string(),
            reason: "Invalid format".to_string(),
        })),
        (":8080", Err(RoutingError::InvalidHost { 
            host: ":8080".to_string(),
            reason: "Invalid format".to_string(),
        })),
        ("", Err(RoutingError::InvalidHost { 
            host: "".to_string(),
            reason: "Invalid format".to_string(),
        })),
        ("example.com:65536", Err(RoutingError::InvalidPort { 
            port: "65536".to_string(),
            reason: "Invalid format".to_string(),
        })),
        ("example.com:0", Err(RoutingError::InvalidPort { 
            port: "0".to_string(),
            reason: "Port must be greater than 0".to_string(),
        })),
        ("example.com:8080:extra", Err(RoutingError::InvalidHost { 
            host: "example.com:8080:extra".to_string(),
            reason: "Invalid format".to_string(),
        })),
    ];

    for (input, expected) in test_cases {
        let result = HostInfo::from_header_value(input);
        match expected {
            Ok((name, port)) => {
                let info = result.expect(&format!("Failed to parse valid host: {}", input));
                assert_eq!(info.name, name);
                assert_eq!(info.port, port);
            }
            Err(expected_err) => {
                let err = result.expect_err(&format!("Should fail to parse invalid host: {}", input));
                assert!(matches!(err, expected_err));
            }
        }
    }
}

#[test]
fn test_host_info_with_path() {
    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: None,
    };

    let with_path = host_info.with_path("/api/users".to_string());
    assert_eq!(with_path.path, Some("/api/users".to_string()));
}

#[test]
fn test_host_info_clone() {
    let original = HostInfo {
        name: "example.com".to_string(),
        port: Some(8080),
        path: Some("/api".to_string()),
    };

    let cloned = original.clone();
    assert_eq!(cloned.name, original.name);
    assert_eq!(cloned.port, original.port);
    assert_eq!(cloned.path, original.path);
} 