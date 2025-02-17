use reverse_proxy_traefik::settings::Settings;
use std::sync::Once;

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;  // 테스트 격리를 위해 추가

    static INIT: Once = Once::new();

    fn setup() {
        INIT.call_once(|| {
            cleanup_env();
        });
    }

    fn teardown() {
        cleanup_env();
    }

    // 테스트 전후 환경변수 초기화를 위한 헬퍼 함수
    fn cleanup_env() {
        std::env::remove_var("PROXY_HTTP_PORT");
        std::env::remove_var("PROXY_HTTPS_ENABLED");
        std::env::remove_var("PROXY_HTTPS_PORT");
        std::env::remove_var("PROXY_TLS_CERT");
        std::env::remove_var("PROXY_TLS_KEY");
        std::env::remove_var("PROXY_LOG_LEVEL");
        std::env::remove_var("PROXY_DOCKER_NETWORK");
        std::env::remove_var("PROXY_LABEL_PREFIX");
    }

    // 테스트용 임시 TOML 파일 생성 헬퍼
    fn create_test_toml(content: &str) -> (String, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_config.toml");
        std::fs::write(&file_path, content).unwrap();
        (file_path.to_str().unwrap().to_string(), dir)
    }

    #[tokio::test]
    #[serial]
    async fn test_settings_validation() {
        setup();

        // 1. 잘못된 포트 번호
        std::env::set_var("PROXY_HTTP_PORT", "99999");
        let result = Settings::from_env().await;
        assert!(result.is_err());
        teardown();
        
        // 2. 잘못된 로그 레벨
        std::env::set_var("PROXY_LOG_LEVEL", "invalid_level");
        let result = Settings::from_env().await;
        assert!(result.is_err());
        teardown();
        
        // 3. 잘못된 Docker 네트워크 이름
        std::env::set_var("PROXY_DOCKER_NETWORK", "invalid@network");
        let result = Settings::from_env().await;
        assert!(result.is_err());
        teardown();
        
        // 4. 잘못된 라벨 접두사
        std::env::set_var("PROXY_LABEL_PREFIX", "invalid-prefix");
        let result = Settings::from_env().await;
        assert!(result.is_err());
        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_settings_defaults() {
        setup();
        
        let settings = Settings::from_env().await.unwrap();
        
        assert_eq!(settings.server.http_port, 80);
        assert!(!settings.server.https_enabled);
        assert_eq!(settings.logging.level, tracing::Level::INFO);
        assert_eq!(settings.docker.network, "reverse-proxy-network");
        assert_eq!(settings.docker.label_prefix, "rproxy.");
        assert!(settings.middleware.is_empty());
        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_settings_from_toml() {
        setup();

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
        
        let (file_path, _temp_dir) = create_test_toml(toml_content);
        let settings = Settings::from_toml_file(&file_path).await.unwrap();
        
        assert_eq!(settings.server.http_port, 9090);
        assert!(settings.server.https_enabled);
        assert_eq!(settings.server.https_port, 443);
        assert_eq!(settings.docker.network, "test-network");
        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_settings_from_env() {
        setup();

        // 환경변수 설정
        std::env::set_var("PROXY_HTTP_PORT", "9090");
        std::env::set_var("PROXY_HTTPS_ENABLED", "true");
        std::env::set_var("PROXY_TLS_CERT", "/path/to/cert.pem");
        std::env::set_var("PROXY_TLS_KEY", "/path/to/key.pem");
        std::env::set_var("PROXY_LOG_LEVEL", "debug");
        std::env::set_var("PROXY_DOCKER_NETWORK", "custom-network");
        std::env::set_var("PROXY_LABEL_PREFIX", "custom.");

        // 설정 로드 및 검증
        let settings = Settings::from_env().await.unwrap();
        
        // 설정값 검증
        assert_eq!(settings.server.http_port, 9090);
        assert!(settings.server.https_enabled);
        assert_eq!(settings.logging.level, tracing::Level::DEBUG);
        assert_eq!(settings.docker.network, "custom-network");
        assert_eq!(settings.docker.label_prefix, "custom.");

        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn test_settings_edge_cases() {
        setup();

        // 1. 포트 충돌 케이스 (HTTP와 HTTPS 포트가 같은 경우)
        std::env::set_var("PROXY_HTTP_PORT", "443");
        std::env::set_var("PROXY_HTTPS_ENABLED", "true");
        std::env::set_var("PROXY_HTTPS_PORT", "443");
        std::env::set_var("PROXY_TLS_CERT", "/path/to/cert.pem");
        std::env::set_var("PROXY_TLS_KEY", "/path/to/key.pem");
        let result = Settings::from_env().await;
        assert!(result.is_err(), "포트 충돌이 감지되어야 함");
        if let Err(e) = result {
            assert!(e.to_string().contains("HTTP와 HTTPS 포트는 달라야 합니다"));
        }

        // 2. 포트 번호 0 케이스
        teardown();
        std::env::set_var("PROXY_HTTP_PORT", "0");
        let result = Settings::from_env().await;
        assert!(result.is_err(), "포트 0은 허용되지 않아야 함");

        // 3. 빈 네트워크 이름 케이스
        teardown();
        std::env::set_var("PROXY_DOCKER_NETWORK", "");
        let result = Settings::from_env().await;
        assert!(result.is_err(), "빈 네트워크 이름은 허용되지 않아야 함");

        // 4. 매우 긴 라벨 접두사 케이스
        teardown();
        std::env::set_var("PROXY_LABEL_PREFIX", "a".repeat(1000) + ".");
        let result = Settings::from_env().await;
        assert!(result.is_err(), "너무 긴 라벨 접두사는 허용되지 않아야 함");

        teardown();
    }
} 