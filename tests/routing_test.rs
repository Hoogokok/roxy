use reverse_proxy_traefik::routing::{BackendService, HostInfo, PathMatcher, RoutingError, RoutingTable};
use std::net::SocketAddr;
use hyper::{Request, Method};
use http_body_util::Empty;
use hyper::body::Bytes;

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
fn test_routing_table_basic() {
    let mut table = RoutingTable::new();
    let backend = BackendService::new("127.0.0.1:8080".parse().unwrap());

    table.add_route("example.com".to_string(), backend, None);

    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: None,
    };
    assert!(table.find_backend(&host_info).is_ok());
}

#[test]
fn test_routing_table_multiple_hosts() {
    let mut table = RoutingTable::new();
    
    let backends = vec![
        ("example.com", "127.0.0.1:8080", None),
        ("test.com", "127.0.0.1:8081", None),
        ("api.example.com", "127.0.0.1:8082", Some(PathMatcher::from_str("/api").unwrap())),
    ];

    for (host, addr, path) in backends.clone() {
        table.add_route(
            host.to_string(),
            BackendService::new(addr.parse().unwrap()),
            path,
        );
    }

    // 각 호스트에 대한 라우팅 테스트
    for (host, addr, path) in backends {
        let host_info = HostInfo {
            name: host.to_string(),
            port: None,
            path: path.map(|p| p.pattern.clone()),
        };
        let backend = table.find_backend(&host_info).expect("Backend not found");
        assert_eq!(
            backend.get_next_address().unwrap().to_string(),
            addr
        );
    }
}

#[test]
fn test_routing_table_path_based() {
    let mut table = RoutingTable::new();
    
    // 동일한 호스트에 대해 다른 경로로 라우트 추가
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
        Some(PathMatcher::from_str("/api").unwrap()),
    );
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8081".parse().unwrap()),
        Some(PathMatcher::from_str("/web").unwrap()),
    );

    // API 경로 테스트
    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: Some("/api".to_string()),
    };
    let backend = table.find_backend(&host_info).unwrap();
    assert_eq!(
        backend.get_next_address().unwrap().to_string(),
        "127.0.0.1:8080"
    );

    // 웹 경로 테스트
    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: Some("/web".to_string()),
    };
    let backend = table.find_backend(&host_info).unwrap();
    assert_eq!(
        backend.get_next_address().unwrap().to_string(),
        "127.0.0.1:8081"
    );
}

#[test]
fn test_routing_table_load_balancing() {
    let mut table = RoutingTable::new();
    
    // 동일한 호스트와 경로에 대해 여러 백엔드 추가
    let backends = vec![
        "127.0.0.1:8080",
        "127.0.0.1:8081",
        "127.0.0.1:8082",
    ];

    for addr in backends.iter() {
        table.add_route(
            "example.com".to_string(),
            BackendService::new(addr.parse().unwrap()),
            Some(PathMatcher::from_str("/api").unwrap()),
        );
    }

    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: Some("/api".to_string()),
    };

    let backend = table.find_backend(&host_info).unwrap();
    
    // 라운드 로빈 검증
    let mut seen_addresses = std::collections::HashSet::new();
    for _ in 0..3 {
        let addr = backend.get_next_address().unwrap();
        seen_addresses.insert(addr);
    }
    assert_eq!(seen_addresses.len(), 3, "모든 백엔드가 순환되어야 함");
}

#[test]
fn test_routing_table_overwrite() {
    let mut table = RoutingTable::new();
    
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
        None,
    );

    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:9090".parse().unwrap()),
        None,
    );

    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: None,
    };
    let backend = table.find_backend(&host_info).unwrap();
    assert_eq!(backend.get_next_address().unwrap().to_string(), "127.0.0.1:8080");
    assert_eq!(backend.get_next_address().unwrap().to_string(), "127.0.0.1:9090");
}

fn setup_routing_table() -> RoutingTable {
    let mut table = RoutingTable::new();
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
        None,
    );
    table
}

fn create_request(host: Option<&str>) -> Request<Empty<Bytes>> {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri("http://example.com/some/path");

    if let Some(host_value) = host {
        builder = builder.header("Host", host_value);
    }

    builder.body(Empty::new()).unwrap()
}

#[test]
fn test_route_request_success() {
    let table = setup_routing_table();
    let req = create_request(Some("example.com"));

    let result = table.route_request(&req);
    assert!(result.is_ok());
    
    let backend = result.unwrap();
    assert_eq!(
        backend.get_next_address().unwrap(),
        "127.0.0.1:8080".parse::<SocketAddr>().unwrap()
    );
}

#[test]
fn test_route_request_missing_host() {
    let table = setup_routing_table();
    let req = create_request(None);

    let result = table.route_request(&req);
    assert!(matches!(result.unwrap_err(), RoutingError::MissingHost));
}

#[test]
fn test_route_request_unknown_host() {
    let table = setup_routing_table();
    let req = create_request(Some("unknown.com"));

    let result = table.route_request(&req);
    assert!(matches!(
        result.unwrap_err(),
        RoutingError::BackendNotFound { host, available_routes: _ }
        if host == "unknown.com"
    ));
}

#[test]
fn test_route_request_invalid_host() {
    let table = setup_routing_table();
    let req = create_request(Some("example.com:invalid_port"));

    let result = table.route_request(&req);
    assert!(matches!(
        result.unwrap_err(),
        RoutingError::InvalidPort { port, reason: _ }
        if port == "invalid_port"
    ));
}

