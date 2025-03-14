use std::path::Path;
use tracing::debug;

use crate::settings::core::{Settings, Result};
use crate::settings::raw::RawSettings;
use crate::settings::server::{Either, HttpsDisabled, HttpsEnabled, parse_env_var};
use crate::settings::typestate::Validated;

/// 설정 로더 모듈
/// 
/// 이 모듈은 다양한 소스(환경 변수, TOML 파일 등)에서 설정을 로드하는 기능을 담당합니다.
impl<HttpsState> Settings<Validated, HttpsState> {
    /// 환경변수 및 설정 파일에서 설정을 로드합니다.
    pub async fn load() -> Result<Either<Settings<Validated, HttpsDisabled>, Settings<Validated, HttpsEnabled>>> {
        // 환경 변수에서 HTTPS 활성화 여부 확인
        let https_enabled = parse_env_var::<bool, _>("PROXY_HTTPS_ENABLED", || false)?;
        
        if https_enabled {
            // HTTPS 설정 로드 및 검증
            let raw_settings = RawSettings::<HttpsEnabled>::from_env()?;
            let validated_settings = raw_settings.validate().await?;
            Ok(Either::Right(validated_settings))
        } else {
            // HTTP 설정 로드 및 검증
            let raw_settings = RawSettings::<HttpsDisabled>::from_env()?;
            let validated_settings = raw_settings.validate().await?;
            Ok(Either::Left(validated_settings))
        }
    }

    pub async fn from_toml_file<P: AsRef<Path>>(path: P) -> Result<Either<Settings<Validated, HttpsDisabled>, Settings<Validated, HttpsEnabled>>> {
        // RawSettings의 from_toml_file 헬퍼 함수 사용
        RawSettings::<HttpsEnabled>::from_toml_file(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::parser::ValidatedConfig;
    use std::env;
    
    #[tokio::test]
    #[ignore] // 환경 변수 의존성으로 인해 기본적으로 무시
    async fn test_load_settings() {
        // 테스트를 위한 환경 변수 설정
        env::set_var("PROXY_HTTP_PORT", "8080");
        env::set_var("PROXY_HTTPS_ENABLED", "false");
        
        let result = Settings::<Validated, HttpsDisabled>::load().await;
        assert!(result.is_ok());
        
        match result.unwrap() {
            Either::Left(settings) => {
                assert_eq!(settings.server.http_port(), 8080);
            },
            Either::Right(_) => panic!("Expected HTTP-only settings"),
        }
        
        // 환경 변수 정리
        env::remove_var("PROXY_HTTP_PORT");
        env::remove_var("PROXY_HTTPS_ENABLED");
    }
    
    #[tokio::test]
    #[ignore] // 파일 시스템 의존성으로 인해 기본적으로 무시
    async fn test_from_toml_file() {
        // 테스트를 위한 임시 파일 생성 로직 구현 필요
        // 여기서는 예시만 제공
        /*
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let mut file = File::create(&config_path).unwrap();
        writeln!(file, "# Test config\n[server]\nhttp_port = 9090").unwrap();
        
        let result = Settings::from_toml_file(&config_path).await;
        assert!(result.is_ok());
        */
    }
} 