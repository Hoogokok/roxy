use reverse_proxy_traefik::routing_v2::matcher::{PathMatcher, PathMatcherKind};

#[test]
fn test_path_matcher_creation() {
    let test_cases = vec![
        // (패턴, 매칭 종류, 성공 여부)
        ("/api", PathMatcherKind::Exact, true),
        ("/api/*", PathMatcherKind::Prefix, true),
        ("^/api/.*", PathMatcherKind::Regex, true),
        ("^[invalid", PathMatcherKind::Regex, false),
        ("/", PathMatcherKind::Exact, true),
        ("/*", PathMatcherKind::Prefix, true),
    ];

    for (pattern, expected_kind, should_succeed) in test_cases {
        let result = PathMatcher::from_str(pattern);
        if should_succeed {
            let matcher = result.expect(&format!("Failed to create matcher for: {}", pattern));
            assert_eq!(
                matcher.kind, 
                expected_kind,
                "패턴 '{}': 예상 종류 {:?}, 실제 종류 {:?}", 
                pattern, 
                expected_kind, 
                matcher.kind
            );
        } else {
            assert!(
                result.is_err(),
                "패턴 '{}'은 실패해야 하는데 성공함", 
                pattern
            );
        }
    }
}

#[test]
fn test_path_matcher_matching() {
    let test_cases = vec![
        // (패턴, 테스트 경로, 예상 결과)
        // Exact 매칭
        ("/api", "/api", true),
        ("/api", "/api/", false),
        ("/api", "/api/users", false),
        
        // Prefix 매칭
        ("/api/*", "/api", true),
        ("/api/*", "/api/", true),
        ("/api/*", "/api/users", true),
        ("/api/*", "/api/users/123", true),
        ("/api/*", "/apis", false),
        ("/api/*", "/api-v2", false),
        
        // Regex 매칭
        ("^/api/v[0-9]+/.*", "/api/v1/users", true),
        ("^/api/v[0-9]+/.*", "/api/v2/posts", true),
        ("^/api/v[0-9]+/.*", "/api/va/users", false),
        
        // 루트 경로 특수 케이스
        ("/", "/", true),
        ("/", "/api", true),
        ("/", "/anything", true),
    ];

    for (pattern, path, expected) in test_cases {
        let matcher = PathMatcher::from_str(pattern)
            .unwrap_or_else(|_| panic!("Failed to create matcher for: {}", pattern));
        
        assert_eq!(
            matcher.matches(path),
            expected,
            "패턴: '{}', 경로: '{}', 예상 결과: {}", 
            pattern,
            path,
            expected
        );
    }
}

#[test]
fn test_path_matcher_equality() {
    let matcher1 = PathMatcher::from_str("/api").unwrap();
    let matcher2 = PathMatcher::from_str("/api").unwrap();
    let matcher3 = PathMatcher::from_str("/api/*").unwrap();

    assert_eq!(matcher1, matcher2);
    assert_ne!(matcher1, matcher3);
}

#[test]
fn test_path_matcher_hash() {
    use std::collections::HashSet;

    let matchers = vec![
        PathMatcher::from_str("/api").unwrap(),
        PathMatcher::from_str("/api").unwrap(),  // 중복
        PathMatcher::from_str("/api/*").unwrap(),
        PathMatcher::from_str("^/api/.*").unwrap(),
    ];

    let unique_matchers: HashSet<_> = matchers.into_iter().collect();
    assert_eq!(unique_matchers.len(), 3, "중복된 매처는 하나로 처리되어야 함");
}

#[test]
fn test_path_matcher_trailing_slash() {
    let prefix_matcher = PathMatcher::from_str("/api/*").unwrap();
    
    // trailing slash가 있거나 없는 경우 모두 동일하게 처리되어야 함
    assert!(prefix_matcher.matches("/api"));
    assert!(prefix_matcher.matches("/api/"));
    assert!(prefix_matcher.matches("/api/users"));
    assert!(prefix_matcher.matches("/api/users/"));
} 