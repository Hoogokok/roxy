use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::middleware::MiddlewareError;

/// Basic 인증 소스 설정
/// 
/// # Docker 라벨 예시
/// 
/// ## Labels 소스
/// ```yaml
/// labels:
///   - "rproxy.http.middlewares.my-auth.type=basic-auth"
///   - "rproxy.http.middlewares.my-auth.basicAuth.users=admin:$2y$05$..."
///   - "rproxy.http.middlewares.my-auth.basicAuth.realm=Restricted Area"
/// ```
/// 
/// ## Htpasswd 파일 소스
/// ```yaml
/// labels:
///   - "rproxy.http.middlewares.my-auth.type=basic-auth"
///   - "rproxy.http.middlewares.my-auth.basicAuth.source=htpasswd"
///   - "rproxy.http.middlewares.my-auth.basicAuth.htpasswd.path=/etc/nginx/.htpasswd"
///   - "rproxy.http.middlewares.my-auth.basicAuth.realm=Restricted Area"
/// ```
/// 
/// ## 환경 변수 소스
/// ```yaml
/// labels:
///   - "rproxy.http.middlewares.my-auth.type=basic-auth"
///   - "rproxy.http.middlewares.my-auth.basicAuth.source=env"
///   - "rproxy.http.middlewares.my-auth.basicAuth.env.prefix=BASIC_AUTH_USER_"
///   - "rproxy.http.middlewares.my-auth.basicAuth.realm=Restricted Area"
/// ```
/// 
/// ## Docker Secrets 소스
/// ```yaml
/// labels:
///   - "rproxy.http.middlewares.my-auth.type=basic-auth"
///   - "rproxy.http.middlewares.my-auth.basicAuth.source=docker-secret"
///   - "rproxy.http.middlewares.my-auth.basicAuth.secret.path=/run/secrets/basic-auth"
///   - "rproxy.http.middlewares.my-auth.basicAuth.realm=Restricted Area"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthSource {
    /// Docker 라벨에서 직접 설정
    Labels,
    /// .htpasswd 파일
    HtpasswdFile(String),
    /// 환경 변수
    EnvVar(String),
    /// Docker secrets
    DockerSecret(String),
}

impl Default for AuthSource {
    fn default() -> Self {
        Self::Labels
    }
}

/// Basic 인증 설정
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BasicAuthConfig {
    /// 사용자 이름과 해시된 비밀번호 맵
    #[serde(default)]
    pub users: HashMap<String, String>,
    
    /// 인증 영역 (realm)
    #[serde(default = "default_realm")]
    pub realm: String,
    
    /// 인증 소스
    #[serde(default)]
    pub source: AuthSource,
}

fn default_realm() -> String {
    "Restricted Area".to_string()
}

impl BasicAuthConfig {
    /// Docker 라벨에서 설정을 파싱
    pub fn from_labels(labels: &HashMap<String, String>, name: &str) -> Result<Self, MiddlewareError> {
        let prefix = format!("rproxy.http.middlewares.{}.basicAuth.", name);
        
        let mut config = Self::default();
        
        // users 파싱
        if let Some(users) = labels.get(&format!("{}{}", prefix, "users")) {
            for user_entry in users.split(',') {
                if let Some((username, password)) = user_entry.split_once(':') {
                    config.users.insert(username.trim().to_string(), password.trim().to_string());
                }
            }
        }
        
        // realm 파싱
        if let Some(realm) = labels.get(&format!("{}{}", prefix, "realm")) {
            config.realm = realm.clone();
        }
        
        // source 파싱
        if let Some(source) = labels.get(&format!("{}{}", prefix, "source")) {
            match source.to_lowercase().as_str() {
                "htpasswd" => {
                    if let Some(path) = labels.get(&format!("{}{}", prefix, "htpasswd.path")) {
                        config.source = AuthSource::HtpasswdFile(path.clone());
                    }
                }
                "env" => {
                    if let Some(var) = labels.get(&format!("{}{}", prefix, "env.name")) {
                        config.source = AuthSource::EnvVar(var.clone());
                    }
                }
                "secret" => {
                    if let Some(secret) = labels.get(&format!("{}{}", prefix, "secret.name")) {
                        config.source = AuthSource::DockerSecret(secret.clone());
                    }
                }
                _ => config.source = AuthSource::Labels,
            }
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_auth_config_from_labels() {
        let mut labels = HashMap::new();
        labels.insert(
            "rproxy.http.middlewares.my-auth.basicAuth.users".to_string(),
            "test:$apr1$H6uskkkW$IgXLP6ewTrSuBkTrqE8wj/".to_string(),
        );
        labels.insert(
            "rproxy.http.middlewares.my-auth.basicAuth.realm".to_string(),
            "My Realm".to_string(),
        );

        let config = BasicAuthConfig::from_labels(&labels, "my-auth").unwrap();
        
        assert_eq!(config.realm, "My Realm");
        assert_eq!(
            config.users.get("test").unwrap(),
            "$apr1$H6uskkkW$IgXLP6ewTrSuBkTrqE8wj/"
        );
        assert_eq!(config.source, AuthSource::Labels);
    }

    #[test]
    fn test_basic_auth_config_htpasswd() {
        let mut labels = HashMap::new();
        labels.insert(
            "rproxy.http.middlewares.my-auth.basicAuth.source".to_string(),
            "htpasswd".to_string(),
        );
        labels.insert(
            "rproxy.http.middlewares.my-auth.basicAuth.htpasswd.path".to_string(),
            "/etc/nginx/.htpasswd".to_string(),
        );

        let config = BasicAuthConfig::from_labels(&labels, "my-auth").unwrap();
        
        assert_eq!(
            config.source,
            AuthSource::HtpasswdFile("/etc/nginx/.htpasswd".to_string())
        );
    }
}
