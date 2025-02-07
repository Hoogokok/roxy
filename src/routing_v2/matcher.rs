use std::hash::Hash;
use regex_lite as regex;
use crate::routing_v2::error::RoutingError;

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
        self.kind.hash(state);
        self.pattern.hash(state);
    }
} 