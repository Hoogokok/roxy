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
    pub fn from_labels(labels: &HashMap<String, String>) -> Result<Self, MiddlewareError> {
        let extract_key = |key: &str| -> Option<String> {
            key.strip_prefix("rproxy.http.middlewares.")
                .and_then(|stripped_key| {
                    stripped_key.find(".basicAuth.").map(move |pos| stripped_key[pos + 11..].to_string())
                })
                .or_else(|| key.strip_prefix("basicAuth.").map(|s| s.to_string()))
        };
        
           // 필요한 값들을 추출
           let users = labels.iter()
           .filter_map(|(key, value)| {
               extract_key(key).filter(|k| k == "users").map(move |_| value.clone())
           })
           .flat_map(|users_str| {
               // 문자열을 소유하는 벡터로 먼저 수집하여 라이프타임 문제 해결
               let user_entries = users_str.split(',')
                   .map(|s| s.trim().to_string())
                   .collect::<Vec<String>>();
               
               // 이제 벡터에서 사용자/비밀번호 쌍 추출
               user_entries.into_iter().filter_map(move |user_str| {
                   let parts: Vec<&str> = user_str.split(':').collect();
                   if parts.len() == 2 {
                       Some((parts[0].to_string(), parts[1].to_string()))
                   } else {
                       None
                   }
               })
           })
           .collect::<HashMap<String, String>>();
        
        // 유효하지 않은 사용자 형식 확인
        if labels.iter().any(|(key, value)| {
            extract_key(key).filter(|k| k == "users").is_some() && 
            value.split(',').any(move |user_str| {
                user_str.trim().split(':').count() != 2
            })
        }) {
            return Err(MiddlewareError::Config {
                message: "Invalid user format in labels".to_string()
            });
        }
        
        // realm 설정 추출
        let realm = labels.iter()
            .find_map(|(key, value)| {
                extract_key(key).filter(|k| k == "realm").map(move |_| value.clone())
            })
            .unwrap_or_else(default_realm);
        
        // htpasswd 경로 추출
        let htpasswd_path = labels.iter()
            .find_map(|(key, value)| {
                extract_key(key).filter(|k| k == "htpasswd.path").map(move |_| value.clone())
            })
            .unwrap_or_default();
        
        // 환경 변수 접두사 추출
        let env_prefix = labels.iter()
            .find_map(|(key, value)| {
                extract_key(key).filter(|k| k == "env.prefix").map(move |_| value.clone())
            })
            .unwrap_or_default();
        
        // 시크릿 경로 추출
        let secret_path = labels.iter()
            .find_map(|(key, value)| {
                extract_key(key).filter(|k| k == "secret.path").map(move |_| value.clone())
            })
            .unwrap_or_default();
        
        // 소스 유형 결정
        let source = labels.iter()
        .find_map(|(key, value)| {
            let htpasswd_path_clone = htpasswd_path.clone();
            let env_prefix_clone = env_prefix.clone();
            let secret_path_clone = secret_path.clone();
            
            extract_key(key).filter(|k| k == "source").map(move |_| {
                match value.as_str() {
                    "htpasswd" => AuthSource::HtpasswdFile(htpasswd_path_clone),
                    "env" => AuthSource::EnvVar(env_prefix_clone),
                    "docker-secret" => AuthSource::DockerSecret(secret_path_clone),
                    _ => AuthSource::Labels,
                }
            })
        })
        .unwrap_or_default();
        
        Ok(BasicAuthConfig {
            users,
            realm,
            source,
        })
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

        let config = BasicAuthConfig::from_labels(&labels).unwrap();
        
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

        let config = BasicAuthConfig::from_labels(&labels).unwrap();
        
        assert_eq!(
            config.source,
            AuthSource::HtpasswdFile("/etc/nginx/.htpasswd".to_string())
        );
    }
}
