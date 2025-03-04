use std::collections::HashMap;
use serde_json::{Value, Map};

/// 도커 라벨 키를 JSON 경로로 변환
pub fn label_key_to_json_path(label_key: &str) -> (String, Vec<String>) {
    // 접두사(rproxy.http) 제거
    let parts: Vec<&str> = label_key.split('.').collect();
    if parts.len() < 4 { 
        // 최소 rproxy.http.{resource_type}.{name} 형식이어야 함
        return (String::new(), Vec::new());
    }
    
    // 리소스 타입 (middlewares, routers, services)
    let resource_type = parts[2].to_string();
    // 리소스 이름
    let resource_name = parts[3].to_string();
    
    // 경로 컴포넌트 구성
    let mut path_components = vec![resource_type, resource_name];
    
    // 속성 경로 추가
    if parts.len() > 4 {
        // type → middleware_type 으로 변환
        if parts[4] == "type" {
            path_components.push("middleware_type".to_string());
        } 
        // 미들웨어 타입별 설정은 settings 아래로 이동
        else if parts.len() > 5 && (parts[4] == "basicAuth" || parts[4] == "cors" || parts[4] == "rateLimit") {
            path_components.push("settings".to_string());
            
            // 나머지 경로 추가
            for i in 5..parts.len() {
                path_components.push(parts[i].to_string());
            }
        }
        // 기타 속성
        else {
            for i in 4..parts.len() {
                path_components.push(parts[i].to_string());
            }
        }
    }
    
    // 첫 번째 컴포넌트는 상위 객체 키, 나머지는 경로
    let root = path_components[0].clone();
    let path = path_components[1..].to_vec();
    (root, path)
}

/// 문자열 값을 적절한 타입으로 변환
pub fn convert_value(value: &str, key: &str) -> Value {
    // 특수 경우 처리: 미들웨어 목록은 쉼표로 구분된 문자열
    if key.ends_with(".middlewares") {
        // 쉼표로 구분된 문자열을 배열로 변환
        let values: Vec<Value> = value
            .split(',')
            .map(|s| Value::String(s.trim().to_string()))
            .collect();
        return Value::Array(values);
    }

    // 불리언 값 처리
    if value.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    } else if value.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }
    
    // 숫자 값 처리
    if let Ok(num) = value.parse::<i64>() {
        return Value::Number(num.into());
    }
    
    if let Ok(num) = value.parse::<f64>() {
        // serde_json은 f64를 Number로 변환할 수 있는지 확인
        if let Some(num) = serde_json::Number::from_f64(num) {
            return Value::Number(num);
        }
    }
    
    // 기본값은 문자열로 처리
    Value::String(value.to_string())
}

/// 도커 라벨 맵을 JSON 객체로 변환
pub fn labels_to_json(labels: &HashMap<String, String>, prefix: &str) -> Value {
    let mut root = Map::new();
    
    for (key, value) in labels {
        // 접두사로 시작하는 키만 처리
        if !key.starts_with(prefix) {
            continue;
        }
        
        // 라벨 키를 JSON 경로로 변환
        let (root_key, path) = label_key_to_json_path(key);
        if root_key.is_empty() {
            continue;
        }
        
        // 값 변환 - 키도 함께 전달
        let converted_value = convert_value(value, key);
        
        // 루트 객체에 해당 타입 맵이 없으면 생성
        if !root.contains_key(&root_key) {
            root.insert(root_key.clone(), Value::Object(Map::new()));
        }
        
        // 경로를 따라 객체 트리 생성 및 값 설정
        let mut current = root.get_mut(&root_key).unwrap().as_object_mut().unwrap();
        
        for (i, segment) in path.iter().enumerate() {
            if i == path.len() - 1 {
                // 마지막 세그먼트는 값을 설정
                current.insert(segment.clone(), converted_value.clone());
            } else {
                // 중간 세그먼트는 객체 생성
                if !current.contains_key(segment) {
                    current.insert(segment.clone(), Value::Object(Map::new()));
                }
                current = current.get_mut(segment).unwrap().as_object_mut().unwrap();
            }
        }
    }
    
    Value::Object(root)
}

/// JSON 객체를 도커 라벨 맵으로 변환
pub fn json_to_labels(json: &Value, prefix: &str) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    
    if let Value::Object(root) = json {
        for (root_key, root_value) in root {
            if let Value::Object(items) = root_value {
                for (item_key, item_value) in items {
                    let path = vec![root_key.as_str(), item_key.as_str()];
                    process_json_object(&mut labels, prefix, path, item_value, json);
                }
            }
        }
    }
    
    labels
}

