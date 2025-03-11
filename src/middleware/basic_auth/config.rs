use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::middleware::MiddlewareError;
use crate::middleware::utils::get_value_case_insensitive;

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
    /// 기본 설정 키 패턴
    pub const DEFAULT_KEY_PATTERNS: [(&'static str, &'static str); 12] = [
        // 패턴 형식: (패턴, 설정 타입)
        ("rproxy.http.middlewares.*.basicAuth.users", "users"),
        ("basicAuth.users", "users"),
        ("rproxy.http.middlewares.*.basicAuth.realm", "realm"),
        ("basicAuth.realm", "realm"),
        ("rproxy.http.middlewares.*.basicAuth.source", "source"),
        ("basicAuth.source", "source"),
        ("rproxy.http.middlewares.*.basicAuth.htpasswd.path", "htpasswd_path"),
        ("basicAuth.htpasswd.path", "htpasswd_path"),
        ("rproxy.http.middlewares.*.basicAuth.env.prefix", "env_prefix"),
        ("basicAuth.env.prefix", "env_prefix"),
        ("rproxy.http.middlewares.*.basicAuth.secret.path", "secret_path"),
        ("basicAuth.secret.path", "secret_path"),
    ];

    /// Docker 라벨에서 설정을 파싱
    pub fn from_labels(labels: &HashMap<String, String>) -> Result<Self, MiddlewareError> {
        Self::from_labels_with_patterns(labels, &Self::DEFAULT_KEY_PATTERNS)
    }
    
    /// 사용자 지정 패턴을 사용하여 Docker 라벨에서 설정을 파싱
    pub fn from_labels_with_patterns(
        labels: &HashMap<String, String>,
        key_patterns: &[(&str, &str)]
    ) -> Result<Self, MiddlewareError> {
        // 기본 설정 생성
        let mut config = Self::default();
        
        // 설정 키 및 값 추출
        let settings = extract_config_settings(labels, key_patterns);
        
        // 사용자 정보 파싱
        if let Some(users_str) = settings.users {
            match parse_users(&users_str) {
                Ok(parsed_users) => config.users = parsed_users,
                Err(err) => return Err(err),
            }
        }
        
        // realm 설정
        if let Some(realm) = settings.realm {
            config.realm = realm;
        }
        
        // 소스 유형 및 경로 설정
        config.source = match settings.source.as_deref() {
            Some("htpasswd") => AuthSource::HtpasswdFile(settings.htpasswd_path.unwrap_or_default()),
            Some("env") => AuthSource::EnvVar(settings.env_prefix.unwrap_or_default()),
            Some("docker-secret") => AuthSource::DockerSecret(settings.secret_path.unwrap_or_default()),
            _ => AuthSource::Labels,
        };
        
        Ok(config)
    }
}

/// 설정 값들을 저장하는 임시 구조체
struct BasicAuthSettings {
    users: Option<String>,
    realm: Option<String>,
    source: Option<String>,
    htpasswd_path: Option<String>,
    env_prefix: Option<String>,
    secret_path: Option<String>,
}

/// 라벨에서 설정 값들을 추출하는 함수
fn extract_config_settings(
    labels: &HashMap<String, String>,
    key_patterns: &[(&str, &str)]
) -> BasicAuthSettings {
    // 결과 초기화
    let mut settings = BasicAuthSettings {
        users: None,
        realm: None,
        source: None,
        htpasswd_path: None,
        env_prefix: None,
        secret_path: None,
    };
    
    // 각 설정 값 추출
    for (pattern, setting_type) in key_patterns {
        // 이미 값이 설정되어 있으면 건너뜀
        let should_continue = match *setting_type {
            "users" => settings.users.is_some(),
            "realm" => settings.realm.is_some(),
            "source" => settings.source.is_some(),
            "htpasswd_path" => settings.htpasswd_path.is_some(),
            "env_prefix" => settings.env_prefix.is_some(),
            "secret_path" => settings.secret_path.is_some(),
            _ => false,
        };
        
        if should_continue {
            continue;
        }
        
        // 값 검색 및 설정
        if let Some(value) = find_value_by_pattern(labels, pattern) {
            match *setting_type {
                "users" => settings.users = Some(value),
                "realm" => settings.realm = Some(value),
                "source" => settings.source = Some(value.to_lowercase()),
                "htpasswd_path" => settings.htpasswd_path = Some(value),
                "env_prefix" => settings.env_prefix = Some(value),
                "secret_path" => settings.secret_path = Some(value),
                _ => {},
            }
        }
    }
    
    settings
}

