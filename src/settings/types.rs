//! Strong types for configuration validation
//! 
//! This module contains type definitions that enforce compile-time validation
//! according to the "Parse, don't validate" principle.

use std::fmt;
use url::Url;
use serde;

/// A validated service identifier.
/// 
/// This type guarantees that the contained ID is valid according to our service
/// naming rules. Once a `ValidServiceId` is created, it is guaranteed to be valid
/// for the lifetime of the program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidServiceId(String);

impl ValidServiceId {
    /// Attempts to create a new `ValidServiceId` from a string.
    /// 
    /// Returns `None` if the service ID is invalid.
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        
        // Service ID validation rules:
        // - Cannot be empty
        // - Must only contain alphanumeric characters, dashes, or underscores
        if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        
        Some(ValidServiceId(id))
    }
    
    /// Returns the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Unwraps the `ValidServiceId` into its inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ValidServiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl serde::Serialize for ValidServiceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        serializer.serialize_str(&self.0)
    }
}

struct ValidServiceIdVisitor;

impl<'de> serde::de::Visitor<'de> for ValidServiceIdVisitor {
    type Value = ValidServiceId;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a valid service ID")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where E: serde::de::Error {
        ValidServiceId::new(value).ok_or_else(|| E::custom(format!("invalid service ID: {}", value)))
    }
}

impl<'de> serde::Deserialize<'de> for ValidServiceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        deserializer.deserialize_str(ValidServiceIdVisitor)
    }
}

/// A validated middleware identifier.
/// 
/// This type guarantees that the contained ID is valid according to our middleware
/// naming rules. Once a `ValidMiddlewareId` is created, it is guaranteed to be valid
/// for the lifetime of the program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidMiddlewareId(String);

impl ValidMiddlewareId {
    /// Attempts to create a new `ValidMiddlewareId` from a string.
    /// 
    /// Returns `None` if the middleware ID is invalid.
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        
        // Middleware ID validation rules:
        // - Cannot be empty
        // - Must only contain alphanumeric characters, dashes, or underscores
        if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        
        Some(ValidMiddlewareId(id))
    }
    
    /// Returns the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Unwraps the `ValidMiddlewareId` into its inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ValidMiddlewareId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl serde::Serialize for ValidMiddlewareId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        serializer.serialize_str(&self.0)
    }
}

struct ValidMiddlewareIdVisitor;

impl<'de> serde::de::Visitor<'de> for ValidMiddlewareIdVisitor {
    type Value = ValidMiddlewareId;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a valid middleware ID")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where E: serde::de::Error {
        ValidMiddlewareId::new(value).ok_or_else(|| E::custom(format!("invalid middleware ID: {}", value)))
    }
}

impl<'de> serde::Deserialize<'de> for ValidMiddlewareId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        deserializer.deserialize_str(ValidMiddlewareIdVisitor)
    }
}

/// A validated router identifier.
/// 
/// This type guarantees that the contained ID is valid according to our router
/// naming rules. Once a `ValidRouterId` is created, it is guaranteed to be valid
/// for the lifetime of the program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidRouterId(String);

impl ValidRouterId {
    /// Attempts to create a new `ValidRouterId` from a string.
    /// 
    /// Returns `None` if the router ID is invalid.
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        
        // Router ID validation rules:
        // - Cannot be empty
        // - Must only contain alphanumeric characters, dashes, or underscores
        if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return None;
        }
        
        Some(ValidRouterId(id))
    }
    
    /// Returns the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Unwraps the `ValidRouterId` into its inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ValidRouterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl serde::Serialize for ValidRouterId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        serializer.serialize_str(&self.0)
    }
}

struct ValidRouterIdVisitor;

impl<'de> serde::de::Visitor<'de> for ValidRouterIdVisitor {
    type Value = ValidRouterId;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a valid router ID")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where E: serde::de::Error {
        ValidRouterId::new(value).ok_or_else(|| E::custom(format!("invalid router ID: {}", value)))
    }
}

impl<'de> serde::Deserialize<'de> for ValidRouterId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        deserializer.deserialize_str(ValidRouterIdVisitor)
    }
}

/// A validated router rule.
/// 
/// This type guarantees that the contained rule is syntactically valid.
/// Rules typically follow formats like:
/// - `Host(`example.com`)`
/// - `PathPrefix(`/api`)`
/// - `Host(`example.com`) && PathPrefix(`/api`)`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidRule(String);