#[test]
fn test_route_request_with_port() {
    let table = setup_routing_table();
    let req = create_request(Some("example.com:8080"));

    let result = table.route_request(&req);
    assert!(result.is_ok());
    
    let backend = result.unwrap();
    assert_eq!(
        backend.get_next_address().unwrap(),
        "127.0.0.1:8080".parse::<SocketAddr>().unwrap()
    );
}

#[test]
fn test_routing_table_remove_route() {
    let mut table = setup_routing_table();
    
    // 존재하는 라우트 제거
    table.remove_route("example.com");
    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: None,
    };
    assert!(matches!(
        table.find_backend(&host_info).unwrap_err(),
        RoutingError::BackendNotFound { host, available_routes }
        if host == "example.com" && available_routes.is_empty()
    ));

    // 존재하지 않는 라우트 제거
    table.remove_route("nonexistent.com");
}

// 새로운 테스트 추가: path 매칭 테스트
#[test]
fn test_routing_table_path_matching() {
    let mut table = RoutingTable::new();
    
    // PathPrefix 매칭 사용
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
        Some(PathMatcher::from_str("/api*").unwrap()),  // PathPrefix 사용
    );

    // 매칭되는 경로들 테스트
    let test_paths = vec![
        "/api",      // 기본 경로
        "/api/",     // trailing slash
        "/api/users" // 하위 경로
    ];

    for path in test_paths {
        let host_info = HostInfo {
            name: "example.com".to_string(),
            port: None,
            path: Some(path.to_string()),
        };
        assert!(
            table.find_backend(&host_info).is_ok(),
            "경로 매칭 실패: {}", 
            path
        );
    }

    // 매칭되지 않는 경로 테스트
    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: Some("/web/users".to_string()),
    };
    assert!(matches!(
        table.find_backend(&host_info).unwrap_err(),
        RoutingError::BackendNotFound { host, .. }
        if host == "example.com"
    ));
}

#[test]
fn test_routing_table_round_robin() {
    let mut table = RoutingTable::new();
    
    // 동일한 호스트에 대해 여러 백엔드 추가
    let host = "example.com".to_string();
    let default_matcher = PathMatcher::from_str("/").unwrap();
    let backends = vec![
        "127.0.0.1:8080",
        "127.0.0.1:8081",
        "127.0.0.1:8082",
    ];

    for addr in backends.iter() {
        table.add_route(
            host.clone(),
            BackendService::new(addr.parse().unwrap()),
            None,
        );
    }

    // 라운드 로빈 검증
    let backend = table.routes.get(&(host, default_matcher)).unwrap();
    assert_eq!(backend.addresses.len(), 3, "모든 백엔드 주소가 병합되어야 함");

    // 여러 번 요청해서 라운드 로빈 확인
    let mut seen_addresses = std::collections::HashSet::new();
    for _ in 0..3 {
        let addr = backend.get_next_address().unwrap();
        seen_addresses.insert(addr);
    }
    assert_eq!(seen_addresses.len(), 3, "모든 백엔드가 순환되어야 함");
}

#[test]
fn test_path_prefix_matching() {
    let mut table = RoutingTable::new();
    
    // API 서버 설정 - PathPrefix 사용
    table.add_route(
        "test.localhost".to_string(),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
        Some(PathMatcher::from_str("/api*").unwrap()),  // PathPrefix 표시를 위해 * 사용
    );

    // 다양한 경로 테스트
    let test_paths = vec![
        "/api",
        "/api/",
        "/api/users",
        "/api/users/123",
    ];

    for path in test_paths {
        let host_info = HostInfo {
            name: "test.localhost".to_string(),
            port: None,
            path: Some(path.to_string()),
        };
        assert!(table.find_backend(&host_info).is_ok(), "Failed to match path: {}", path);
    }
}

#[test]
fn test_path_matcher_creation() {
    let test_cases = vec![
        // (패턴, 매칭 성공 여부)
        ("/api", true),
        ("/api/*", true),
        ("^/api/.*", true),
        ("^[invalid", false),  // 잘못된 정규식
    ];

    for (pattern, should_succeed) in test_cases {
        let result = PathMatcher::from_str(pattern);
        assert_eq!(
            result.is_ok(), 
            should_succeed,
            "패턴: {}, 결과: {:?}", 
            pattern, 
            result
        );
    }
}

#[test]
fn test_path_matcher_matching() {
    let test_cases = vec![
        // (패턴, 경로, 예상 결과)
        // Path 매칭 (Exact)
        ("/api", "/api", true),
        ("/api", "/api/", false),     // trailing slash는 다른 경로
        ("/api", "/api/users", false), // 하위 경로는 매칭되지 않음
        
        // PathPrefix 매칭
        ("/api*", "/api", true),      // 기본 경로 매칭
        ("/api*", "/api/", true),     // trailing slash 매칭
        ("/api*", "/api/users", true), // 하위 경로 매칭
        ("/api*", "/apis", false),    // 다른 경로는 매칭되지 않음
        
        // PathRegexp 매칭
        ("^/api/.*", "/api/users", true),
        ("^/api/.*", "/api/", true),
        ("^/api/.*", "/web/api", false),
    ];

    for (pattern, path, expected) in test_cases {
        let matcher = PathMatcher::from_str(pattern).unwrap();
        assert_eq!(
            matcher.matches(path), 
            expected,
            "패턴: {}, 경로: {}", 
            pattern, 
            path
        );
    }
}