/// 패턴에 맞는 값을 찾는 함수
fn find_value_by_pattern(labels: &HashMap<String, String>, pattern: &str) -> Option<String> {
    // 단순 키인 경우 (별표 없음)
    if !pattern.contains('*') {
        return get_value_case_insensitive(labels, pattern).cloned();
    }
    
    // 별표를 포함하는 패턴의 경우 (와일드카드 처리)
    for (key, value) in labels {
        if pattern_matches(key, pattern) {
            return Some(value.clone());
        }
    }
    
    None
}

/// 주어진 키가 패턴과 일치하는지 확인
fn pattern_matches(key: &str, pattern: &str) -> bool {
    // 패턴을 별표로 분리
    let parts: Vec<&str> = pattern.split('*').collect();
    
    // 키를 소문자로 변환
    let key_lower = key.to_lowercase();
    
    // 패턴의 모든 부분이 키에 포함되어 있는지 확인
    let mut last_pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        
        let part_lower = part.to_lowercase();
        
        match key_lower[last_pos..].find(&part_lower) {
            Some(pos) => {
                // 첫 번째 부분은 키의 시작 부분과 일치해야 함
                if i == 0 && pos > 0 {
                    return false;
                }
                last_pos += pos + part_lower.len();
            },
            None => return false,
        }
    }
    
    // 마지막 부분이 키의 끝 부분과 일치해야 함 (마지막 부분이 비어 있지 않은 경우)
    if !parts.is_empty() && !parts.last().unwrap().is_empty() {
        return key_lower.ends_with(&parts.last().unwrap().to_lowercase());
    }
    
    true
}

/// 사용자 문자열을 파싱합니다.
fn parse_users(users_str: &str) -> Result<HashMap<String, String>, MiddlewareError> {
    let mut users = HashMap::new();
    
    for user_entry in users_str.split(',') {
        let parts: Vec<&str> = user_entry.trim().split(':').collect();
        if parts.len() != 2 {
            return Err(MiddlewareError::Config {
                message: "Invalid user format in labels".to_string()
            });
        }
        
        users.insert(parts[0].to_string(), parts[1].to_string());
    }
    
    Ok(users)
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
    
    #[test]
    fn test_basic_auth_config_case_insensitive() {
        let mut labels = HashMap::new();
        // 카멜 케이스
        labels.insert(
            "rproxy.http.middlewares.my-auth.basicAuth.Users".to_string(),
            "admin:$apr1$H6uskkkW$IgXLP6ewTrSuBkTrqE8wj/".to_string(),
        );
        // 대문자
        labels.insert(
            "RPROXY.HTTP.MIDDLEWARES.MY-AUTH.BASICAUTH.REALM".to_string(),
            "Admin Area".to_string(),
        );
        // 소문자
        labels.insert(
            "rproxy.http.middlewares.my-auth.basicauth.source".to_string(),
            "htpasswd".to_string(),
        );
        // 혼합 케이스
        labels.insert(
            "Rproxy.Http.Middlewares.My-Auth.BasicAuth.Htpasswd.Path".to_string(),
            "/etc/auth/.htpasswd".to_string(),
        );

        let config = BasicAuthConfig::from_labels(&labels).unwrap();
        
        // 대소문자 구분 없이 설정이 올바르게 적용되었는지 확인
        assert_eq!(config.realm, "Admin Area");
        assert_eq!(
            config.users.get("admin").unwrap(),
            "$apr1$H6uskkkW$IgXLP6ewTrSuBkTrqE8wj/"
        );
        assert_eq!(
            config.source,
            AuthSource::HtpasswdFile("/etc/auth/.htpasswd".to_string())
        );
    }
    
    #[test]
    fn test_basic_auth_config_multiple_users() {
        let mut labels = HashMap::new();
        labels.insert(
            "rproxy.http.middlewares.my-auth.basicAuth.users".to_string(),
            "admin:$hash1,user:$hash2,guest:$hash3".to_string(),
        );

        let config = BasicAuthConfig::from_labels(&labels).unwrap();
        
        assert_eq!(config.users.len(), 3);
        assert_eq!(config.users.get("admin").unwrap(), "$hash1");
        assert_eq!(config.users.get("user").unwrap(), "$hash2");
        assert_eq!(config.users.get("guest").unwrap(), "$hash3");
    }
}