impl ValidRule {
    /// Attempts to create a new `ValidRule` from a string.
    /// 
    /// Returns `None` if the rule is invalid.
    pub fn new(rule: impl Into<String>) -> Option<Self> {
        let rule = rule.into();
        
        // 빈 규칙은 유효하지 않음
        if rule.is_empty() {
            return None;
        }
        
        // 괄호 균형 검사
        let mut paren_stack = 0;
        for c in rule.chars() {
            match c {
                '(' => paren_stack += 1,
                ')' => {
                    paren_stack -= 1;
                    if paren_stack < 0 {
                        return None; // 불균형한 괄호
                    }
                },
                _ => {}
            }
        }
        if paren_stack != 0 {
            return None; // 불균형한 괄호
        }
        
        // 백틱 균형 검사 (더 정밀하게)
        let mut in_backtick = false;
        for c in rule.chars() {
            if c == '`' {
                in_backtick = !in_backtick;
            }
        }
        if in_backtick {
            return None; // 백틱이 열렸지만 닫히지 않음
        }
        
        // 괄호와 백틱의 관계 검사
        // 부분적인 룰(Host, Method 등)을 추출
        let parts: Vec<&str> = rule.split("&&").map(str::trim)
                                .flat_map(|p| p.split("||").map(str::trim))
                                .collect();
        
        for part in parts {
            // 괄호가 있는 부분 검사 (예: Host(...))
            let open_paren_idx = part.find('(');
            if let Some(open_idx) = open_paren_idx {
                // 닫는 괄호 찾기
                let close_idx = part.rfind(')').unwrap_or(0);
                if close_idx <= open_idx {
                    return None; // 불균형한 괄호
                }
                
                // 괄호 내용 추출
                let content = &part[open_idx+1..close_idx];
                
                // 괄호 안에 최소한 한 쌍의 백틱이 있어야 함
                let backtick_count = content.chars().filter(|&c| c == '`').count();
                if backtick_count < 2 {
                    return None;
                }
                
                // 내용 검사: 백틱이 항상 짝으로 있어야 함
                let mut in_backtick_pair = false;
                for c in content.chars() {
                    if c == '`' {
                        in_backtick_pair = !in_backtick_pair;
                    }
                }
                if in_backtick_pair {
                    return None; // 백틱이 열렸지만 닫히지 않음
                }
            }
        }
        
        // 모든 검사를 통과하면 유효한 규칙
        Some(ValidRule(rule))
    }
    
    /// Returns the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Unwraps the `ValidRule` into its inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ValidRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl serde::Serialize for ValidRule {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        serializer.serialize_str(&self.0)
    }
}

struct ValidRuleVisitor;

impl<'de> serde::de::Visitor<'de> for ValidRuleVisitor {
    type Value = ValidRule;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a valid routing rule")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where E: serde::de::Error {
        ValidRule::new(value).ok_or_else(|| E::custom(format!("invalid rule: {}", value)))
    }
}

impl<'de> serde::Deserialize<'de> for ValidRule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        deserializer.deserialize_str(ValidRuleVisitor)
    }
}

/// A validated version string.
/// 
/// This type guarantees that the contained version follows a valid format.
/// It supports semantic versioning like "1.0.0" or simpler formats like "1.0".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(String);

// serde를 위한 Serialize, Deserialize 구현
impl serde::Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

// String에서 Version으로 변환하는 방문자 구현
struct VersionVisitor;

impl<'de> serde::de::Visitor<'de> for VersionVisitor {
    type Value = Version;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a valid version string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Version::new(value).ok_or_else(|| E::custom(format!("invalid version: {}", value)))
    }
}

impl<'de> serde::Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(VersionVisitor)
    }
}

impl Version {
    /// Attempts to create a new `Version` from a string.
    /// 
    /// Returns `None` if the version format is invalid.
    pub fn new(version: impl Into<String>) -> Option<Self> {
        let version = version.into();
        
        // Version validation rules:
        // - Cannot be empty
        // - Must match pattern: major.minor or major.minor.patch
        // - Each component must be a non-negative integer
        if version.is_empty() {
            return None;
        }
        
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() < 1 || parts.len() > 3 {
            return None; // Must have 1-3 parts
        }
        
        // Check that each part is a valid non-negative integer
        for part in parts {
            if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            
            // Optional: Check that the first digit isn't a leading zero
            // unless the number is exactly zero
            if part.len() > 1 && part.starts_with('0') {
                return None;
            }
        }
        
        Some(Version(version))
    }
    
