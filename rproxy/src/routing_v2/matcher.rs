use std::hash::Hash;
use regex_lite as regex;
use crate::routing_v2::error::RoutingError;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathMatcherKind {
    Exact,
    Prefix,
    Regex,
}

#[derive(Debug, Clone)]
pub struct PathMatcher {
    pub kind: PathMatcherKind,
    pub pattern: String,
    regex: Option<regex::Regex>,
}

impl PathMatcher {
    pub fn from_str(pattern: &str) -> Result<Self, RoutingError> {
        if pattern.starts_with("^") {
            // 정규식 매칭
            let re = regex::Regex::new(pattern)
                .map_err(|e| RoutingError::InvalidPathPattern {
                    pattern: pattern.to_string(),
                    reason: e.to_string(),
                })?;
            Ok(PathMatcher {
                kind: PathMatcherKind::Regex,
                pattern: pattern.to_string(),
                regex: Some(re),
            })
        } else {
            // '*' 패턴을 제거하고 모두 Prefix로 처리
            Ok(PathMatcher {
                kind: if pattern.ends_with("*") {
                    PathMatcherKind::Prefix
                } else {
                    PathMatcherKind::Exact
                },
                pattern: pattern.trim_end_matches('*').to_string(),
                regex: None,
            })
        }
    }

    /// 접두사 매처를 생성합니다.
    /// 이 메서드는 항상 Prefix 타입의 매처를 반환하며, 실패하지 않습니다.
    pub fn prefix(pattern: &str) -> Self {
        PathMatcher {
            kind: PathMatcherKind::Prefix,
            pattern: pattern.trim_end_matches('*').trim_end_matches('/').to_string(),
            regex: None,
        }
    }

    pub fn matches(&self, path: &str) -> bool {
        if self.pattern == "/" {
            return true;
        }

        match self.kind {
            PathMatcherKind::Exact => self.pattern == path,
            PathMatcherKind::Prefix => {
                // Traefik 스타일: 접두사 매칭에서는 trailing slash 무시
                let pattern = self.pattern.trim_end_matches('/');
                let path = path.trim_end_matches('/');
                path == pattern || path.starts_with(&format!("{}/", pattern))
            },
            PathMatcherKind::Regex => self.regex.as_ref()
                .map(|r| r.is_match(path))
                .unwrap_or(false),
        }
    }
}

impl PartialEq for PathMatcher {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.pattern == other.pattern
    }
}

impl Eq for PathMatcher {}

impl Hash for PathMatcher {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // kind와 pattern만 해시에 포함시킵니다.
        // regex 필드는 pattern으로부터 파생된 값이므로 해시에서 제외합니다.
        // 이는 내부 가변성을 가진 Regex 타입으로 인한 "mutable key type" 경고를 방지합니다.
        self.kind.hash(state);
        self.pattern.hash(state);
    }
}

impl fmt::Display for PathMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            PathMatcherKind::Exact => write!(f, "Exact({})", self.pattern),
            PathMatcherKind::Prefix => write!(f, "Prefix({})", self.pattern),
            PathMatcherKind::Regex => write!(f, "Regex({})", self.pattern),
        }
    }
} 