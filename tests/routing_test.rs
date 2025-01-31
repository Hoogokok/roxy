use reverse_proxy_traefik::routing::{HostInfo, RoutingTable, BackendService};

#[test]
fn test_host_info_parsing() {
    // 기본 호스트
    let host = "example.com";
    let info = HostInfo::from_header_value(host).unwrap();
    assert_eq!(info.name, "example.com");
    assert_eq!(info.port, None);

    // 포트가 있는 호스트
    let host = "example.com:8080";
    let info = HostInfo::from_header_value(host).unwrap();
    assert_eq!(info.name, "example.com");
    assert_eq!(info.port, Some(8080));

    // 잘못된 형식
    let host = "example.com:invalid";
    assert!(HostInfo::from_header_value(host).is_none());
}

#[test]
fn test_routing_table() {
    let mut table = RoutingTable::new();
    let backend = BackendService {
        address: "127.0.0.1:8080".parse().unwrap(),
    };

    // 라우트 추가
    table.add_route("example.com".to_string(), backend);

    // 존재하는 호스트 조회
    let host_info = HostInfo {
        name: "example.com".to_string(),
        port: None,
    };
    assert!(table.find_backend(&host_info).is_some());

    // 존재하지 않는 호스트 조회
    let host_info = HostInfo {
        name: "unknown.com".to_string(),
        port: None,
    };
    assert!(table.find_backend(&host_info).is_none());
} 