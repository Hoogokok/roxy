use std::collections::HashMap;

pub fn get_value_case_insensitive<'a>(
    map: &'a HashMap<String, String>, 
    key: &str
) -> Option<&'a String> {
    // 1. 직접 조회
    if let Some(value) = map.get(key) {
        return Some(value);
    }
    
    // 2. 대소문자 무시 비교
    let key_lower = key.to_lowercase();
    for (k, v) in map {
        if k.to_lowercase() == key_lower {
            return Some(v);
        }
    }
    
    // 3. 카멜 케이스 키를 스네이크 케이스로 변환하여 찾기
    let snake_case = camel_to_snake(key);
    for (k, v) in map {
        if k.to_lowercase() == snake_case {
            return Some(v);
        }
    }
    
    // 4. 스네이크 케이스 키를 카멜 케이스로 변환하여 찾기
    for (k, v) in map {
        let camel_case = snake_to_camel(k);
        if camel_case.to_lowercase() == key_lower {
            return Some(v);
        }
    }
    
    None
}

// 카멜 케이스를 스네이크 케이스로 변환
fn camel_to_snake(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            result.push('_');
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c.to_lowercase().next().unwrap());
        }
    }
    result
}

// 스네이크 케이스를 카멜 케이스로 변환
fn snake_to_camel(s: &str) -> String {
    let parts: Vec<&str> = s.split(['_', '.']).collect();
    let mut result = String::new();
    
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            // 첫 부분은 그대로 소문자
            result.push_str(&part.to_lowercase());
        } else if !part.is_empty() {
            // 첫 글자를 대문자로 나머지는 소문자로
            let first_char = part.chars().next().unwrap_or(' ');
            result.push(first_char.to_uppercase().next().unwrap());
            result.push_str(&part[1..].to_lowercase());
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_exact_key_match() {
        let mut map = HashMap::new();
        map.insert("rateLimit.average".to_string(), "100".to_string());
        
        let result = get_value_case_insensitive(&map, "rateLimit.average");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "100");
    }
    
    #[test]
    fn test_lowercase_key_match() {
        let mut map = HashMap::new();
        map.insert("ratelimit.average".to_string(), "100".to_string());
        
        let result = get_value_case_insensitive(&map, "rateLimit.average");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "100");
    }
    
    #[test]
    fn test_uppercase_key_match() {
        let mut map = HashMap::new();
        map.insert("RATELIMIT.AVERAGE".to_string(), "100".to_string());
        
        let result = get_value_case_insensitive(&map, "rateLimit.average");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "100");
    }
    
    #[test]
    fn test_snake_case_to_camel_case() {
        let mut map = HashMap::new();
        map.insert("rate_limit.average".to_string(), "100".to_string());
        
        let result = get_value_case_insensitive(&map, "rateLimit.average");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "100");
    }
    
    #[test]
    fn test_no_match() {
        let mut map = HashMap::new();
        map.insert("something.else".to_string(), "100".to_string());
        
        let result = get_value_case_insensitive(&map, "rateLimit.average");
        assert!(result.is_none());
    }
}