fn process_json_object(labels: &mut HashMap<String, String>, prefix: &str, path: Vec<&str>, value: &Value, root_json: &Value) {
    match value {
        Value::Object(obj) => {
            // settings 필드는 특별한 처리
            if path.len() >= 3 && path[2] == "settings" {
                // 미들웨어 이름 가져오기
                let middleware_name = path[1];
                
                // 미들웨어 타입 가져오기
                let middleware_type = if let Some(root_obj) = root_json.as_object() {
                    if let Some(middlewares) = root_obj.get("middlewares") {
                        if let Some(middleware) = middlewares.get(middleware_name) {
                            if let Some(Value::String(typ)) = middleware.get("middleware_type") {
                                typ.as_str()
                            } else {
                                "unknown"
                            }
                        } else {
                            "unknown"
                        }
                    } else {
                        "unknown"
                    }
                } else {
                    "unknown"
                };
                
                // middleware_type 기반으로 Docker 라벨 키 중간 부분 생성
                let type_prefix = match middleware_type {
                    "basic-auth" => "basicAuth",
                    "cors" => "cors",
                    "ratelimit" => "rateLimit",
                    "headers" => "headers",
                    _ => "settings",
                };
                
                for (key, val) in obj {
                    let mut new_path = path.clone();
                    new_path[2] = type_prefix; // path[2]인 "settings"를 미들웨어 타입으로 교체
                    new_path.push(key); // 세부 설정 키 추가
                    process_json_value(labels, prefix, new_path, val);
                }
            } else {
                // 일반 객체 처리
                for (key, val) in obj {
                    let mut new_path = path.clone();
                    new_path.push(key.as_str());
                    process_json_object(labels, prefix, new_path, val, root_json);
                }
            }
        },
        // 다른 값 타입은 직접 처리
        _ => {
            process_json_value(labels, prefix, path, value);
        }
    }
}

