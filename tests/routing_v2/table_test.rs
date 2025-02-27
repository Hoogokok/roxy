use reverse_proxy_traefik::routing_v2::{RoutingTable, BackendService, HostInfo, PathMatcher, RoutingError};
use std::net::SocketAddr;
use hyper::{Request, Method};
use http_body_util::Empty;
use hyper::body::Bytes;
use std::sync::atomic::AtomicUsize;
use reverse_proxy_traefik::routing_v2::backend::LoadBalancerStrategy;

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

#[test]
fn test_routing_table_round_robin() {
    let mut table = RoutingTable::new();
    let addr1 = "127.0.0.1:8080".parse().unwrap();
    
    // 첫 번째 백엔드 추가
    let mut service = BackendService::new(addr1);
    service.enable_load_balancer(LoadBalancerStrategy::RoundRobin {
        current_index: AtomicUsize::new(0)
    });
    
    // 추가 백엔드 주소들
    let addr2 = "127.0.0.1:8081".parse().unwrap();
    let addr3 = "127.0.0.1:8082".parse().unwrap();
    service.add_address(addr2, 1).unwrap();
    service.add_address(addr3, 1).unwrap();
    
    table.add_route("example.com".to_string(), service, None);

    let req = create_request(Some("example.com"), "/");
    let backend = table.route_request(&req).unwrap();
    
    // 라운드 로빈 순서 확인
    assert_eq!(backend.get_next_address().unwrap(), addr1);
    assert_eq!(backend.get_next_address().unwrap(), addr2);
    assert_eq!(backend.get_next_address().unwrap(), addr3);
    assert_eq!(backend.get_next_address().unwrap(), addr1);
}

#[test]
fn test_routing_table_weighted() {
    let mut table = RoutingTable::new();
    let addr1 = "127.0.0.1:8080".parse().unwrap();
    
    // 가중치 기반 백엔드 설정
    let mut service = BackendService::new(addr1);
    service.enable_load_balancer(LoadBalancerStrategy::Weighted {
        current_index: AtomicUsize::new(0),
        total_weight: 1,
    });
    
    // 2배 가중치를 가진 두 번째 백엔드 추가
    let addr2 = "127.0.0.1:8081".parse().unwrap();
    service.add_address(addr2, 2).unwrap();
    
    table.add_route("example.com".to_string(), service, None);

    let req = create_request(Some("example.com"), "/");
    let backend = table.route_request(&req).unwrap();
    
    // 가중치에 따른 분배 확인
    let mut addr1_count = 0;
    let mut addr2_count = 0;
    
    for _ in 0..30 {
        let addr = backend.get_next_address().unwrap();
        match addr {
            a if a == addr1 => addr1_count += 1,
            a if a == addr2 => addr2_count += 1,
            _ => panic!("Unexpected address"),
        }
    }
    
    // addr1:addr2 = 1:2 비율 확인
    assert!(addr1_count > 7 && addr1_count < 13); // ~10
    assert!(addr2_count > 17 && addr2_count < 23); // ~20
}

#[test]
fn test_routing_table_load_balancer_activation() {
    let mut table = RoutingTable::new();
    let addr1 = "127.0.0.1:8080".parse().unwrap();
    let addr2 = "127.0.0.1:8081".parse().unwrap();
    
    // 로드밸런서 없이 시작
    let service = BackendService::new(addr1);
    table.add_route("example.com".to_string(), service, None);
    
    // 두 번째 백엔드 추가 시 자동으로 라운드 로빈 활성화
    let service2 = BackendService::new(addr2);
    table.add_route("example.com".to_string(), service2, None);
    
    let req = create_request(Some("example.com"), "/");
    let backend = table.route_request(&req).unwrap();
    
    // 두 주소가 모두 사용되는지 확인
    let mut addresses = std::collections::HashSet::new();
    for _ in 0..2 {
        addresses.insert(backend.get_next_address().unwrap());
    }
    
    assert_eq!(addresses.len(), 2, "두 백엔드가 모두 사용되어야 함");
} 