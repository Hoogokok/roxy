use std::collections::HashMap;
use std::marker::PhantomData;
use serde::{Deserialize, Serialize};
use crate::middleware::MiddlewareError;
use crate::middleware::utils::get_value_case_insensitive;
use crate::middleware::typestate::{MiddlewareConfigError, MiddlewareValidator};
use crate::settings::typestate::{Raw, TypeState, Validatable, Validated, ValidationErrorCollector};

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

/// BasicAuthConfig를 구성하기 위한 빌더 구조체
pub struct BasicAuthConfigBuilder {
    users: HashMap<String, String>,
    realm: String,
    source: AuthSource,
}

impl BasicAuthConfigBuilder {
    /// 새로운 빌더 인스턴스 생성
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
            realm: default_realm(), // 기본 realm 사용
            source: AuthSource::Labels,
        }
    }

    /// 인증 영역(realm) 설정
    pub fn realm(mut self, realm: &str) -> Self {
        self.realm = realm.to_string();
        self
    }

    /// 사용자 추가
    pub fn add_user(mut self, username: &str, password_hash: &str) -> Self {
        self.users.insert(username.to_string(), password_hash.to_string());
        self
    }

    /// 인증 소스 설정
    pub fn source(mut self, source: AuthSource) -> Self {
        self.source = source;
        self
    }

    /// 빌더 완료 및 검증된 설정 반환
    pub fn build(self) -> Result<BasicAuthConfig<Validated>, MiddlewareConfigError> {
        // Raw 설정 생성 후 검증
        let raw_config = BasicAuthConfig::<Raw> {
            users: self.users,
            realm: self.realm,
            source: self.source,
            _state: PhantomData,
        };
        
        // validate 메서드를 통해 Raw -> Validated 변환
        raw_config.validate()
    }
}

/// Basic 인증 설정
#[derive(Debug, Clone, Deserialize)]
pub struct BasicAuthConfig<S: TypeState = Raw> {
    /// 사용자 이름과 해시된 비밀번호 맵
    #[serde(default)]
    pub users: HashMap<String, String>,
    
    /// 인증 영역 (realm)
    #[serde(default = "default_realm")]
    pub realm: String,
    
    /// 인증 소스
    #[serde(default)]
    pub source: AuthSource,
    
    /// 타입스테이트 마커
    #[serde(skip)]
    _state: PhantomData<S>,
}

impl<S: TypeState> Default for BasicAuthConfig<S> {
    fn default() -> Self {
        Self {
            users: HashMap::new(),
            realm: default_realm(),
            source: AuthSource::default(),
            _state: PhantomData,
        }
    }
}

impl BasicAuthConfig<Raw> {
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

    /// 새로운 BasicAuthConfig 인스턴스 생성
    #[allow(dead_code)]
    pub fn new(users: HashMap<String, String>, realm: String, source: AuthSource) -> Self {
        Self {
            users,
            realm,
            source,
            _state: PhantomData,
        }
    }

    /// 새로운 빌더 인스턴스 생성
    pub fn builder() -> BasicAuthConfigBuilder {
        BasicAuthConfigBuilder::new()
    }

    /// 사용자를 추가합니다 (체인 메서드)
    pub fn add_user(mut self, username: &str, password_hash: &str) -> Self {
        self.users.insert(username.to_string(), password_hash.to_string());
        self
    }

    /// 인증 영역(realm)을 설정합니다 (체인 메서드)
    pub fn with_realm(mut self, realm: &str) -> Self {
        self.realm = realm.to_string();
        self
    }

    /// 인증 소스를 설정합니다 (체인 메서드)
    pub fn with_source(mut self, source: AuthSource) -> Self {
        self.source = source;
        self
    }

    /// 현재 설정을 검증하고 Validated 상태로 변환합니다 (체인 종료 메서드)
    pub fn into_validated(self) -> Result<BasicAuthConfig<Validated>, MiddlewareConfigError> {
        self.validate()
    }

    /// Docker 라벨에서 설정을 파싱하여 Raw 상태의 설정을 생성합니다 (내부용)
    fn parse_labels_raw(labels: &HashMap<String, String>) -> Result<Self, MiddlewareError> {
        Self::from_labels_with_patterns(labels, &Self::DEFAULT_KEY_PATTERNS)
    }
    