fn process_json_value(labels: &mut HashMap<String, String>, prefix: &str, path: Vec<&str>, value: &Value) {
    // middleware_type 필드는 type으로 변환
    let actual_path = if path.len() >= 3 && path[2] == "middleware_type" {
        let mut new_path = path.clone();
        new_path[2] = "type";
        new_path
    } else {
        path
    };
    
    // 전체 키 생성
    let key = format!("{}{}", prefix, actual_path.join("."));
    
    // 값 변환
    let string_value = match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(arr) => {
            arr.iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    _ => v.to_string(),
                })
                .collect::<Vec<String>>()
                .join(",")
        },
        Value::Null => "".to_string(),
        Value::Object(_) => return, // 객체는 개별 필드로 처리되므로 여기서는 처리하지 않음
    };
    
    labels.insert(key, string_value);
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_label_key_to_json_path() {
        // 기본 변환 테스트
        let (root, path) = label_key_to_json_path("rproxy.http.middlewares.cors.enabled");
        assert_eq!(root, "middlewares");
        assert_eq!(path, vec!["cors", "enabled"]);
        
        // type → middleware_type 변환 테스트
        let (root, path) = label_key_to_json_path("rproxy.http.middlewares.auth.type");
        assert_eq!(root, "middlewares");
        assert_eq!(path, vec!["auth", "middleware_type"]);
        
        // 미들웨어 설정 → settings 이동 테스트
        let (root, path) = label_key_to_json_path("rproxy.http.middlewares.cors.cors.allowOrigins");
        assert_eq!(root, "middlewares");
        assert_eq!(path, vec!["cors", "settings", "allowOrigins"]);
        
        // 라우터 규칙 테스트
        let (root, path) = label_key_to_json_path("rproxy.http.routers.api.rule");
        assert_eq!(root, "routers");
        assert_eq!(path, vec!["api", "rule"]);
    }
    
    #[test]
    fn test_convert_value() {
        // 불리언 변환 테스트
        assert_eq!(convert_value("true", "rproxy.http.middlewares.cors.enabled"), Value::Bool(true));
        assert_eq!(convert_value("false", "rproxy.http.middlewares.cors.enabled"), Value::Bool(false));
        assert_eq!(convert_value("True", "rproxy.http.middlewares.cors.enabled"), Value::Bool(true));
        
        // 숫자 변환 테스트
        assert_eq!(convert_value("123", "rproxy.http.middlewares.cors.enabled"), Value::Number(123.into()));
        assert_eq!(convert_value("-10", "rproxy.http.middlewares.cors.enabled"), Value::Number((-10).into()));
        
        // 미들웨어 목록 변환 테스트 (쉼표 구분 문자열 -> 배열)
        let middlewares_array = convert_value("auth,cors,rate-limit", "rproxy.http.routers.api.middlewares");
        if let Value::Array(items) = middlewares_array {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::String("auth".to_string()));
            assert_eq!(items[1], Value::String("cors".to_string()));
            assert_eq!(items[2], Value::String("rate-limit".to_string()));
        } else {
            panic!("Expected array for middlewares");
        }
        
        // 일반 쉼표 구분 문자열은 그대로 문자열로 취급
        assert_eq!(convert_value("a,b,c", "rproxy.http.middlewares.cors.setting"), Value::String("a,b,c".to_string()));
        
        // 문자열 변환 테스트
        assert_eq!(convert_value("hello", "rproxy.http.middlewares.cors.enabled"), Value::String("hello".to_string()));
    }
    
    #[test]
    fn test_labels_to_json() {
        let mut labels = HashMap::new();
        labels.insert("rproxy.http.middlewares.cors.type".to_string(), "cors".to_string());
        labels.insert("rproxy.http.middlewares.cors.enabled".to_string(), "true".to_string());
        labels.insert("rproxy.http.middlewares.cors.cors.allowOrigins".to_string(), "*".to_string());
        labels.insert("rproxy.http.routers.api.rule".to_string(), "Host(`example.com`)".to_string());
        labels.insert("rproxy.http.routers.api.middlewares".to_string(), "cors,auth".to_string());
        
        let json = labels_to_json(&labels, "rproxy.http.");
        
        // 결과 검증
        if let Value::Object(root) = json {
            // 미들웨어 검증
            if let Some(Value::Object(middlewares)) = root.get("middlewares") {
                if let Some(Value::Object(cors)) = middlewares.get("cors") {
                    assert_eq!(cors.get("middleware_type"), Some(&Value::String("cors".to_string())));
                    assert_eq!(cors.get("enabled"), Some(&Value::Bool(true)));
                    
                    if let Some(Value::Object(settings)) = cors.get("settings") {
                        assert_eq!(settings.get("allowOrigins"), Some(&Value::String("*".to_string())));
                    } else {
                        panic!("Expected settings object");
                    }
                } else {
                    panic!("Expected cors object");
                }
            } else {
                panic!("Expected middlewares object");
            }
            
            // 라우터 검증
            if let Some(Value::Object(routers)) = root.get("routers") {
                if let Some(Value::Object(api)) = routers.get("api") {
                    assert_eq!(api.get("rule"), Some(&Value::String("Host(`example.com`)".to_string())));
                    
                    if let Some(Value::Array(middlewares)) = api.get("middlewares") {
                        assert_eq!(middlewares.len(), 2);
                        assert_eq!(middlewares[0], Value::String("cors".to_string()));
                        assert_eq!(middlewares[1], Value::String("auth".to_string()));
                    } else {
                        panic!("Expected middlewares array");
                    }
                } else {
                    panic!("Expected api object");
                }
            } else {
                panic!("Expected routers object");
            }
        } else {
            panic!("Expected root object");
        }
    }
    
    #[test]
    fn test_json_to_labels() {
        // JSON 객체 생성
        let mut root = Map::new();
        
        // 미들웨어 객체 생성
        let mut middlewares = Map::new();
        let mut cors = Map::new();
        cors.insert("middleware_type".to_string(), Value::String("cors".to_string()));
        cors.insert("enabled".to_string(), Value::Bool(true));
        
        let mut settings = Map::new();
        settings.insert("allowOrigins".to_string(), Value::String("*".to_string()));
        settings.insert("allowMethods".to_string(), Value::Array(vec![
            Value::String("GET".to_string()),
            Value::String("POST".to_string()),
        ]));
        cors.insert("settings".to_string(), Value::Object(settings));
        
        middlewares.insert("cors".to_string(), Value::Object(cors));
        root.insert("middlewares".to_string(), Value::Object(middlewares));
        
        // 라우터 객체 생성
        let mut routers = Map::new();
        let mut api = Map::new();
        api.insert("rule".to_string(), Value::String("Host(`example.com`)".to_string()));
        api.insert("middlewares".to_string(), Value::Array(vec![
            Value::String("cors".to_string()),
        ]));
        api.insert("service".to_string(), Value::String("api-service".to_string()));
        
        routers.insert("api".to_string(), Value::Object(api));
        root.insert("routers".to_string(), Value::Object(routers));
        
        let json = Value::Object(root);
        
        // JSON을 Docker 라벨로 변환
        let labels = json_to_labels(&json, "rproxy.http.");
        
        // 결과 검증
        assert_eq!(labels.get("rproxy.http.middlewares.cors.type"), Some(&"cors".to_string()));
        assert_eq!(labels.get("rproxy.http.middlewares.cors.enabled"), Some(&"true".to_string()));
        
        // 타입별 설정 필드 검증
        assert_eq!(labels.get("rproxy.http.middlewares.cors.cors.allowOrigins"), Some(&"*".to_string()));
        assert_eq!(labels.get("rproxy.http.middlewares.cors.cors.allowMethods"), Some(&"GET,POST".to_string()));
        
        assert_eq!(labels.get("rproxy.http.routers.api.rule"), Some(&"Host(`example.com`)".to_string()));
        assert_eq!(labels.get("rproxy.http.routers.api.middlewares"), Some(&"cors".to_string()));
        assert_eq!(labels.get("rproxy.http.routers.api.service"), Some(&"api-service".to_string()));
    }
} 