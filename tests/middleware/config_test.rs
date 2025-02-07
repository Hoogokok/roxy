use reverse_proxy_traefik::middleware::MiddlewareConfig;
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
    assert_eq!(config.middleware_type, "headers");
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