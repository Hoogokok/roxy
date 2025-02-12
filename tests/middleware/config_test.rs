use reverse_proxy_traefik::middleware::{headers::HeadersConfig, MiddlewareConfig, MiddlewareType};
use std::collections::HashMap;

#[test]
fn test_middleware_config_from_labels() {
    let mut labels = HashMap::new();
    labels.insert(
        "rproxy.http.middlewares.my-headers.type".to_string(),
        "headers".to_string(),
    );
    labels.insert(
        "rproxy.http.middlewares.my-headers.headers.customResponseHeaders.X-Custom-Header".to_string(),
        "value".to_string(),
    );

    let configs = MiddlewareConfig::from_labels(&labels);
    assert_eq!(configs.len(), 1);
    
    let (name, config) = &configs[0];
    assert_eq!(name, "my-headers");
    assert_eq!(config.middleware_type, MiddlewareType::Headers);
    assert!(config.enabled);
    assert_eq!(config.order, 0);
    assert!(config.settings.contains_key("headers.customResponseHeaders.X-Custom-Header"));
}

#[test]
fn test_middleware_config_from_toml() {
    let toml_str = r#"
        [middlewares.cors]
        middleware_type = "cors"
        enabled = true
        order = 1

        [middlewares.cors.settings]
        allow_origins = ["*"]
        allow_methods = ["GET", "POST"]
    "#;

    let configs = MiddlewareConfig::from_toml(toml_str).unwrap();
    assert_eq!(configs.len(), 1);
    
    let config = configs.get("cors").unwrap();
    assert_eq!(config.middleware_type, "cors");
    assert!(config.enabled);
    assert_eq!(config.order, 1);
}

#[test]
fn test_headers_config_from_toml() {
    let toml_content = r#"
        [request]
        add = { "X-Request-ID" = "123" }
        remove = ["User-Agent"]
        set = { "Host" = "example.com" }

        [response]
        add = { "X-Custom-Header" = "value" }
        remove = ["Server"]
        set = { "Content-Type" = "application/json" }
    "#;

    let config = HeadersConfig::from_toml(toml_content).unwrap();
    
    // 요청 헤더 설정 검증
    assert_eq!(config.request.add.get("X-Request-ID").map(String::as_str), Some("123"));
    assert!(config.request.remove.iter().any(|x| x == "User-Agent"));
    assert_eq!(config.request.set.get("Host").map(String::as_str), Some("example.com"));

    // 응답 헤더 설정 검증
    assert_eq!(config.response.add.get("X-Custom-Header").map(String::as_str), Some("value"));
    assert!(config.response.remove.iter().any(|x| x == "Server"));
    assert_eq!(config.response.set.get("Content-Type").map(String::as_str), Some("application/json"));
}

#[test]
fn test_headers_config_empty_toml() {
    let toml_content = "";
    let config = HeadersConfig::from_toml(toml_content).unwrap();
    
    // 기본값 검증
    assert!(config.request.add.is_empty());
    assert!(config.request.remove.is_empty());
    assert!(config.request.set.is_empty());
    assert!(config.response.add.is_empty());
    assert!(config.response.remove.is_empty());
    assert!(config.response.set.is_empty());
}

#[test]
fn test_headers_config_from_labels() {
    let mut labels = HashMap::new();
    
    // 요청 헤더 레이블
    labels.insert(
        "rproxy.http.middlewares.my-headers.headers.request.add.X-Request-ID".to_string(),
        "123".to_string(),
    );
    labels.insert(
        "rproxy.http.middlewares.my-headers.headers.request.remove".to_string(),
        "User-Agent".to_string(),
    );
    
    // 응답 헤더 레이블
    labels.insert(
        "rproxy.http.middlewares.my-headers.headers.response.add.X-Custom-Header".to_string(),
        "value".to_string(),
    );
    labels.insert(
        "rproxy.http.middlewares.my-headers.headers.response.set.Content-Type".to_string(),
        "application/json".to_string(),
    );

    let config = HeadersConfig::from_labels(&labels, "my-headers").unwrap();
    
    // 요청 헤더 검증
    assert_eq!(config.request.add.get("X-Request-ID").map(String::as_str), Some("123"));
    assert!(config.request.remove.iter().any(|x| x == "User-Agent"));
    
    // 응답 헤더 검증
    assert_eq!(config.response.add.get("X-Custom-Header").map(String::as_str), Some("value"));
    assert_eq!(config.response.set.get("Content-Type").map(String::as_str), Some("application/json"));
} 