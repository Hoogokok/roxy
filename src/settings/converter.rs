use std::collections::HashMap;
use serde_json::{Value, Map};

/// 언더스코어(snake_case) → 캐멀케이스(camelCase) 변환
pub fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    
    for (i, c) in s.chars().enumerate() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else if i == 0 {
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    
    result
}

/// 캐멀케이스(camelCase) → 언더스코어(snake_case) 변환
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    
    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            result.push('_');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c.to_ascii_lowercase());
        }
    }
    
    result
}

/// 표준화된 키 형식으로 변환
/// 
/// 현재 표준은 캐멀케이스(camelCase)입니다.
pub fn standardize_key(key: &str) -> String {
    if key.contains('_') {
        to_camel_case(key)
    } else {
        key.to_string()
    }
}

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
        // type → middleware_type 으로 변환 (JSON에서는 snake_case 사용)
        if parts[4] == "type" {
            path_components.push("middleware_type".to_string());
        } 
        // 미들웨어 타입별 설정은 settings 아래로 이동
        else if parts.len() > 5 && is_middleware_type(parts[4]) {
            path_components.push("settings".to_string());
            
            // 나머지 경로 추가
            for i in 5..parts.len() {
                let key = parts[i].to_string();
                // 카멜케이스를 스네이크케이스로 변환 (JSON에서는 snake_case 사용)
                if key.chars().any(|c| c.is_ascii_uppercase()) {
                    path_components.push(to_snake_case(&key));
                } else {
                    path_components.push(key);
                }
            }
        }
        // 기타 속성
        else {
            for i in 4..parts.len() {
                let key = parts[i].to_string();
                // 카멜케이스를 스네이크케이스로 변환 (JSON에서는 snake_case 사용)
                if key.chars().any(|c| c.is_ascii_uppercase()) {
                    path_components.push(to_snake_case(&key));
                } else {
                    path_components.push(key);
                }
            }
        }
    }
    
    // 첫 번째 컴포넌트는 상위 객체 키, 나머지는 경로
    let root = path_components[0].clone();
    let path = path_components[1..].to_vec();
    (root, path)
}

/// 주어진 문자열이 미들웨어 타입인지 확인
fn is_middleware_type(s: &str) -> bool {
    matches!(s, "basicAuth" | "cors" | "rateLimit" | "headers" | "stripPrefix" | "addPrefix")
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
                    let mut path = Vec::new();
                    path.push(root_key.as_str());
                    path.push(item_key.as_str());
                    
                    // 미들웨어 타입 처리
                    if root_key == "middlewares" && item_value.is_object() {
                        let obj = item_value.as_object().unwrap();
                        
                        // middleware_type 필드 처리
                        if let Some(typ) = obj.get("middleware_type") {
                            if let Some(typ_str) = typ.as_str() {
                                let type_key = format!("{}{}.{}.type", prefix, root_key, item_key);
                                labels.insert(type_key, typ_str.to_string());
                                
                                // settings 필드 처리
                                if let Some(settings) = obj.get("settings") {
                                    if let Some(settings_obj) = settings.as_object() {
                                        // 미들웨어 타입에 따른 설정 키 결정
                                        let middleware_type = match typ_str {
                                            "basic-auth" => "basicAuth",
                                            "cors" => "cors",
                                            "rate-limit" => "rateLimit",
                                            "header" => "headers",
                                            "strip-prefix" => "stripPrefix",
                                            "add-prefix" => "addPrefix",
                                            _ => "unknown"
                                        };
                                        
                                        // 설정 추가
                                        for (setting_key, setting_val) in settings_obj {
                                            let field_key = if setting_key.contains('_') {
                                                to_camel_case(setting_key)
                                            } else {
                                                setting_key.clone()
                                            };
                                            
                                            let setting_path = format!("{}{}.{}.{}.{}", 
                                                prefix, root_key, item_key, middleware_type, field_key);
                                            
                                            // 값 변환
                                            let value_str = match setting_val {
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
                                                Value::Object(_) => continue,
                                            };
                                            
                                            labels.insert(setting_path, value_str);
                                        }
                                    }
                                }
                            }
                        }
                        
                        // 미들웨어 외 다른 필드 처리
                        for (field_key, field_val) in obj {
                            if field_key != "middleware_type" && field_key != "settings" {
                                let key_str = if field_key.contains('_') {
                                    to_camel_case(field_key)
                                } else {
                                    field_key.clone()
                                };
                                
                                let field_path = format!("{}{}.{}.{}", 
                                    prefix, root_key, item_key, key_str);
                                
                                // 값 변환 (배열 등)
                                match field_val {
                                    Value::String(s) => {
                                        labels.insert(field_path, s.clone());
                                    },
                                    Value::Bool(b) => {
                                        labels.insert(field_path, b.to_string());
                                    },
                                    Value::Number(n) => {
                                        labels.insert(field_path, n.to_string());
                                    },
                                    Value::Array(arr) => {
                                        let value = arr.iter()
                                                      .map(|v| match v {
                                                          Value::String(s) => s.clone(),
                                                          _ => v.to_string(),
                                                      })
                                                      .collect::<Vec<String>>()
                                                      .join(",");
                                        labels.insert(field_path, value);
                                    },
                                    Value::Null => {
                                        labels.insert(field_path, "".to_string());
                                    },
                                    Value::Object(_) => {},
                                }
                            }
                        }
                    } 
                    // 라우터 처리
                    else if root_key == "routers" && item_value.is_object() {
                        let obj = item_value.as_object().unwrap();
                        
                        for (field_key, field_val) in obj {
                            let key_str = if field_key.contains('_') {
                                to_camel_case(field_key)
                            } else {
                                field_key.clone()
                            };
                            
                            let field_path = format!("{}{}.{}.{}", prefix, root_key, item_key, key_str);
                            
                            // 값 변환 (배열 등)
                            match field_val {
                                Value::String(s) => {
                                    labels.insert(field_path, s.clone());
                                },
                                Value::Bool(b) => {
                                    labels.insert(field_path, b.to_string());
                                },
                                Value::Number(n) => {
                                    labels.insert(field_path, n.to_string());
                                },
                                Value::Array(arr) => {
                                    let value = arr.iter()
                                                  .map(|v| match v {
                                                      Value::String(s) => s.clone(),
                                                      _ => v.to_string(),
                                                  })
                                                  .collect::<Vec<String>>()
                                                  .join(",");
                                    labels.insert(field_path, value);
                                },
                                Value::Null => {
                                    labels.insert(field_path, "".to_string());
                                },
                                Value::Object(_) => {},
                            }
                        }
                    }
                    // 서비스 처리
                    else if root_key == "services" && item_value.is_object() {
                        process_service_object(&mut labels, prefix, root_key, item_key, item_value);
                    }
                    // 기타 일반 필드 처리
                    else {
                        process_simple_object(&mut labels, prefix, path, item_value);
                    }
                }
            }
        }
    }
    
    labels
}

