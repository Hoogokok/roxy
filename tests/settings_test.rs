use reverse_proxy_traefik::settings::Settings;
use std::fs;

#[test]
fn test_settings_validation() {
    // 설정 유효성 검사 테스트
}

#[test]
fn test_settings_defaults() {
    // 환경변수 초기화 (이전 테스트의 영향을 받지 않도록)
    std::env::remove_var("PROXY_HTTP_PORT");
    std::env::remove_var("PROXY_HTTPS_ENABLED");
    std::env::remove_var("PROXY_LOG_LEVEL");
    std::env::remove_var("PROXY_DOCKER_NETWORK");
    
    let settings = Settings::from_env().unwrap();
    
    // 서버 기본값 검증
    assert_eq!(settings.server.http_port, 8080);  // 기본 HTTP 포트
    assert!(!settings.server.https_enabled);       // HTTPS 기본 비활성화
    
    // 로깅 기본값 검증
    assert_eq!(settings.logging.level, tracing::Level::INFO);
    
    // Docker 기본값 검증
    assert_eq!(settings.docker.network, "reverse-proxy-network");
    assert_eq!(settings.docker.label_prefix, "rproxy.");
    
    // 미들웨어 기본값 검증
    assert!(settings.middleware.is_empty());  // 기본적으로 미들웨어 없음
}

#[test]
fn test_settings_from_toml() {
    // 임시 TOML 파일 생성
    let toml_content = r#"
        [server]
        http_port = 9090
        https_enabled = true
        https_port = 443

        [logging]
        format = "json"
        level = "debug"

        [docker]
        network = "test-network"
        label_prefix = "test."
    "#;
    
    let temp_file = "test_config.toml";
    fs::write(temp_file, toml_content).unwrap();

    // 설정 로드 및 검증
    let settings = Settings::from_toml_file(temp_file).unwrap();
    
    assert_eq!(settings.server.http_port, 9090);
    assert!(settings.server.https_enabled);
    assert_eq!(settings.server.https_port, 443);
    assert_eq!(settings.docker.network, "test-network");
    
    // 임시 파일 삭제
    fs::remove_file(temp_file).unwrap();
} 