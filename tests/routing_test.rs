use reverse_proxy_traefik::routing::{HostInfo, RoutingTable, BackendService, RoutingError};

#[test]
fn test_host_info_parsing() {
    // 기본 케이스 테스트
    let test_cases = vec![
        // (입력, 예상 결과)
        ("example.com", Ok(("example.com", None))),
        ("example.com:8080", Ok(("example.com", Some(8080)))),
        ("example.com:80", Ok(("example.com", Some(80)))),
        // 잘못된 입력 테스트
        ("example.com:invalid", Err(RoutingError::InvalidPort("invalid".to_string()))),
        ("example.com:", Err(RoutingError::InvalidHost("example.com:".to_string()))),
        (":8080", Err(RoutingError::InvalidHost(":8080".to_string()))),
        ("", Err(RoutingError::InvalidHost("".to_string()))),
        ("example.com:65536", Err(RoutingError::InvalidPort("65536".to_string()))),
        ("example.com:0", Err(RoutingError::InvalidPort("0".to_string()))),
        ("example.com:8080:extra", Err(RoutingError::InvalidHost("example.com:8080:extra".to_string()))),
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
fn test_routing_table_basic() {
    let mut table = RoutingTable::new();
    let backend = BackendService {
        address: "127.0.0.1:8080".parse().unwrap(),
    };

    // 기본 라우팅 테스트
    table.add_route("example.com".to_string(), backend);

    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
    };
    assert!(table.find_backend(&host_info).is_ok());
}

#[test]
fn test_routing_table_multiple_hosts() {
    let mut table = RoutingTable::new();
    
    // 여러 백엔드 서비스 추가
    let backends = vec![
        ("example.com", "127.0.0.1:8080"),
        ("test.com", "127.0.0.1:8081"),
        ("api.example.com", "127.0.0.1:8082"),
    ];

    for (host, addr) in backends.clone() {
        table.add_route(
            host.to_string(),
            BackendService {
                address: addr.parse().unwrap(),
            },
        );
    }

    // 각 호스트에 대한 라우팅 테스트
    for (host, addr) in backends {
        let host_info = HostInfo {
            name: host.to_string(),
            port: None,
        };
        let backend = table.find_backend(&host_info).expect("Backend not found");
        assert_eq!(backend.address.to_string(), addr);
    }

    // 존재하지 않는 호스트 테스트
    let unknown_host = HostInfo {
        name: "unknown.com".to_string(),
        port: None,
    };
    assert!(matches!(
        table.find_backend(&unknown_host).unwrap_err(),
        RoutingError::BackendNotFound(_)
    ));
}

#[test]
fn test_routing_table_overwrite() {
    let mut table = RoutingTable::new();
    
    // 첫 번째 백엔드 추가
    table.add_route(
        "example.com".to_string(),
        BackendService {
            address: "127.0.0.1:8080".parse().unwrap(),
        },
    );

    // 같은 호스트에 대해 다른 백엔드 추가
    table.add_route(
        "example.com".to_string(),
        BackendService {
            address: "127.0.0.1:9090".parse().unwrap(),
        },
    );

    // 최신 백엔드로 업데이트되었는지 확인
    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
    };
    let backend = table.find_backend(&host_info).unwrap();
    assert_eq!(backend.address.to_string(), "127.0.0.1:9090");
} 