    /// Returns the major version number.
    pub fn major(&self) -> u32 {
        self.0.split('.')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
    
    /// Returns the minor version number, or 0 if not specified.
    pub fn minor(&self) -> u32 {
        self.0.split('.')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
    
    /// Returns the patch version number, or 0 if not specified.
    pub fn patch(&self) -> u32 {
        self.0.split('.')
            .nth(2)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
    
    /// Returns the inner string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// Unwraps the `Version` into its inner String.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 유효한 URL을 나타내는 타입
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidUrl(String);

// serde를 위한 Serialize, Deserialize 구현
impl serde::Serialize for ValidUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

// String에서 ValidUrl로 변환하는 방문자 구현
struct ValidUrlVisitor;

impl<'de> serde::de::Visitor<'de> for ValidUrlVisitor {
    type Value = ValidUrl;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a valid URL string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        ValidUrl::new(value).ok_or_else(|| E::custom(format!("invalid URL: {}", value)))
    }
}

impl<'de> serde::Deserialize<'de> for ValidUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(ValidUrlVisitor)
    }
}

impl ValidUrl {
    /// URL 문자열에서 `ValidUrl` 인스턴스 생성 시도
    ///
    /// 유효하지 않은 URL인 경우 `None` 반환
    pub fn new(url_str: impl Into<String>) -> Option<Self> {
        let url_str = url_str.into();
        
        // 빈 URL은 유효하지 않음
        if url_str.is_empty() {
            return None;
        }
        
        // URL 파싱 시도
        match Url::parse(&url_str) {
            Ok(url) => {
                // 스키마 검증 (http 또는 https만 허용)
                if url.scheme() != "http" && url.scheme() != "https" {
                    return None;
                }
                
                // 호스트 검증 (호스트가 반드시 존재해야 함)
                if url.host().is_none() {
                    return None;
                }
                
                Some(ValidUrl(url_str))
            },
            Err(_) => None
        }
    }
    
    /// 내부 문자열 참조 반환
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    /// 내부 문자열 소유권 반환
    pub fn into_inner(self) -> String {
        self.0
    }
    
    /// URL 객체로 파싱
    pub fn parse(&self) -> Url {
        // 이미 유효성 검증을 했으므로 unwrap 사용 가능
        Url::parse(&self.0).unwrap()
    }
    
    /// 호스트 이름 반환
    pub fn host(&self) -> String {
        self.parse().host_str().unwrap_or("").to_string()
    }
    
    /// 포트 번호 반환
    pub fn port(&self) -> Option<u16> {
        self.parse().port()
    }
    
    /// 경로 반환
    pub fn path(&self) -> String {
        self.parse().path().to_string()
    }
}

impl fmt::Display for ValidUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_service_ids() {
        // Valid service IDs
        let valid_ids = vec![
            "service1",
            "service-name",
            "service_name",
            "SERVICE_NAME",
            "123service",
            "s",
        ];

        for id in valid_ids {
            let valid_id = ValidServiceId::new(id);
            assert!(valid_id.is_some(), "Expected '{}' to be a valid service ID", id);
            
            if let Some(valid_id) = valid_id {
                // Test as_str() returns the original string
                assert_eq!(valid_id.as_str(), id);
                
                // Test into_inner() returns the original string
                assert_eq!(valid_id.into_inner(), id);
            }
        }
    }

    #[test]
    fn test_invalid_service_ids() {
        // Invalid service IDs
        let invalid_ids = vec![
            "", // Empty string
            "service name", // Contains spaces
            "service.name", // Contains periods
            "service/name", // Contains slashes
            "service:name", // Contains colons
            "service!",     // Contains special characters
        ];

        for id in invalid_ids {
            let invalid_id = ValidServiceId::new(id);
            assert!(invalid_id.is_none(), "Expected '{}' to be an invalid service ID", id);
        }
    }

