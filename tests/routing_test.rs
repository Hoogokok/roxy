use reverse_proxy_traefik::routing::{HostInfo, RoutingTable, BackendService, RoutingError};
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
        ("example.com", "127.0.0.1:8080"),
        ("test.com", "127.0.0.1:8081"),
        ("api.example.com", "127.0.0.1:8082"),
    ];

    for (host, addr) in backends.clone() {
        table.add_route(
            host.to_string(),
            BackendService::new(addr.parse().unwrap()),
            None,
        );
    }

    // 각 호스트에 대한 라우팅 테스트
    for (host, addr) in backends {
        let host_info = HostInfo {
            name: host.to_string(),
            port: None,
            path: None,
        };
        let backend = table.find_backend(&host_info).expect("Backend not found");
        assert_eq!(
            backend.get_next_address().unwrap().to_string(),
            addr
        );
    }

    // 존재하지 않는 호스트 테스트
    let unknown_host = HostInfo {
        name: "unknown.com".to_string(),
        port: None,
        path: None,
    };
    assert!(matches!(
        table.find_backend(&unknown_host).unwrap_err(),
        RoutingError::BackendNotFound { host, available_routes: _ } 
        if host == "unknown.com"
    ));
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
    
    // /api 경로에 대한 라우트 추가
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
        Some("/api".to_string()),
    );

    // 매칭되는 경로 테스트
    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: Some("/api/users".to_string()),
    };
    assert!(table.find_backend(&host_info).is_ok());

    // 매칭되지 않는 경로 테스트
    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
        path: Some("/web/users".to_string()),
    };
    assert!(matches!(
        table.find_backend(&host_info).unwrap_err(),
        RoutingError::BackendNotFound { host, available_routes: _ }
        if host == "example.com"
    ));
} 