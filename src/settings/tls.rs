use std::{env, path::PathBuf, marker::PhantomData};
use serde::Deserialize;
use tokio::fs;
use super::{server::{Raw, Validated, parse_env_var}, SettingsError};

#[derive(Debug, Clone)]
pub struct TlsSettings<State = Validated> {
    /// HTTPS 활성화 여부
    pub enabled: bool,

    /// HTTPS 포트
    pub port: u16,

    /// 인증서 파일 경로
    pub cert_path: Option<PathBuf>,

    /// 개인키 파일 경로
    pub key_path: Option<PathBuf>,
    
    /// 상태 마커
    _marker: PhantomData<State>,
}

// 역직렬화는 Raw 상태로 수행
impl<'de> Deserialize<'de> for TlsSettings<Raw> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // 기존 구조체와 동일한 필드를 가진 헬퍼 구조체
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default)]
            enabled: bool,
            #[serde(default = "default_https_port")]
            port: u16,
            cert_path: Option<PathBuf>,
            key_path: Option<PathBuf>,
        }

        let helper = Helper::deserialize(deserializer)?;
        
        Ok(TlsSettings {
            enabled: helper.enabled,
            port: helper.port,
            cert_path: helper.cert_path,
            key_path: helper.key_path,
            _marker: PhantomData,
        })
    }
}

// Raw 상태에서의 메서드
impl TlsSettings<Raw> {
    pub fn new(
        enabled: bool,
        port: u16,
        cert_path: Option<PathBuf>,
        key_path: Option<PathBuf>,
    ) -> Self {
        Self {
            enabled,
            port,
            cert_path,
            key_path,
            _marker: PhantomData,
        }
    }
    
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
            _marker: PhantomData,
        })
    }
    
    // Raw에서 Validated로 검증하여 상태 전환
    pub async fn validated(self) -> Result<TlsSettings<Validated>, SettingsError> {
        // TLS가 활성화되어 있을 때만 검증
        if self.enabled {
            // 인증서 및 키 파일 경로 검증
            if self.cert_path.is_none() {
                return Err(SettingsError::ValidationError {
                    field: "cert_path".to_string(),
                    message: "TLS가 활성화된 경우 인증서 경로가 필요합니다".to_string(),
                });
            }
            
            if self.key_path.is_none() {
                return Err(SettingsError::ValidationError {
                    field: "key_path".to_string(),
                    message: "TLS가 활성화된 경우 개인키 경로가 필요합니다".to_string(),
                });
            }
            
            // 파일 존재 여부 확인
            let cert_path = self.cert_path.as_ref().unwrap();
            if !fs::try_exists(cert_path).await.map_err(|e| SettingsError::FileError { 
                path: cert_path.to_string_lossy().to_string(),
                error: e,
            })? {
                return Err(SettingsError::ValidationError {
                    field: "cert_path".to_string(),
                    message: format!("인증서 파일이 존재하지 않습니다: {}", cert_path.to_string_lossy()),
                });
            }
            
            let key_path = self.key_path.as_ref().unwrap();
            if !fs::try_exists(key_path).await.map_err(|e| SettingsError::FileError { 
                path: key_path.to_string_lossy().to_string(),
                error: e,
            })? {
                return Err(SettingsError::ValidationError {
                    field: "key_path".to_string(),
                    message: format!("개인키 파일이 존재하지 않습니다: {}", key_path.to_string_lossy()),
                });
            }
        }
        
        // 검증이 성공하면 Validated 상태로 전환
        Ok(TlsSettings {
            enabled: self.enabled,
            port: self.port,
            cert_path: self.cert_path,
            key_path: self.key_path,
            _marker: PhantomData,
        })
    }
}

// Validated 상태에서의 메서드들
impl TlsSettings<Validated> {
    // 안전한 게터 메서드
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    pub fn port(&self) -> u16 {
        self.port
    }
    
    pub fn cert_path(&self) -> Option<&PathBuf> {
        self.cert_path.as_ref()
    }
    
    pub fn key_path(&self) -> Option<&PathBuf> {
        self.key_path.as_ref()
    }
}

impl Default for TlsSettings<Raw> {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_https_port(),
            cert_path: None,
            key_path: None,
            _marker: PhantomData,
        }
    }
}

impl Default for TlsSettings<Validated> {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_https_port(),
            cert_path: None,
            key_path: None,
            _marker: PhantomData,
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
            _marker: PhantomData,
        };

        assert!(settings.validated().await.is_ok());

        // 테스트 파일 정리
        tokio::fs::remove_file(&cert_path).await.unwrap();
        tokio::fs::remove_file(&key_path).await.unwrap();
    }
} 