    #[test]
    fn test_valid_middleware_ids() {
        // Valid middleware IDs
        let valid_ids = vec![
            "cors",
            "api-cors",
            "auth_middleware",
            "RATE_LIMIT",
            "123middleware",
            "m",
        ];

        for id in valid_ids {
            let valid_id = ValidMiddlewareId::new(id);
            assert!(valid_id.is_some(), "Expected '{}' to be a valid middleware ID", id);
            
            if let Some(valid_id) = valid_id {
                // Test as_str() returns the original string
                assert_eq!(valid_id.as_str(), id);
                
                // Test into_inner() returns the original string
                assert_eq!(valid_id.into_inner(), id);
            }
        }
    }

    #[test]
    fn test_invalid_middleware_ids() {
        // Invalid middleware IDs
        let invalid_ids = vec![
            "", // Empty string
            "middleware name", // Contains spaces
            "middleware.name", // Contains periods
            "middleware/name", // Contains slashes
            "middleware:name", // Contains colons
            "middleware!",     // Contains special characters
        ];

        for id in invalid_ids {
            let invalid_id = ValidMiddlewareId::new(id);
            assert!(invalid_id.is_none(), "Expected '{}' to be an invalid middleware ID", id);
        }
    }

    #[test]
    fn test_valid_router_ids() {
        // Valid router IDs
        let valid_ids = vec![
            "api",
            "web-router",
            "auth_router",
            "MAIN_ROUTER",
            "123router",
            "r",
        ];

        for id in valid_ids {
            let valid_id = ValidRouterId::new(id);
            assert!(valid_id.is_some(), "Expected '{}' to be a valid router ID", id);
            
            if let Some(valid_id) = valid_id {
                // Test as_str() returns the original string
                assert_eq!(valid_id.as_str(), id);
                
                // Test into_inner() returns the original string
                assert_eq!(valid_id.into_inner(), id);
            }
        }
    }

    #[test]
    fn test_invalid_router_ids() {
        // Invalid router IDs
        let invalid_ids = vec![
            "", // Empty string
            "router name", // Contains spaces
            "router.name", // Contains periods
            "router/name", // Contains slashes
            "router:name", // Contains colons
            "router!",     // Contains special characters
        ];

        for id in invalid_ids {
            let invalid_id = ValidRouterId::new(id);
            assert!(invalid_id.is_none(), "Expected '{}' to be an invalid router ID", id);
        }
    }

    #[test]
    fn test_display_implementation() {
        // Test display for ValidServiceId
        let service_id = "test-service";
        let valid_service_id = ValidServiceId::new(service_id).unwrap();
        assert_eq!(format!("{}", valid_service_id), service_id);
        
        // Test display for ValidMiddlewareId
        let middleware_id = "test-middleware";
        let valid_middleware_id = ValidMiddlewareId::new(middleware_id).unwrap();
        assert_eq!(format!("{}", valid_middleware_id), middleware_id);
        
        // Test display for ValidRouterId
        let router_id = "test-router";
        let valid_router_id = ValidRouterId::new(router_id).unwrap();
        assert_eq!(format!("{}", valid_router_id), router_id);
    }

    #[test]
    fn test_valid_rules() {
        // Valid rules
        let valid_rules = vec![
            "Host(`example.com`)",
            "PathPrefix(`/api`)",
            "Host(`example.com`) && PathPrefix(`/api`)",
            "Method(`GET`)",
            "Host(`example.com`) || Host(`api.example.com`)",
            "(Host(`example.com`) && PathPrefix(`/api`)) || (Host(`api.example.com`) && PathPrefix(`/v2`))",
        ];

        for rule in valid_rules {
            let valid_rule = ValidRule::new(rule);
            assert!(valid_rule.is_some(), "Expected '{}' to be a valid rule", rule);
            
            if let Some(valid_rule) = valid_rule {
                // Test as_str() returns the original string
                assert_eq!(valid_rule.as_str(), rule);
                
                // Test into_inner() returns the original string
                assert_eq!(valid_rule.into_inner(), rule);
            }
        }
    }

    #[test]
    fn test_invalid_rules() {
        // Invalid rules
        let invalid_rules = vec![
            "", // Empty string
            "Host(example.com)", // Missing backticks
            "Host(`example.com`", // Unbalanced parenthesis
            "Host`example.com`)", // Unbalanced parenthesis
            "Host(`example.com) && PathPrefix(/api`)", // Unbalanced backticks
            ")Host(`example.com`)", // Incorrect syntax
        ];

        for rule in invalid_rules {
            let invalid_rule = ValidRule::new(rule);
            assert!(invalid_rule.is_none(), "Expected '{}' to be an invalid rule", rule);
        }
    }

