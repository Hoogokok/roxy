use std::collections::HashMap;
use crate::middleware::MiddlewareError;
use super::config::{AuthSource, BasicAuthConfig};
use std::fs;
use bcrypt;
/// Basic 인증을 위한 인증기 트레이트
/// 
/// # 지원하는 해시 알고리즘
/// - bcrypt ($2a$, $2b$, $2y$ 접두사)
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
/// 
/// # 예시
/// ```text
/// # .htpasswd 파일 형식
/// user1:$2y$05$c4WoMPo3SXsafkva.HHa6uXQZWr7oboPiC2bT/r7q1BB8I2s0BRqC
/// user2:$2b$05$LgzK4lXJzxGHVoJ0KhO1E.eQE9L5.H4TD/w0Nz8cP6b/U.ik2M0FW
/// ```
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

/// 환경 변수 기반 인증기
pub struct EnvAuthenticator {
    users: HashMap<String, String>,
    env_prefix: String,
}

impl EnvAuthenticator {
    pub fn new(prefix: String) -> Self {
        Self {
            users: HashMap::new(),
            env_prefix: prefix,
        }
    }
}

impl Authenticator for EnvAuthenticator {
    fn verify_credentials(&self, username: &str, password: &str) -> bool {
        if let Some(hash) = self.users.get(username) {
            verify_password(password, hash)
        } else {
            false
        }
    }

    fn load_credentials(&mut self) -> Result<(), MiddlewareError> {
        self.users.clear();
        
        // BASIC_AUTH_USER_<name>=<bcrypt_hash> 형식의 환경 변수 로드
        for (key, value) in std::env::vars() {
            if key.starts_with(&self.env_prefix) {
                let username = key.trim_start_matches(&self.env_prefix);
                self.users.insert(username.to_string(), value);
            }
        }
        Ok(())
    }
}

/// Docker secrets 기반 인증기
pub struct DockerSecretsAuthenticator {
    users: HashMap<String, String>,
    secrets_path: String,
}

impl DockerSecretsAuthenticator {
    pub fn new(path: String) -> Self {
        Self {
            users: HashMap::new(),
            secrets_path: path,
        }
    }
}

impl Authenticator for DockerSecretsAuthenticator {
    fn verify_credentials(&self, username: &str, password: &str) -> bool {
        if let Some(hash) = self.users.get(username) {
            verify_password(password, hash)
        } else {
            false
        }
    }

    fn load_credentials(&mut self) -> Result<(), MiddlewareError> {
        self.users.clear();
        
        // /run/secrets/<secret_name> 파일에서 자격증명 로드
        let content = fs::read_to_string(&self.secrets_path).map_err(|e| MiddlewareError::Runtime {
            middleware: "basic-auth".to_string(),
            message: format!("Failed to read Docker secret file: {}", e),
            source: None,
        })?;

        // username:hash 형식의 라인 파싱
        for line in content.lines() {
            if let Some((username, hash)) = line.split_once(':') {
                self.users.insert(username.to_string(), hash.to_string());
            }
        }
        Ok(())
    }
}

/// 인증기 팩토리
pub fn create_authenticator(config: &BasicAuthConfig) -> Result<Box<dyn Authenticator>, MiddlewareError> {
    match &config.source {
        AuthSource::Labels => Ok(Box::new(LabelAuthenticator::new(config))),
        AuthSource::HtpasswdFile(path) => {
            let mut authenticator = HtpasswdAuthenticator::new(path.clone());
            authenticator.load_credentials()?;
            Ok(Box::new(authenticator))
        }
        AuthSource::EnvVar(prefix) => {
            let mut authenticator = EnvAuthenticator::new(prefix.clone());
            authenticator.load_credentials()?;
            Ok(Box::new(authenticator))
        }
        AuthSource::DockerSecret(path) => {
            let mut authenticator = DockerSecretsAuthenticator::new(path.clone());
            authenticator.load_credentials()?;
            Ok(Box::new(authenticator))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcrypt::DEFAULT_COST;
    use std::io::Write;
    use tempfile::NamedTempFile;

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

    #[test]
    fn test_htpasswd_authenticator() -> Result<(), Box<dyn std::error::Error>> {
        // 임시 .htpasswd 파일 생성
        let mut temp_file = NamedTempFile::new()?;
        
        // bcrypt 해시 생성 및 파일에 쓰기
        let hash = bcrypt::hash("test-password", DEFAULT_COST)?;
        writeln!(temp_file, "test-user:{}", hash)?;
        
        // 지원하지 않는 해시 형식 추가
        writeln!(temp_file, "md5-user:$apr1$fHxP13Ee$Gu9.3RxLfGHvw2NpjQPyX1")?;

        let mut authenticator = HtpasswdAuthenticator::new(temp_file.path().to_str().unwrap().to_string());
        authenticator.load_credentials()?;

        // bcrypt 해시 검증
        assert!(authenticator.verify_credentials("test-user", "test-password"));
        assert!(!authenticator.verify_credentials("test-user", "wrong-password"));

        // 지원하지 않는 해시는 항상 false 반환
        assert!(!authenticator.verify_credentials("md5-user", "any-password"));

        Ok(())
    }

    #[test]
    fn test_env_authenticator() -> Result<(), Box<dyn std::error::Error>> {
        // 테스트용 환경 변수 설정
        let hash = bcrypt::hash("test-password", DEFAULT_COST)?;
        std::env::set_var("BASIC_AUTH_USER_admin", &hash);

        let mut authenticator = EnvAuthenticator::new("BASIC_AUTH_USER_".to_string());
        authenticator.load_credentials()?;

        // 검증
        assert!(authenticator.verify_credentials("admin", "test-password"));
        assert!(!authenticator.verify_credentials("admin", "wrong-password"));
        assert!(!authenticator.verify_credentials("non-existent", "any-password"));

        // 환경 변수 정리
        std::env::remove_var("BASIC_AUTH_USER_admin");
        Ok(())
    }

    #[test]
    fn test_docker_secrets_authenticator() -> Result<(), Box<dyn std::error::Error>> {
        // 임시 secret 파일 생성
        let mut temp_file = NamedTempFile::new()?;
        
        // bcrypt 해시 생성
        let hash = bcrypt::hash("secret-password", DEFAULT_COST)?;
        writeln!(temp_file, "admin:{}", hash)?;
        
        // 인증기 생성 및 테스트
        let mut authenticator = DockerSecretsAuthenticator::new(
            temp_file.path().to_str().unwrap().to_string()
        );
        authenticator.load_credentials()?;

        // 정상 케이스
        assert!(authenticator.verify_credentials("admin", "secret-password"));
        
        // 실패 케이스
        assert!(!authenticator.verify_credentials("admin", "wrong-password"));
        assert!(!authenticator.verify_credentials("non-existent", "any-password"));

        Ok(())
    }

    #[test]
    fn test_docker_secrets_file_not_found() {
        let mut authenticator = DockerSecretsAuthenticator::new(
            "/non/existent/path".to_string()
        );
        assert!(authenticator.load_credentials().is_err());
    }

    #[test]
    fn test_docker_secrets_invalid_format() -> Result<(), Box<dyn std::error::Error>> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "invalid-format-line")?;

        let mut authenticator = DockerSecretsAuthenticator::new(
            temp_file.path().to_str().unwrap().to_string()
        );
        authenticator.load_credentials()?;

        // 잘못된 형식은 무시되어야 함
        assert!(!authenticator.verify_credentials("invalid-format-line", "any-password"));

        Ok(())
    }
}