    /// 사용자 지정 패턴을 사용하여 Docker 라벨에서 설정을 파싱합니다
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

impl Validatable<BasicAuthConfig<Validated>> for BasicAuthConfig<Raw> {
    type Error = MiddlewareConfigError;

    fn validate(self) -> Result<BasicAuthConfig<Validated>, Self::Error> {
        let mut validator = MiddlewareValidator::new();
        validator.start_collecting();
        
        // 1. 사용자가 없고 소스가 Labels인 경우 오류
        if self.users.is_empty() && self.source == AuthSource::Labels {
            validator.add_error(MiddlewareConfigError::InvalidValue {
                field: "users".to_string(),
                message: "Labels 소스를 사용할 경우 최소 한 명의 사용자가 필요합니다".to_string(),
            });
        }
        
        // 2. 사용자 이름 검증
        for (username, password) in &self.users {
            if username.is_empty() {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: "users.username".to_string(),
                    message: "사용자 이름은 비어있을 수 없습니다".to_string(),
                });
            }
            
            if password.is_empty() {
                validator.add_error(MiddlewareConfigError::InvalidValue {
                    field: format!("users.{}.password", username),
                    message: "비밀번호 해시는 비어있을 수 없습니다".to_string(),
                });
            }
        }
        
        // 3. realm 검증
        if self.realm.is_empty() {
            validator.add_error(MiddlewareConfigError::InvalidValue {
                field: "realm".to_string(),
                message: "인증 영역(realm)은 비어있을 수 없습니다".to_string(),
            });
        }
        
        // 4. 소스 유형에 따른 추가 검증
        match &self.source {
            AuthSource::HtpasswdFile(path) => {
                if path.is_empty() {
                    validator.add_error(MiddlewareConfigError::InvalidValue {
                        field: "source.htpasswd_path".to_string(),
                        message: "htpasswd 파일 경로는 비어있을 수 없습니다".to_string(),
                    });
                }
            },
            AuthSource::EnvVar(prefix) => {
                if prefix.is_empty() {
                    validator.add_error(MiddlewareConfigError::InvalidValue {
                        field: "source.env_prefix".to_string(),
                        message: "환경 변수 접두사는 비어있을 수 없습니다".to_string(),
                    });
                }
            },
            AuthSource::DockerSecret(path) => {
                if path.is_empty() {
                    validator.add_error(MiddlewareConfigError::InvalidValue {
                        field: "source.secret_path".to_string(),
                        message: "Docker Secret 파일 경로는 비어있을 수 없습니다".to_string(),
                    });
                }
            },
            _ => {}
        }
        
        // 검증 결과 처리
        if validator.has_errors() {
            // 첫 번째 오류 반환
            let errors = validator.into_errors();
            Err(errors.into_iter().next().unwrap())
        } else {
            // 검증된 설정 반환
            Ok(BasicAuthConfig {
                users: self.users,
                realm: self.realm,
                source: self.source,
                _state: PhantomData,
            })
        }
    }
}

impl BasicAuthConfig<Validated> {
    /// Docker 라벨에서 설정을 파싱하고 곧바로 검증합니다.
    /// "검증하지 말고 파싱하라" 접근법에 따라 파싱과 검증을 한 단계로 통합합니다.
    pub fn from_labels(labels: &HashMap<String, String>) -> Result<Self, MiddlewareError> {
        // 1. 원시 설정 파싱
        let raw_config = BasicAuthConfig::<Raw>::parse_labels_raw(labels)?;
        
        // 2. 파싱된 설정 검증
        raw_config.validate()
            .map_err(|e| MiddlewareError::Config { message: e.to_string() })
    }
}

