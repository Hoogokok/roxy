use std::{env, path::PathBuf};
use serde::Deserialize;
use tokio::fs;
use super::{server::parse_env_var, SettingsError};

#[derive(Debug, Clone, Deserialize)]
pub struct TlsSettings {
    /// HTTPS 활성화 여부
    #[serde(default)]
    pub enabled: bool,

    /// HTTPS 포트
    #[serde(default = "default_https_port")]
    pub port: u16,

    /// 인증서 파일 경로
    pub cert_path: Option<PathBuf>,

    /// 개인키 파일 경로
    pub key_path: Option<PathBuf>,
}

impl TlsSettings {
    pub fn from_env() -> Result<Self, SettingsError> {
        Ok(Self {
            enabled: parse_env_var("PROXY_TLS_ENABLED", || false)?,
            port: parse_env_var("PROXY_TLS_PORT", default_https_port)?,
            cert_path: env::var("PROXY_TLS_CERT")
                .map(PathBuf::from)
                .ok(),
            key_path: env::var("PROXY_TLS_KEY")
                .map(PathBuf::from)
                .ok(),
        })
    }

    /// TLS 설정이 유효한지 검증
    pub async fn validate(&self) -> Result<(), SettingsError> {
        if !self.enabled {
            return Ok(());
        }

        // 인증서와 키 파일이 모두 지정되어 있는지 확인
        let cert_path = self.cert_path.as_ref().ok_or_else(|| SettingsError::EnvVarMissing {
            var_name: "PROXY_TLS_CERT".to_string(),
        })?;

        let key_path = self.key_path.as_ref().ok_or_else(|| SettingsError::EnvVarMissing {
            var_name: "PROXY_TLS_KEY".to_string(),
        })?;

        // 파일 존재 여부 확인
        if !cert_path.exists() {
            return Err(SettingsError::FileError {
                path: cert_path.to_string_lossy().to_string(),
                error: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "인증서 파일을 찾을 수 없습니다",
                ),
            });
        }

        if !key_path.exists() {
            return Err(SettingsError::FileError {
                path: key_path.to_string_lossy().to_string(),
                error: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "개인키 파일을 찾을 수 없습니다",
                ),
            });
        }

        // 파일 읽기 권한 확인
        fs::read(cert_path).await.map_err(|e| SettingsError::FileError {
            path: cert_path.to_string_lossy().to_string(),
            error: e,
        })?;

        fs::read(key_path).await.map_err(|e| SettingsError::FileError {
            path: key_path.to_string_lossy().to_string(),
            error: e,
        })?;

        Ok(())
    }
}

impl Default for TlsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_https_port(),
            cert_path: None,
            key_path: None,
        }
    }
}

fn default_https_port() -> u16 {
    443
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_tls_settings_validation() {
        // 임시 인증서와 키 파일 생성
        let cert_path = PathBuf::from("test_cert.pem");
        let key_path = PathBuf::from("test_key.pem");

        let mut cert_file = File::create(&cert_path).await.unwrap();
        cert_file.write_all(b"test cert").await.unwrap();

        let mut key_file = File::create(&key_path).await.unwrap();
        key_file.write_all(b"test key").await.unwrap();

        let settings = TlsSettings {
            enabled: true,
            port: 443,
            cert_path: Some(cert_path.clone()),
            key_path: Some(key_path.clone()),
        };

        assert!(settings.validate().await.is_ok());

        // 테스트 파일 정리
        tokio::fs::remove_file(&cert_path).await.unwrap();
        tokio::fs::remove_file(&key_path).await.unwrap();
    }
} 