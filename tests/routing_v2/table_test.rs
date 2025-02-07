use reverse_proxy_traefik::routing_v2::{RoutingTable, BackendService, HostInfo, PathMatcher, RoutingError};
use std::net::SocketAddr;
use hyper::{Request, Method};
use http_body_util::Empty;
use hyper::body::Bytes;

// 테스트 헬퍼 함수
fn create_request(host: Option<&str>, path: &str) -> Request<Empty<Bytes>> {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri(format!("http://example.com{}", path));

    if let Some(host_value) = host {
        builder = builder.header("Host", host_value);
    }

    builder.body(Empty::new()).unwrap()
}

#[test]
fn test_routing_table_basic() {
    let mut table = RoutingTable::new();
    let backend = BackendService::new("127.0.0.1:8080".parse().unwrap());

    table.add_route("example.com".to_string(), backend, None);

    let req = create_request(Some("example.com"), "/");
    assert!(table.route_request(&req).is_ok());
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

    for (host, addr, path) in backends {
        let path_str = path.map_or("/".to_string(), |p| p.pattern);
        let req = create_request(Some(host), &path_str);
        let backend = table.route_request(&req).expect("Backend not found");
        assert_eq!(
            backend.get_next_address().unwrap().to_string(),
            addr
        );
    }
}

#[test]
fn test_routing_table_path_based() {
    let mut table = RoutingTable::new();
    
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
        Some(PathMatcher::from_str("/api/*").unwrap()),
    );
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8081".parse().unwrap()),
        Some(PathMatcher::from_str("/web/*").unwrap()),
    );

    // API 경로 테스트
    let req = create_request(Some("example.com"), "/api/users");
    let backend = table.route_request(&req).unwrap();
    assert_eq!(
        backend.get_next_address().unwrap().to_string(),
        "127.0.0.1:8080"
    );

    // 웹 경로 테스트
    let req = create_request(Some("example.com"), "/web/index.html");
    let backend = table.route_request(&req).unwrap();
    assert_eq!(
        backend.get_next_address().unwrap().to_string(),
        "127.0.0.1:8081"
    );
}

#[test]
fn test_routing_table_load_balancing() {
    let mut table = RoutingTable::new();
    
    let backends = vec![
        "127.0.0.1:8080",
        "127.0.0.1:8081",
        "127.0.0.1:8082",
    ];

    for addr in backends.iter() {
        table.add_route(
            "example.com".to_string(),
            BackendService::new(addr.parse().unwrap()),
            Some(PathMatcher::from_str("/api/*").unwrap()),
        );
    }

    let req = create_request(Some("example.com"), "/api/users");
    let backend = table.route_request(&req).unwrap();
    
    let mut seen_addresses = std::collections::HashSet::new();
    for _ in 0..3 {
        let addr = backend.get_next_address().unwrap();
        seen_addresses.insert(addr);
    }
    assert_eq!(seen_addresses.len(), 3, "모든 백엔드가 순환되어야 함");
}

#[test]
fn test_routing_table_error_cases() {
    let table = RoutingTable::new();

    // 호스트 헤더 누락
    let req = create_request(None, "/");
    assert!(matches!(
        table.route_request(&req).unwrap_err(),
        RoutingError::MissingHost
    ));

    // 알 수 없는 호스트
    let req = create_request(Some("unknown.com"), "/");
    assert!(matches!(
        table.route_request(&req).unwrap_err(),
        RoutingError::BackendNotFound { host, .. }
        if host == "unknown.com"
    ));

    // 잘못된 호스트 형식
    let req = create_request(Some("example.com:invalid"), "/");
    assert!(matches!(
        table.route_request(&req).unwrap_err(),
        RoutingError::InvalidPort { port, .. }
        if port == "invalid"
    ));
}

#[test]
fn test_routing_table_remove_route() {
    let mut table = RoutingTable::new();
    
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
        None,
    );

    // 라우트 제거 전 테스트
    let req = create_request(Some("example.com"), "/");
    assert!(table.route_request(&req).is_ok());

    // 라우트 제거
    table.remove_route("example.com");

    // 라우트 제거 후 테스트
    let req = create_request(Some("example.com"), "/");
    assert!(matches!(
        table.route_request(&req).unwrap_err(),
        RoutingError::BackendNotFound { host, .. }
        if host == "example.com"
    ));
}

#[test]
fn test_routing_table_sync_docker_routes() {
    let mut table = RoutingTable::new();
    let mut new_routes = std::collections::HashMap::new();
    
    new_routes.insert(
        ("example.com".to_string(), PathMatcher::from_str("/").unwrap()),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
    );

    table.sync_docker_routes(new_routes);

    let req = create_request(Some("example.com"), "/");
    assert!(table.route_request(&req).is_ok());
} 