use reverse_proxy_traefik::routing_v2::*;
use hyper::{Request, Method};
use http_body_util::Empty;
use hyper::body::Bytes;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

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

#[tokio::test]
async fn test_load_balanced_routing() {
    // 2. 라우팅 테이블 생성
    let mut table = RoutingTable::new();
    
    // 3. 로드밸런서가 활성화된 백엔드 서비스 생성
    let mut service = BackendService::new("127.0.0.1:8001".parse().unwrap());
    service.enable_load_balancer(LoadBalancerStrategy::RoundRobin {
        current_index: AtomicUsize::new(0),
    });
    service.add_address("127.0.0.1:8002".parse().unwrap(), 1).unwrap();

    // 4. 라우트 추가
    table.add_route(
        "api.example.com".to_string(),
        service,
        Some(PathMatcher::from_str("/").unwrap()),
    );

    // 5. 요청 테스트
    let request = create_request("api.example.com", "/users");
    
    // 첫 번째 요청은 server1으로
    let backend1 = table.route_request(&request).unwrap();
    assert_eq!(backend1.get_next_address().unwrap().to_string(), "127.0.0.1:8001");

    // 두 번째 요청은 server2로
    let backend2 = table.route_request(&request).unwrap();
    assert_eq!(backend2.get_next_address().unwrap().to_string(), "127.0.0.1:8002");
}

#[tokio::test]
async fn test_weighted_load_balancing() {
    // 1. 라우팅 테이블 생성
    let mut table = RoutingTable::new();
    
    // 2. 가중치 2:1 설정으로 백엔드 서비스 생성
    let mut service = BackendService::new("127.0.0.1:8001".parse().unwrap());
    service.enable_load_balancer(LoadBalancerStrategy::Weighted {
        current_index: AtomicUsize::new(0),
        total_weight: 1,
    });
    service.add_address("127.0.0.1:8002".parse().unwrap(), 2).unwrap();

    // 3. 라우트 추가
    table.add_route(
        "api.example.com".to_string(),
        service,
        Some(PathMatcher::from_str("/").unwrap()),
    );

    // 4. 요청 생성
    let request = create_request("api.example.com", "/users");
    
    // 5. 여러 번의 요청을 보내고 분배 비율 확인
    let mut addr1_count = 0;
    let mut addr2_count = 0;

    for _ in 0..6 {
        let backend = table.route_request(&request).unwrap();
        match backend.get_next_address().unwrap().to_string().as_str() {
            "127.0.0.1:8001" => addr1_count += 1,
            "127.0.0.1:8002" => addr2_count += 1,
            _ => unreachable!(),
        }
    }

    // 6. 가중치에 따른 분배 확인
    assert_eq!(addr1_count, 2, "첫 번째 서버(가중치 1)는 2번 호출되어야 함");
    assert_eq!(addr2_count, 4, "두 번째 서버(가중치 2)는 4번 호출되어야 함");
} 