use bytes::Bytes;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::fmt;

#[derive(Debug)]
pub enum ParserError {
    InvalidFormat(String),
    InvalidEncoding(String),
    InvalidLength(String),
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
            Self::InvalidEncoding(msg) => write!(f, "Invalid encoding: {}", msg),
            Self::InvalidLength(msg) => write!(f, "Invalid length: {}", msg),
        }
    }
}

#[derive(Debug)]
/// RecordSet 기반의 헤더 파서
pub struct HeaderParser {
    buffer: Vec<u8>,       // 메인 버퍼
    overflow: Vec<u8>,     // 불완전한 헤더를 위한 오버플로우 버퍼
    positions: Vec<Range>, // 헤더 위치 정보
}

/// 헤더의 시작/끝 위치 정보
#[derive(Debug)]
struct Range {
    start: usize,
    end: usize,
    name_end: usize,  // 헤더 이름의 끝 위치
}

impl HeaderParser {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4096),  // 초기 버퍼 크기
            overflow: Vec::new(),
            positions: Vec::new(),
        }
    }

    /// 스트리밍 방식으로 헤더 파싱
    pub fn parse_streaming(&mut self, chunk: &[u8]) -> Result<Vec<(String, String)>, ParserError> {
        // 1. 오버플로우 버퍼 처리
        if !self.overflow.is_empty() {
            self.buffer.extend_from_slice(&self.overflow);
            self.overflow.clear();
        }

        // 2. 새 데이터 추가
        self.buffer.extend_from_slice(chunk);

        // 3. 완전한 헤더 찾기
        let mut headers = Vec::new();
        let mut pos = 0;

        while let Some(range) = self.find_next_header(pos) {
            if range.end > self.buffer.len() {
                // 불완전한 헤더는 오버플로우로
                self.overflow.extend_from_slice(&self.buffer[pos..]);
                break;
            }

            // 헤더 파싱
            let header = std::str::from_utf8(&self.buffer[range.start..range.end])
                .map_err(|e| ParserError::InvalidEncoding(e.to_string()))?;

            // 개행 문자 제거 및 공백 처리
            let header = header.trim_end_matches(|c| c == '\r' || c == '\n');
            
            if !header.is_empty() {
                let (name, value) = self.split_header(header)?;
                headers.push((name.to_string(), value.to_string()));
            }
            
            pos = range.end;
        }

        // 남은 데이터가 있으면 오버플로우로
        if pos < self.buffer.len() {
            self.overflow.extend_from_slice(&self.buffer[pos..]);
        }
        self.buffer.clear();

        Ok(headers)
    }

    /// 헤더 이름과 값 분리
    fn split_header<'a>(&self, header: &'a str) -> Result<(&'a str, &'a str), ParserError> {
        let mut parts = header.splitn(2, ':');
        let name = parts.next()
            .ok_or_else(|| ParserError::InvalidFormat("Missing header name".into()))?
            .trim();
        let value = parts.next()
            .ok_or_else(|| ParserError::InvalidFormat("Missing header value".into()))?
            .trim();

        Ok((name, value))
    }

    /// 다음 완전한 헤더의 범위 찾기
    fn find_next_header(&self, start: usize) -> Option<Range> {
        let mut pos = start;
        while pos < self.buffer.len() {
            if self.buffer[pos] == b'\n' {
                let name_end = self.find_colon(start, pos)?;
                return Some(Range {
                    start,
                    end: pos + 1,
                    name_end,
                });
            }
            pos += 1;
        }
        None
    }

    fn find_colon(&self, start: usize, end: usize) -> Option<usize> {
        self.buffer[start..end].iter().position(|&b| b == b':').map(|p| start + p)
    }

    /// base64 인코딩된 데이터 파싱
    pub fn parse_base64(&mut self, data: &[u8]) -> Result<Bytes, ParserError> {
        self.buffer.clear();
        self.buffer.extend_from_slice(data);
        STANDARD.decode(&self.buffer)
            .map(Bytes::from)
            .map_err(|e| ParserError::InvalidEncoding(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header() {
        let mut parser = HeaderParser::new();
        let input = b"Authorization: Basic dXNlcjpwYXNz\n";
        
        let headers = parser.parse_streaming(input).unwrap();
        assert_eq!(headers[0].0, "Authorization");
        assert_eq!(headers[0].1, "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn test_parse_base64() {
        let mut parser = HeaderParser::new();
        let input = b"dXNlcjpwYXNz";  // "user:pass" in base64
        
        let decoded = parser.parse_base64(input).unwrap();
        assert_eq!(&decoded[..], b"user:pass");
    }
}