fn process_service_object(labels: &mut HashMap<String, String>, prefix: &str, root_key: &str, item_key: &str, value: &Value) {
    if let Some(obj) = value.as_object() {
        for (field_key, field_val) in obj {
            // loadbalancer 필드 처리
            if field_key == "loadbalancer" && field_val.is_object() {
                if let Some(lb_obj) = field_val.as_object() {
                    for (lb_key, lb_val) in lb_obj {
                        // server 필드 처리
                        if lb_key == "server" && lb_val.is_object() {
                            if let Some(server_obj) = lb_val.as_object() {
                                for (server_key, server_val) in server_obj {
                                    let key_str = if server_key.contains('_') {
                                        to_camel_case(server_key)
                                    } else {
                                        server_key.clone()
                                    };
                                    
                                    let field_path = format!("{}{}.{}.{}.{}.{}", 
                                        prefix, root_key, item_key, field_key, lb_key, key_str);
                                    
                                    // 값 변환
                                    match server_val {
                                        Value::String(s) => {
                                            labels.insert(field_path, s.clone());
                                        },
                                        Value::Bool(b) => {
                                            labels.insert(field_path, b.to_string());
                                        },
                                        Value::Number(n) => {
                                            labels.insert(field_path, n.to_string());
                                        },
                                        Value::Array(_) | Value::Null | Value::Object(_) => {},
                                    }
                                }
                            }
                        } else {
                            let key_str = if lb_key.contains('_') {
                                to_camel_case(lb_key)
                            } else {
                                lb_key.clone()
                            };
                            
                            let field_path = format!("{}{}.{}.{}.{}", 
                                prefix, root_key, item_key, field_key, key_str);
                            
                            // 값 변환
                            match lb_val {
                                Value::String(s) => {
                                    labels.insert(field_path, s.clone());
                                },
                                Value::Bool(b) => {
                                    labels.insert(field_path, b.to_string());
                                },
                                Value::Number(n) => {
                                    labels.insert(field_path, n.to_string());
                                },
                                Value::Array(_) | Value::Null | Value::Object(_) => {},
                            }
                        }
                    }
                }
            } else {
                let key_str = if field_key.contains('_') {
                    to_camel_case(field_key)
                } else {
                    field_key.clone()
                };
                
                let field_path = format!("{}{}.{}.{}", 
                    prefix, root_key, item_key, key_str);
                
                // 값 변환
                match field_val {
                    Value::String(s) => {
                        labels.insert(field_path, s.clone());
                    },
                    Value::Bool(b) => {
                        labels.insert(field_path, b.to_string());
                    },
                    Value::Number(n) => {
                        labels.insert(field_path, n.to_string());
                    },
                    Value::Array(_) | Value::Null | Value::Object(_) => {},
                }
            }
        }
    }
}

