use reverse_proxy_traefik::routing_v2::*;
use std::net::SocketAddr;
use hyper::{Request, Method};
use http_body_util::Empty;
use hyper::body::Bytes;
use std::collections::HashMap;

// 테스트 헬퍼 함수
fn create_request(host: &str, path: &str) -> Request<Empty<Bytes>> {
    Request::builder()
        .method(Method::GET)
        .uri(format!("http://{}{}", host, path))
        .header("Host", host)
        .body(Empty::new())
        .unwrap()
}

// 전체 시스템 통합 테스트
#[test]
fn test_complete_routing_flow() {
    let mut table = RoutingTable::new();
    
    // 1. 다양한 라우팅 규칙 설정
    let routes = vec![
        // API 서버 (버전별 라우팅)
        ("api.example.com", "127.0.0.1:8080", "/v1/*"),
        ("api.example.com", "127.0.0.1:8081", "/v2/*"),
        // 웹 서버 (정확한 경로 매칭)
        ("web.example.com", "127.0.0.1:8090", "/"),
        // 정규식 기반 라우팅
        ("docs.example.com", "127.0.0.1:9000", "^/([a-z]+)/.*"),
    ];

    for (host, addr, pattern) in routes {
        table.add_route(
            host.to_string(),
            BackendService::new(addr.parse().unwrap()),
            Some(PathMatcher::from_str(pattern).unwrap()),
        );
    }

    // 2. 다양한 요청 시나리오 테스트
    let test_cases = vec![
        // (호스트, 경로, 예상 백엔드)
        ("api.example.com", "/v1/users", "127.0.0.1:8080"),
        ("api.example.com", "/v2/posts", "127.0.0.1:8081"),
        ("web.example.com", "/", "127.0.0.1:8090"),
        ("docs.example.com", "/guide/intro", "127.0.0.1:9000"),
    ];

    for (host, path, expected_backend) in test_cases {
        let req = create_request(host, path);
        let result = table.route_request(&req);
        
        assert!(
            result.is_ok(),
            "라우팅 실패 - 호스트: {}, 경로: {}", 
            host, 
            path
        );

        let backend = result.unwrap();
        assert_eq!(
            backend.get_next_address().unwrap().to_string(),
            expected_backend,
            "잘못된 백엔드 - 호스트: {}, 경로: {}", 
            host, 
            path
        );
    }
}

#[test]
fn test_docker_integration() {
    let mut table = RoutingTable::new();
    
    // 1. 초기 라우트 설정
    let mut initial_routes = HashMap::new();
    initial_routes.insert(
        ("api.example.com".to_string(), PathMatcher::from_str("/v1/*").unwrap()),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
    );
    table.sync_docker_routes(initial_routes);

    // 2. 초기 라우트 테스트
    let req = create_request("api.example.com", "/v1/users");
    assert!(table.route_request(&req).is_ok());

    // 3. 새로운 라우트로 업데이트
    let mut updated_routes = HashMap::new();
    updated_routes.insert(
        ("api.example.com".to_string(), PathMatcher::from_str("/v2/*").unwrap()),
        BackendService::new("127.0.0.1:8081".parse().unwrap()),
    );
    table.sync_docker_routes(updated_routes);

    // 4. 이전 라우트는 더 이상 동작하지 않음
    let old_req = create_request("api.example.com", "/v1/users");
    assert!(matches!(
        table.route_request(&old_req).unwrap_err(),
        RoutingError::BackendNotFound { host, .. }
        if host == "api.example.com"
    ));

    // 5. 새로운 라우트는 정상 동작
    let new_req = create_request("api.example.com", "/v2/users");
    assert!(table.route_request(&new_req).is_ok());
}

#[test]
fn test_error_handling_flow() {
    let mut table = RoutingTable::new();
    
    // 1. 기본 라우트 설정
    table.add_route(
        "example.com".to_string(),
        BackendService::new("127.0.0.1:8080".parse().unwrap()),
        Some(PathMatcher::from_str("/api/*").unwrap()),
    );

    // 2. 다양한 에러 시나리오 테스트
    let error_cases = vec![
        // 호스트 헤더 누락
        (None, "/api/users", RoutingError::MissingHost),
        
        // 알 수 없는 호스트
        (Some("unknown.com"), "/api/users", RoutingError::BackendNotFound {
            host: "unknown.com".to_string(),
            available_routes: vec!["example.com:/api/*".to_string()],
        }),
        
        // 잘못된 포트
        (Some("example.com:invalid"), "/api/users", RoutingError::InvalidPort {
            port: "invalid".to_string(),
            reason: "Invalid format".to_string(),
        }),
    ];

    for (host, path, expected_error) in error_cases {
        let req = match host {
            Some(h) => create_request(h, path),
            None => Request::builder()
                .method(Method::GET)
                .uri(format!("http://example.com{}", path))
                .body(Empty::new())
                .unwrap(),
        };

        let result = table.route_request(&req);
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        assert!(matches!(error, expected_error));
    }
} 