fn default_realm() -> String {
    "Restricted Area".to_string()
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

        let config = BasicAuthConfig::<Validated>::from_labels(&labels).unwrap();
        
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

        let config = BasicAuthConfig::<Validated>::from_labels(&labels).unwrap();
        
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

        let config = BasicAuthConfig::<Validated>::from_labels(&labels).unwrap();
        
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

        let config = BasicAuthConfig::<Validated>::from_labels(&labels).unwrap();
        
        assert_eq!(config.users.len(), 3);
        assert_eq!(config.users.get("admin").unwrap(), "$hash1");
        assert_eq!(config.users.get("user").unwrap(), "$hash2");
        assert_eq!(config.users.get("guest").unwrap(), "$hash3");
    }
    
    #[test]
    fn test_basic_auth_config_validation_success() {
        let mut users = HashMap::new();
        users.insert("admin".to_string(), "$hash1".to_string());
        
        let config = BasicAuthConfig::<Raw>::new(
            users,
            "Test Realm".to_string(),
            AuthSource::Labels
        );
        
        let validated = config.validate();
        assert!(validated.is_ok());
    }
    
    #[test]
    fn test_basic_auth_config_validation_failure_empty_users() {
        let mut labels = HashMap::new();
        labels.insert(
            "rproxy.http.middlewares.my-auth.basicAuth.realm".to_string(),
            "My Realm".to_string(),
        );
        
        let result = BasicAuthConfig::<Validated>::from_labels(&labels);
        assert!(result.is_err());
        
        let err = result.unwrap_err();
        match err {
            MiddlewareError::Config { message } => {
                assert!(message.contains("users"));
            },
            _ => panic!("예상치 못한 오류 타입"),
        }
    }
    
    #[test]
    fn test_basic_auth_config_validation_failure_empty_realm() {
        let mut users = HashMap::new();
        users.insert("admin".to_string(), "$hash1".to_string());
        
        let config = BasicAuthConfig::<Raw>::new(
            users,
            "".to_string(),
            AuthSource::Labels
        );
        
        let validated = config.validate();
        assert!(validated.is_err());
        
        match validated.err().unwrap() {
            MiddlewareConfigError::InvalidValue { field, .. } => {
                assert_eq!(field, "realm");
            },
            _ => panic!("잘못된 오류 타입"),
        }
    }
    
    #[test]
    fn test_basic_auth_config_validation_htpasswd_path() {
        let mut labels = HashMap::new();
        labels.insert(
            "rproxy.http.middlewares.my-auth.basicAuth.source".to_string(),
            "htpasswd".to_string(),
        );
        // 경로를 제공하지 않음
        
        let result = BasicAuthConfig::<Validated>::from_labels(&labels);
        assert!(result.is_err());
        
        let err = result.unwrap_err();
        match err {
            MiddlewareError::Config { message } => {
                assert!(message.contains("htpasswd_path"));
            },
            _ => panic!("예상치 못한 오류 타입"),
        }
    }

    // 새로운 테스트 케이스들
    #[test]
    fn test_builder_pattern() {
        // 빌더 패턴을 이용한 설정 생성 테스트
        let config = BasicAuthConfig::<Raw>::builder()
            .realm("Admin Area")
            .add_user("admin", "$hash1")
            .add_user("user", "$hash2")
            .source(AuthSource::Labels)
            .build()
            .unwrap();
        
        // 검증된 상태의 설정이 생성되어야 함
        assert_eq!(config.realm, "Admin Area");
        assert_eq!(config.users.len(), 2);
        assert_eq!(config.users.get("admin").unwrap(), "$hash1");
        assert_eq!(config.users.get("user").unwrap(), "$hash2");
        assert_eq!(config.source, AuthSource::Labels);
    }
    
    #[test]
    fn test_chain_termination() {
        // 체인 메서드와 명시적 종료 메서드 테스트
        let mut users = HashMap::new();
        users.insert("admin".to_string(), "$hash1".to_string());
        
        let config = BasicAuthConfig::<Raw>::new(
                users,
                "Default Realm".to_string(),
                AuthSource::Labels
            )
            .with_realm("Custom Realm")
            .with_source(AuthSource::HtpasswdFile("/path/to/.htpasswd".to_string()))
            .add_user("guest", "$hash2")
            .into_validated()
            .unwrap();
        
        // 검증된 상태로 전환되었는지 확인
        assert_eq!(config.realm, "Custom Realm");
        assert_eq!(config.users.len(), 2);
        assert_eq!(config.source, AuthSource::HtpasswdFile("/path/to/.htpasswd".to_string()));
    }
    
    #[test]
    fn test_builder_validation_failure() {
        // 유효하지 않은 설정으로 빌더 패턴 테스트 (실패 케이스)
        let result = BasicAuthConfig::<Raw>::builder()
            .realm("") // 빈 realm (유효하지 않음)
            .add_user("admin", "$hash1")
            .build();
        
        // 검증 실패해야 함
        assert!(result.is_err());
        // 적절한 오류 메시지 확인
        match result.unwrap_err() {
            MiddlewareConfigError::InvalidValue { field, .. } => {
                assert_eq!(field, "realm");
            },
            _ => panic!("예상하지 못한 오류 타입"),
        }
    }
}
