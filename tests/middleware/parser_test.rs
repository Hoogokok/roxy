use reverse_proxy_traefik::middleware::HeaderParser;

#[test]
fn test_parse_multiline_headers() {
    let mut parser = HeaderParser::new();
    
    // Set-Cookie 헤더 형식으로 수정
    let input = b"Set-Cookie: session=abc123; Path=/; HttpOnly\nSet-Cookie: preferences=dark-mode; Path=/settings\n";
    
    let headers = parser.parse_streaming(input).unwrap();
    assert_eq!(headers.len(), 2);
    
    assert_eq!(headers[0].0, "Set-Cookie");
    assert_eq!(headers[0].1, "session=abc123; Path=/; HttpOnly");
    assert_eq!(headers[1].0, "Set-Cookie");
    assert_eq!(headers[1].1, "preferences=dark-mode; Path=/settings");
}

#[test]
fn test_parse_incomplete_header() {
    let mut parser = HeaderParser::new();
    
    // 첫 번째 청크
    let chunk1 = b"Set-Cookie: session=abc123; Path=/;";
    let headers = parser.parse_streaming(chunk1).unwrap();
    assert_eq!(headers.len(), 0); // 불완전한 헤더는 아직 반환되지 않음
    
    // 두 번째 청크
    let chunk2 = b" HttpOnly\n";
    let headers = parser.parse_streaming(chunk2).unwrap();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].0, "Set-Cookie");
    assert_eq!(headers[0].1, "session=abc123; Path=/; HttpOnly");
}

#[test]
fn test_parse_invalid_header() {
    let mut parser = HeaderParser::new();
    
    // 잘못된 형식의 헤더
    let input = b"invalid-header-without-newline";
    let result = parser.parse_streaming(input);
    
    assert!(result.is_ok()); // 파싱은 성공하지만
    assert_eq!(result.unwrap().len(), 0); // 유효한 헤더가 없음
}

#[test]
fn test_parse_base64() {
    let mut parser = HeaderParser::new();
    
    // "Hello, World!" in base64
    let input = b"SGVsbG8sIFdvcmxkIQ==";
    let decoded = parser.parse_base64(input).unwrap();
    
    assert_eq!(&decoded[..], b"Hello, World!");
} 