fn process_simple_object(labels: &mut HashMap<String, String>, prefix: &str, path: Vec<&str>, value: &Value) {
    match value {
        Value::Object(obj) => {
            for (key, val) in obj {
                let mut new_path = path.clone();
                
                let key_str = if key.contains('_') {
                    to_camel_case(key)
                } else {
                    key.clone()
                };
                
                new_path.push(&key_str);
                process_simple_object(labels, prefix, new_path, val);
            }
        },
        Value::String(s) => {
            let key = format!("{}{}", prefix, path.join("."));
            labels.insert(key, s.clone());
        },
        Value::Bool(b) => {
            let key = format!("{}{}", prefix, path.join("."));
            labels.insert(key, b.to_string());
        },
        Value::Number(n) => {
            let key = format!("{}{}", prefix, path.join("."));
            labels.insert(key, n.to_string());
        },
        Value::Array(arr) => {
            let key = format!("{}{}", prefix, path.join("."));
            let value = arr.iter()
                           .map(|v| match v {
                               Value::String(s) => s.clone(),
                               _ => v.to_string(),
                           })
                           .collect::<Vec<String>>()
                           .join(",");
            labels.insert(key, value);
        },
        Value::Null => {
            let key = format!("{}{}", prefix, path.join("."));
            labels.insert(key, "".to_string());
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("basic_auth"), "basicAuth");
        assert_eq!(to_camel_case("allow_origins"), "allowOrigins");
        assert_eq!(to_camel_case("rate_limit"), "rateLimit");
        assert_eq!(to_camel_case("already_camel"), "alreadyCamel");
        assert_eq!(to_camel_case("single"), "single");
        assert_eq!(to_camel_case("multiple_word_example"), "multipleWordExample");
    }
    
    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("basicAuth"), "basic_auth");
        assert_eq!(to_snake_case("allowOrigins"), "allow_origins");
        assert_eq!(to_snake_case("rateLimit"), "rate_limit");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
        assert_eq!(to_snake_case("single"), "single");
        assert_eq!(to_snake_case("MultipleWordExample"), "multiple_word_example");
    }
    
    #[test]
    fn test_standardize_key() {
        assert_eq!(standardize_key("basic_auth"), "basicAuth");
        assert_eq!(standardize_key("camelCase"), "camelCase");
        assert_eq!(standardize_key("simple"), "simple");
    }
    
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
        
        // 미들웨어 설정 테스트 (settings 필드로 이동)
        let (root, path) = label_key_to_json_path("rproxy.http.middlewares.auth.basicAuth.users");
        assert_eq!(root, "middlewares");
        assert_eq!(path, vec!["auth", "settings", "users"]);
        
        // 캐멀케이스 → 스네이크케이스 변환 테스트
        let (root, path) = label_key_to_json_path("rproxy.http.middlewares.auth.basicAuth.allowCredentials");
        assert_eq!(root, "middlewares");
        assert_eq!(path, vec!["auth", "settings", "allow_credentials"]);
        
        // 중첩된 필드 테스트
        let (root, path) = label_key_to_json_path("rproxy.http.middlewares.auth.basicAuth.settings.customField");
        assert_eq!(root, "middlewares");
        assert_eq!(path, vec!["auth", "settings", "settings", "custom_field"]);
    }
    
    #[test]
    fn test_convert_value() {
        // 문자열 테스트
        assert_eq!(convert_value("test", "key"), Value::String("test".to_string()));
        
        // 불리언 테스트
        assert_eq!(convert_value("true", "key"), Value::Bool(true));
        assert_eq!(convert_value("false", "key"), Value::Bool(false));
        
        // 숫자 테스트
        assert_eq!(convert_value("123", "key"), Value::Number(123.into()));
        assert!(matches!(convert_value("123.45", "key"), Value::Number(_)));
        
        // 미들웨어 목록 테스트
        let value = convert_value("auth,cors", "rproxy.http.routers.api.middlewares");
        assert!(matches!(value, Value::Array(_)));
        if let Value::Array(arr) = value {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], Value::String("auth".to_string()));
            assert_eq!(arr[1], Value::String("cors".to_string()));
        }
    }
    
    #[test]
    fn test_labels_to_json() {
        let mut labels = HashMap::new();
        labels.insert("rproxy.http.middlewares.cors.type".to_string(), "cors".to_string());
        labels.insert("rproxy.http.middlewares.cors.cors.allowOrigins".to_string(), "*".to_string());
        labels.insert("rproxy.http.routers.api.rule".to_string(), "Host(`test.localhost`)".to_string());
        labels.insert("rproxy.http.routers.api.middlewares".to_string(), "cors,auth".to_string());
        
        let json = labels_to_json(&labels, "rproxy.http.");
        
        // 필드 검증
        assert!(json.is_object());
        let json_obj = json.as_object().unwrap();
        
        // 미들웨어 검증
        assert!(json_obj.contains_key("middlewares"));
        let middlewares = json_obj.get("middlewares").unwrap().as_object().unwrap();
        assert!(middlewares.contains_key("cors"));
        
        // 미들웨어 타입 검증
        let cors = middlewares.get("cors").unwrap().as_object().unwrap();
        assert_eq!(cors.get("middleware_type").unwrap(), "cors");
        
        // 미들웨어 설정 검증
        assert!(cors.contains_key("settings"));
        let cors_settings = cors.get("settings").unwrap().as_object().unwrap();
        assert_eq!(cors_settings.get("allow_origins").unwrap(), "*");
        
        // 라우터 검증
        assert!(json_obj.contains_key("routers"));
        let routers = json_obj.get("routers").unwrap().as_object().unwrap();
        assert!(routers.contains_key("api"));
        
        // 라우터 설정 검증
        let api = routers.get("api").unwrap().as_object().unwrap();
        assert_eq!(api.get("rule").unwrap(), "Host(`test.localhost`)");
        
        // 미들웨어 목록 검증
        let middlewares_list = api.get("middlewares").unwrap().as_array().unwrap();
        assert_eq!(middlewares_list.len(), 2);
        assert_eq!(middlewares_list[0], "cors");
        assert_eq!(middlewares_list[1], "auth");
    }
    
    #[test]
    fn test_json_to_labels() {
        // 테스트용 JSON 객체 생성
        let json_str = r#"
        {
            "middlewares": {
                "cors": {
                    "middleware_type": "cors",
                    "settings": {
                        "allow_origins": "*",
                        "allow_methods": "GET,POST,PUT"
                    }
                },
                "auth": {
                    "middleware_type": "basic-auth",
                    "settings": {
                        "users": "admin:password"
                    }
                }
            },
            "routers": {
                "api": {
                    "rule": "Host(`test.localhost`)",
                    "middlewares": ["cors", "auth"],
                    "service": "api_service"
                }
            }
        }
        "#;
        
        let json: Value = serde_json::from_str(json_str).unwrap();
        let labels = json_to_labels(&json, "rproxy.http.");
        
        // 필드 검증
        assert!(labels.contains_key("rproxy.http.middlewares.cors.type"));
        assert_eq!(labels.get("rproxy.http.middlewares.cors.type").unwrap(), "cors");
        
        // 설정 검증 - 스네이크케이스가 캐멀케이스로 변환되었는지 확인
        assert!(labels.contains_key("rproxy.http.middlewares.cors.cors.allowOrigins"));
        assert_eq!(labels.get("rproxy.http.middlewares.cors.cors.allowOrigins").unwrap(), "*");
        
        assert!(labels.contains_key("rproxy.http.middlewares.cors.cors.allowMethods"));
        assert_eq!(labels.get("rproxy.http.middlewares.cors.cors.allowMethods").unwrap(), "GET,POST,PUT");
        
        // 기본 인증 미들웨어 검증
        assert!(labels.contains_key("rproxy.http.middlewares.auth.type"));
        assert_eq!(labels.get("rproxy.http.middlewares.auth.type").unwrap(), "basic-auth");
        
        assert!(labels.contains_key("rproxy.http.middlewares.auth.basicAuth.users"));
        assert_eq!(labels.get("rproxy.http.middlewares.auth.basicAuth.users").unwrap(), "admin:password");
        
        // 라우터 검증
        assert!(labels.contains_key("rproxy.http.routers.api.rule"));
        assert_eq!(labels.get("rproxy.http.routers.api.rule").unwrap(), "Host(`test.localhost`)");
        
        // 미들웨어 목록 검증
        assert!(labels.contains_key("rproxy.http.routers.api.middlewares"));
        assert_eq!(labels.get("rproxy.http.routers.api.middlewares").unwrap(), "cors,auth");
        
        // 서비스 검증
        assert!(labels.contains_key("rproxy.http.routers.api.service"));
        assert_eq!(labels.get("rproxy.http.routers.api.service").unwrap(), "api_service");
    }
} 