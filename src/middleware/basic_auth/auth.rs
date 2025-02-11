use std::collections::HashMap;
use crate::middleware::MiddlewareError;
use super::config::{AuthSource, BasicAuthConfig};
use std::fs;
use bcrypt;
/// 인증 처리를 위한 트레이트
pub trait Authenticator: Send + Sync {
    /// 사용자 자격증명을 검증합니다.
    fn verify_credentials(&self, username: &str, password: &str) -> bool;
    /// 자격증명을 로드합니다.
    fn load_credentials(&mut self) -> Result<(), MiddlewareError>;
}

/// 기본 라벨 기반 인증기
pub struct LabelAuthenticator {
    users: HashMap<String, String>,
}

impl LabelAuthenticator {
    pub fn new(config: &BasicAuthConfig) -> Self {
        Self {
            users: config.users.clone(),
        }
    }
}

impl Authenticator for LabelAuthenticator {
    fn verify_credentials(&self, username: &str, password: &str) -> bool {
        self.users
            .get(username)
            .map(|hash| verify_password(password, hash))
            .unwrap_or(false)
    }

    fn load_credentials(&mut self) -> Result<(), MiddlewareError> {
        Ok(()) // 이미 config에서 로드됨
    }
}

/// .htpasswd 파일 기반 인증기
pub struct HtpasswdAuthenticator {
    path: String,
    users: HashMap<String, String>,
}

impl HtpasswdAuthenticator {
    pub fn new(path: String) -> Self {
        Self {
            path,
            users: HashMap::new(),
        }
    }
}

impl Authenticator for HtpasswdAuthenticator {
    fn verify_credentials(&self, username: &str, password: &str) -> bool {
        if let Some(hash) = self.users.get(username) {
            verify_password(password, hash)
        } else {
            false
        }
    }

    fn load_credentials(&mut self) -> Result<(), MiddlewareError> {
        let content = fs::read_to_string(&self.path).map_err(|e| MiddlewareError::Runtime {
            middleware: "basic-auth".to_string(),
            message: format!("Failed to read htpasswd file: {}", e),
            source: None,
        })?;

        self.users.clear();
        
        // 직접 파일 파싱
        for line in content.lines() {
            if let Some((username, hash)) = line.split_once(':') {
                self.users.insert(username.to_string(), hash.to_string());
            }
        }

        Ok(())
    }
}

/// 비밀번호 검증 함수
fn verify_password(password: &str, hash: &str) -> bool {
    if hash.starts_with("$2") {
        bcrypt::verify(password, hash).unwrap_or(false)
    } else {
        // bcrypt가 아닌 해시는 지원하지 않음
        false
    }
}

/// 인증기 팩토리
pub fn create_authenticator(config: &BasicAuthConfig) -> Result<Box<dyn Authenticator>, MiddlewareError> {
    match &config.source {
        AuthSource::Labels => {
            Ok(Box::new(LabelAuthenticator::new(config)))
        }
        AuthSource::HtpasswdFile(path) => {
            let mut authenticator = HtpasswdAuthenticator::new(path.clone());
            authenticator.load_credentials()?;
            Ok(Box::new(authenticator))
        }
        // TODO: 다른 인증 소스 구현
        _ => Err(MiddlewareError::Config {
            middleware: "basic-auth".to_string(),
            message: "Unsupported auth source".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcrypt::DEFAULT_COST;

    #[test]
    fn test_label_authenticator() {
        let mut users = HashMap::new();
        // bcrypt 해시로 테스트
        let hash = bcrypt::hash("test-password", DEFAULT_COST).unwrap();
        users.insert("test".to_string(), hash);

        let config = BasicAuthConfig {
            users,
            realm: "Test Realm".to_string(),
            source: AuthSource::Labels,
        };

        let authenticator = LabelAuthenticator::new(&config);
        assert!(authenticator.verify_credentials("test", "test-password"));
        assert!(!authenticator.verify_credentials("test", "wrong-password"));
    }

    #[test]
    fn test_bcrypt_verify() {
        let hash = bcrypt::hash("password", DEFAULT_COST).unwrap();
        assert!(verify_password("password", &hash));
        assert!(!verify_password("wrong", &hash));
    }
}