    #[test]
    fn test_valid_versions() {
        // Valid versions
        let valid_versions = vec![
            "1",
            "1.0",
            "1.0.0",
            "10.20.30",
            "0.1.0",
        ];

        for version in valid_versions {
            let valid_version = Version::new(version);
            assert!(valid_version.is_some(), "Expected '{}' to be a valid version", version);
            
            if let Some(valid_version) = valid_version {
                // Test as_str() returns the original string
                assert_eq!(valid_version.as_str(), version);
                
                // Test into_inner() returns the original string
                assert_eq!(valid_version.into_inner(), version);
            }
        }
    }

    #[test]
    fn test_invalid_versions() {
        // Invalid versions
        let invalid_versions = vec![
            "", // Empty string
            "1.0.0.0", // Too many components
            "1..0", // Empty component
            "1.a.0", // Non-numeric component
            "01.1.0", // Leading zero
            "-1.0.0", // Negative version
            "1.0-alpha", // Non-numeric suffix
        ];

        for version in invalid_versions {
            let invalid_version = Version::new(version);
            assert!(invalid_version.is_none(), "Expected '{}' to be an invalid version", version);
        }
    }

    #[test]
    fn test_version_components() {
        // Test major, minor, patch extraction
        let test_cases = vec![
            ("1", 1, 0, 0),
            ("2.3", 2, 3, 0),
            ("4.5.6", 4, 5, 6),
        ];

        for (version_str, expected_major, expected_minor, expected_patch) in test_cases {
            let version = Version::new(version_str).unwrap();
            assert_eq!(version.major(), expected_major);
            assert_eq!(version.minor(), expected_minor);
            assert_eq!(version.patch(), expected_patch);
        }
    }

    #[test]
    fn test_version_comparison() {
        // Test version ordering
        let v1 = Version::new("1.0.0").unwrap();
        let v2 = Version::new("1.0.1").unwrap();
        let v3 = Version::new("1.1.0").unwrap();
        let v4 = Version::new("2.0.0").unwrap();

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
        
        // Test equality
        let v1a = Version::new("1.0.0").unwrap();
        assert_eq!(v1, v1a);
    }

    #[test]
    fn test_valid_urls() {
        // 유효한 URL 목록
        let valid_urls = vec![
            "http://example.com",
            "https://example.com",
            "http://localhost:8080",
            "https://example.com/path",
            "http://192.168.1.1:8080/api",
            "http://user:pass@example.com",
            "https://example.com/path?query=value",
            "http://example.com/path#fragment",
        ];

        for url in valid_urls {
            let valid_url = ValidUrl::new(url);
            assert!(valid_url.is_some(), "URL '{}' should be valid", url);
            
            if let Some(valid_url) = valid_url {
                assert_eq!(valid_url.as_str(), url);
                assert_eq!(valid_url.to_string(), url);
            }
        }
    }

    #[test]
    fn test_invalid_urls() {
        // 유효하지 않은 URL 목록
        let invalid_urls = vec![
            "",
            "not a url",
            "ftp://example.com",      // 지원하지 않는 스키마
            "file:///path/to/file",   // 지원하지 않는 스키마
            "http://",                // 호스트 없음
            "http",                   // 스키마만 있음
            "example.com",            // 스키마 없음
            "://example.com",         // 잘못된 스키마
        ];

        for url in invalid_urls {
            let valid_url = ValidUrl::new(url);
            assert!(valid_url.is_none(), "URL '{}' should be invalid", url);
        }
    }

    #[test]
    fn test_url_components() {
        let url = ValidUrl::new("http://example.com:8080/path?query=value#fragment").unwrap();
        
        assert_eq!(url.host(), "example.com");
        assert_eq!(url.port(), Some(8080));
        assert_eq!(url.path(), "/path");
        
        // parse() 메서드가 완전한 Url 객체를 반환하는지 확인
        let parsed = url.parse();
        assert_eq!(parsed.scheme(), "http");
        assert_eq!(parsed.host_str(), Some("example.com"));
        assert_eq!(parsed.port(), Some(8080));
        assert_eq!(parsed.path(), "/path");
        assert_eq!(parsed.query(), Some("query=value"));
        assert_eq!(parsed.fragment(), Some("fragment"));
    }
} 