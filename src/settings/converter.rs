use std::collections::HashMap;
use serde_json::{Value, Map};
use tracing::debug;

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

/// JSON 키를 표준화된 형식으로 변환 (camelCase -> snake_case)
pub fn standardize_json_key(key: &str) -> String {
    if key.contains('.') || key.chars().any(|c| c.is_ascii_uppercase()) {
        to_snake_case(key)
    } else {
        key.to_string()
    }
}

/// 라벨 키를 표준화된 형식으로 변환 (snake_case -> camelCase)
pub fn standardize_label_key(key: &str) -> String {
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
    let mut path_components = vec![resource_type.clone(), resource_name];
    
    // 속성 경로 추가
    if parts.len() > 4 {
        // type 필드는 그대로 유지 (serde 속성을 통해 변환됨)
        if parts[4] == "type" {
            path_components.push("type".to_string());
        } 
        // 미들웨어 타입별 설정은 settings 아래로 이동
        else if parts.len() > 5 && is_middleware_type(parts[4]) {
            path_components.push("settings".to_string());
            
            // 나머지 경로 추가
            for i in 5..parts.len() {
                let key = parts[i].to_string();
                // 표준화된 JSON 키 형식으로 변환
                path_components.push(standardize_json_key(&key));
            }
        }
        // 기타 속성
        else {
            for i in 4..parts.len() {
                let key = parts[i].to_string();
                // 표준화된 JSON 키 형식으로 변환
                path_components.push(standardize_json_key(&key));
            }
        }
    }
    
    (resource_type, path_components.into_iter().skip(1).collect())
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

/// 도커 라벨 맵을 JSON 객체로 변환 - 함수형 패러다임 적용 버전
pub fn labels_to_json(labels: &HashMap<String, String>, prefix: &str) -> Value {
    // 접두사 표준화
    let normalized_prefix = normalize_prefix(prefix);
    
    // 접두사로 시작하는 라벨만 필터링
    let filtered_labels = filter_labels_by_prefix(labels, &normalized_prefix);
    
    // 서버 라벨 별도 처리
    let (server_labels, regular_labels): (HashMap<String, String>, HashMap<String, String>) = 
        filtered_labels.into_iter()
            .partition(|(key, _)| key.contains(".loadbalancer.server."));
    
    // 일반 라벨 처리
    let mut root = process_regular_labels(&regular_labels, &normalized_prefix);
    
    // 서버 라벨 처리
    if !server_labels.is_empty() {
        process_server_labels(&mut root, &server_labels, &normalized_prefix);
    }
    
    Value::Object(root)
}

/// 접두사 표준화 (끝에 점이 있는지 확인)
fn normalize_prefix(prefix: &str) -> String {
    if prefix.ends_with('.') {
        prefix.to_string()
    } else {
        format!("{}.", prefix)
    }
}

/// 접두사로 시작하는 라벨만 필터링
fn filter_labels_by_prefix(
    labels: &HashMap<String, String>, 
    prefix: &str
) -> HashMap<String, String> {
    labels.iter()
        .filter(|(key, _)| key.starts_with(prefix))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// 일반 라벨 처리
fn process_regular_labels(
    labels: &HashMap<String, String>,
    prefix: &str
) -> Map<String, Value> {
    labels.iter().fold(Map::new(), |mut root, (key, value)| {
        debug!("처리 중인 라벨: {}={}", key, value);
        
        // 라벨 키를 JSON 경로로 변환
        let (root_key, path) = label_key_to_json_path(key);
        if root_key.is_empty() {
            debug!("유효하지 않은 라벨 패턴 무시: {}", key);
            return root;
        }
        
        debug!("변환된 JSON 경로: 루트={}, 경로={:?}", root_key, path);
        
        // 값 변환 - 키도 함께 전달
        let converted_value = convert_value(value, key);
        debug!("변환된 값: {:?}", converted_value);
        
        // 루트 객체에 해당 타입 맵이 없으면 생성
        if !root.contains_key(&root_key) {
            root.insert(root_key.clone(), Value::Object(Map::new()));
            debug!("루트 객체 생성: {}", root_key);
        }
        
        // 경로를 따라 객체 트리 생성 및 값 설정
        let mut current = root.get_mut(&root_key).unwrap().as_object_mut().unwrap();
        
        for (i, segment) in path.iter().enumerate() {
            if i == path.len() - 1 {
                // 마지막 세그먼트는 값을 설정
                debug!("필드 설정: {} = {:?}", segment, converted_value);
                current.insert(segment.clone(), converted_value.clone());
            } else {
                // 중간 세그먼트는 객체 생성
                if !current.contains_key(segment) {
                    debug!("중간 객체 생성: {}", segment);
                    current.insert(segment.clone(), Value::Object(Map::new()));
                }
                current = current.get_mut(segment).unwrap().as_object_mut().unwrap();
            }
        }
        
        root
    })
}

/// 서버 라벨 처리
fn process_server_labels(
    root: &mut Map<String, Value>,
    server_labels: &HashMap<String, String>,
    prefix: &str
) {
    // 서비스별로 서버 속성 그룹화
    let mut grouped_servers: HashMap<String, HashMap<String, String>> = HashMap::new();
    
    for (key, value) in server_labels {
        // 서비스 이름과 서버 속성 추출
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() >= 7 {
            let service_name = parts[3].to_string();
            let server_property = parts[6].to_string();
            
            grouped_servers
                .entry(service_name)
                .or_insert_with(HashMap::new)
                .insert(server_property, value.clone());
        }
    }
    
    // services 객체가 없으면 생성
    if !root.contains_key("services") {
        root.insert("services".to_string(), Value::Object(Map::new()));
    }
    
    let services = root.get_mut("services").unwrap().as_object_mut().unwrap();
    
    // 각 서비스별로 서버 객체 생성
    for (service_name, server_props) in grouped_servers {
        // 해당 서비스가 없으면 생성
        if !services.contains_key(&service_name) {
            services.insert(service_name.clone(), Value::Object(Map::new()));
        }
        
        let service = services.get_mut(&service_name).unwrap().as_object_mut().unwrap();
        
        // loadbalancer가 없으면 생성
        if !service.contains_key("loadbalancer") {
            service.insert("loadbalancer".to_string(), Value::Object(Map::new()));
        }
        
        let loadbalancer = service.get_mut("loadbalancer").unwrap().as_object_mut().unwrap();
        
        // 서버 객체 생성
        let mut server_obj = Map::new();
        
        // URL 생성: port를 url로 변환
        if let Some(port) = server_props.get("port") {
            let url = format!("http://localhost:{}", port);
            server_obj.insert("url".to_string(), Value::String(url));
        }
        
        // weight 추가
        if let Some(weight) = server_props.get("weight") {
            if let Ok(w) = weight.parse::<u64>() {
                server_obj.insert("weight".to_string(), Value::Number(w.into()));
            }
        }
        
        // 기본 weight 추가 (없는 경우)
        if !server_obj.contains_key("weight") {
            server_obj.insert("weight".to_string(), Value::Number(1.into()));
        }
        
        // servers 배열에 추가
        let servers = vec![Value::Object(server_obj)];
        loadbalancer.insert("servers".to_string(), Value::Array(servers));
    }
}

/// JSON 객체를 도커 라벨 맵으로 변환 - 함수형 패러다임 적용 버전
pub fn json_to_labels(json: &Value, prefix: &str) -> HashMap<String, String> {
    // 접두사에 점이 있는지 확인하고 표준화
    let normalized_prefix = normalize_prefix(prefix);
    
    // 최종 결과를 저장할 해시맵
    let mut result = HashMap::new();
    
    if let Value::Object(root) = json {
        // 루트 객체의 각 항목 처리
        for (root_key, root_value) in root {
            if let Value::Object(items) = root_value {
                // 각 리소스 항목 처리
                process_resource_items(&mut result, &normalized_prefix, root_key, items);
            }
        }
    }
    
    result
}

/// 리소스 항목 처리 (middlewares, routers, services 등)
fn process_resource_items(
    result: &mut HashMap<String, String>,
    prefix: &str,
    resource_type: &str,
    items: &Map<String, Value>
) {
    for (item_key, item_value) in items {
        // 아이템이 객체인 경우에만 처리
        if let Some(obj) = item_value.as_object() {
            match resource_type {
                "middlewares" => process_middleware_item(result, prefix, item_key, obj),
                "services" => process_service_item(result, prefix, item_key, obj),
                _ => process_generic_item(result, prefix, resource_type, item_key, obj)
            }
        }
    }
}

/// 미들웨어 항목 처리
fn process_middleware_item(
    result: &mut HashMap<String, String>,
    prefix: &str,
    middleware_name: &str,
    obj: &Map<String, Value>
) {
    // middleware_type 필드 처리
    if let Some(Value::String(typ)) = obj.get("middleware_type") {
        // 미들웨어 타입 라벨 추가
        let type_key = format!("{}middlewares.{}.type", prefix, middleware_name);
        result.insert(type_key, typ.clone());
        
        // 미들웨어 타입에 따른 설정 키 결정
        let middleware_type = get_middleware_type_key(typ);
        
        // settings 필드 처리
        if let Some(Value::Object(settings)) = obj.get("settings") {
            for (setting_key, setting_val) in settings {
                let field_key = convert_key_to_camel_case(setting_key);
                let setting_path = format!(
                    "{}middlewares.{}.{}.{}", 
                    prefix, middleware_name, middleware_type, field_key
                );
                
                // 값을 라벨에 적합한 형식으로 변환하여 추가
                add_value_to_result(result, &setting_path, setting_val);
            }
        }
    }
    
    // 미들웨어 외 다른 필드 처리
    for (field_key, field_val) in obj {
        if field_key != "middleware_type" && field_key != "settings" {
            let key_str = convert_key_to_camel_case(field_key);
            let field_path = format!("{}middlewares.{}.{}", prefix, middleware_name, key_str);
            
            // 값을 라벨에 적합한 형식으로 변환하여 추가
            add_value_to_result(result, &field_path, field_val);
        }
    }
}

/// 서비스 항목 처리
fn process_service_item(
    result: &mut HashMap<String, String>,
    prefix: &str,
    service_name: &str,
    obj: &Map<String, Value>
) {
    for (field_key, field_val) in obj {
        // loadbalancer 필드 처리
        if field_key == "loadbalancer" && field_val.is_object() {
            if let Some(lb_obj) = field_val.as_object() {
                process_loadbalancer_field(result, prefix, service_name, lb_obj);
            }
        } else {
            // 기타 서비스 필드 처리
            let field_path = format!("{}services.{}.{}", prefix, service_name, field_key);
            add_value_to_result(result, &field_path, field_val);
        }
    }
}

/// 로드밸런서 필드 처리
fn process_loadbalancer_field(
    result: &mut HashMap<String, String>,
    prefix: &str,
    service_name: &str,
    lb_obj: &Map<String, Value>
) {
    for (lb_key, lb_val) in lb_obj {
        // servers 배열 처리
        if lb_key == "servers" && lb_val.is_array() {
            if let Some(servers) = lb_val.as_array() {
                servers.iter().enumerate().for_each(|(idx, server_val)| {
                    if let Some(server) = server_val.as_object() {
                        // url 처리
                        if let Some(Value::String(url)) = server.get("url") {
                            let url_path = format!(
                                "{}services.{}.loadbalancer.servers.{}.url", 
                                prefix, service_name, idx
                            );
                            result.insert(url_path, url.clone());
                        }
                        
                        // weight 처리
                        if let Some(weight) = server.get("weight") {
                            if let Some(w) = weight.as_u64() {
                                let weight_path = format!(
                                    "{}services.{}.loadbalancer.servers.{}.weight", 
                                    prefix, service_name, idx
                                );
                                result.insert(weight_path, w.to_string());
                            }
                        }
                    }
                });
            }
        } else {
            // 기타 loadbalancer 필드 처리
            let lb_path = format!(
                "{}services.{}.loadbalancer.{}", 
                prefix, service_name, lb_key
            );
            add_value_to_result(result, &lb_path, lb_val);
        }
    }
}

/// 일반 항목 처리 (미들웨어나 서비스가 아닌 경우)
fn process_generic_item(
    result: &mut HashMap<String, String>,
    prefix: &str,
    resource_type: &str,
    item_name: &str,
    obj: &Map<String, Value>
) {
    // 객체의 각 필드를 라벨로 변환
    for (field_key, field_val) in obj {
        let label_key = format!("{}{}.{}.{}", prefix, resource_type, item_name, field_key);
        add_value_to_result(result, &label_key, field_val);
    }
}

/// 미들웨어 타입에 따른 설정 키 결정
fn get_middleware_type_key(middleware_type: &str) -> String {
    match middleware_type {
        "basic-auth" => "basicAuth".to_string(),
        "cors" => "cors".to_string(),
        "rate-limit" => "rateLimit".to_string(),
        "header" => "headers".to_string(),
        "strip-prefix" => "stripPrefix".to_string(),
        "add-prefix" => "addPrefix".to_string(),
        _ => "unknown".to_string()
    }
}

/// 키를 캐멀케이스로 변환 (필요한 경우)
fn convert_key_to_camel_case(key: &str) -> String {
    if key.contains('_') {
        to_camel_case(key)
    } else {
        key.to_string()
    }
}

/// JSON 값을 라벨 값으로 변환하여 결과에 추가
fn add_value_to_result(result: &mut HashMap<String, String>, key: &str, value: &Value) {
    match value {
        Value::String(s) => {
            result.insert(key.to_string(), s.clone());
        },
        Value::Bool(b) => {
            result.insert(key.to_string(), b.to_string());
        },
        Value::Number(n) => {
            result.insert(key.to_string(), n.to_string());
        },
        Value::Array(arr) => {
            // 배열을 쉼표로 구분된 문자열로 변환
            let values: Vec<String> = arr.iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => Some(v.to_string()),
                })
                .collect();
            result.insert(key.to_string(), values.join(","));
        },
        Value::Object(obj) => {
            // 중첩 객체 처리
            for (sub_key, sub_val) in obj {
                let nested_key = format!("{}.{}", key, sub_key);
                add_value_to_result(result, &nested_key, sub_val);
            }
        },
        Value::Null => {
            result.insert(key.to_string(), "".to_string());
        },
    }
}

/// URL에서 포트를 추출하는 헬퍼 함수
fn extract_port_from_url(url: &str) -> Option<u16> {
    // URL 구문 분석: http://example.com:8080 형식 지원
    let url_parts: Vec<&str> = url.split("://").collect();
    if url_parts.len() < 2 {
        return None;
    }
    
    let host_port: Vec<&str> = url_parts[1].split(':').collect();
    if host_port.len() < 2 {
        // 포트가 지정되지 않은 경우, 기본 포트 추론
        return match url_parts[0] {
            "http" => Some(80),
            "https" => Some(443),
            _ => Some(80), // 기본값
        };
    }
    
    // 포트 파싱 (path가 있는 경우 처리: 8080/path)
    let port_str = host_port[1].split('/').next().unwrap_or(host_port[1]);
    port_str.parse().ok()
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
        
        // type 필드는 그대로 유지 (serde 속성을 통해 변환됨)
        let (root, path) = label_key_to_json_path("rproxy.http.middlewares.auth.type");
        assert_eq!(root, "middlewares");
        assert_eq!(path, vec!["auth", "type"]);
        
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
        assert_eq!(cors.get("type").unwrap(), "cors");
        
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
    
    #[test]
    fn test_real_world_labels_to_json() {
        let mut labels = HashMap::new();
        
        // 실제 도커 컴포즈에서 사용된 라벨 예시
        labels.insert("rproxy.http.middlewares.api-cors.type".to_string(), "cors".to_string());
        labels.insert("rproxy.http.middlewares.api-cors.cors.allowOrigins".to_string(), 
                    "http://localhost:3000,https://example.com".to_string());
        labels.insert("rproxy.http.middlewares.api-cors.cors.allowMethods".to_string(), 
                    "GET,POST,PUT,DELETE,OPTIONS".to_string());
        labels.insert("rproxy.http.middlewares.api-cors.enabled".to_string(), "true".to_string());
        labels.insert("rproxy.http.routers.api.rule".to_string(), 
                    "Host(`test.localhost`) && PathPrefix(`/api`)".to_string());
        labels.insert("rproxy.http.routers.api.middlewares".to_string(), "api-cors".to_string());
        
        // JSON으로 변환
        let json = labels_to_json(&labels, "rproxy.http.");
        
        // JSON 결과 출력 (디버깅 용도)
        let json_str = serde_json::to_string_pretty(&json).unwrap();
        println!("변환된 JSON: {}", json_str);
        
        // 기본 구조 검증
        assert!(json.is_object());
        let json_obj = json.as_object().unwrap();
        
        // middlewares 검증
        assert!(json_obj.contains_key("middlewares"));
        let middlewares = json_obj.get("middlewares").unwrap().as_object().unwrap();
        assert!(middlewares.contains_key("api-cors"));
        
        // 미들웨어 타입 필드 이름 검증 - type 필드 이름 사용
        let cors = middlewares.get("api-cors").unwrap().as_object().unwrap();
        assert!(cors.contains_key("type"));
        assert_eq!(cors.get("type").unwrap().as_str().unwrap(), "cors");
        
        // enabled 필드 검증
        assert!(cors.contains_key("enabled"));
        assert_eq!(cors.get("enabled").unwrap().as_bool().unwrap(), true);
        
        // 미들웨어 설정 검증
        assert!(cors.contains_key("settings"));
        let settings = cors.get("settings").unwrap().as_object().unwrap();
        assert!(settings.contains_key("allow_origins"));
        assert_eq!(settings.get("allow_origins").unwrap().as_str().unwrap(), 
                 "http://localhost:3000,https://example.com");
        
        // 라우터 검증
        assert!(json_obj.contains_key("routers"));
        let routers = json_obj.get("routers").unwrap().as_object().unwrap();
        assert!(routers.contains_key("api"));
        
        // 라우터 속성 검증
        let api_router = routers.get("api").unwrap().as_object().unwrap();
        assert!(api_router.contains_key("rule"));
        assert!(api_router.contains_key("middlewares"));
        
        // 미들웨어 목록 검증
        let middlewares_list = api_router.get("middlewares").unwrap().as_array().unwrap();
        assert_eq!(middlewares_list.len(), 1);
        assert_eq!(middlewares_list[0].as_str().unwrap(), "api-cors");
    }
    
    #[test]
    fn test_loadbalancer_labels_to_json() {
        let mut labels = HashMap::new();
        
        // 로드밸런서 서버 설정 라벨 추가
        labels.insert("rproxy.http.services.web.loadbalancer.server.port".to_string(), "8080".to_string());
        labels.insert("rproxy.http.services.web.loadbalancer.server.weight".to_string(), "2".to_string());
        
        // JSON으로 변환
        let json = labels_to_json(&labels, "rproxy.");
        
        // 결과 검증
        assert!(json.is_object());
        let json_obj = json.as_object().unwrap();
        
        // 서비스 검증
        assert!(json_obj.contains_key("services"));
        let services = json_obj.get("services").unwrap().as_object().unwrap();
        assert!(services.contains_key("web"));
        
        // 로드밸런서 설정 검증
        let web = services.get("web").unwrap().as_object().unwrap();
        assert!(web.contains_key("loadbalancer"));
        
        let loadbalancer = web.get("loadbalancer").unwrap().as_object().unwrap();
        assert!(loadbalancer.contains_key("servers"));
        
        // servers 배열 검증
        let servers = loadbalancer.get("servers").unwrap().as_array().unwrap();
        assert_eq!(servers.len(), 1);
        
        // 첫 번째 서버 검증
        let server = servers[0].as_object().unwrap();
        assert!(server.contains_key("url"));
        assert!(server.contains_key("weight"));
        
        // URL 형식 검증
        assert_eq!(server.get("url").unwrap(), "http://localhost:8080");
        assert_eq!(server.get("weight").unwrap().as_u64().unwrap(), 2);
    }
    
    #[test]
    fn test_loadbalancer_json_to_labels() {
        // 테스트용 JSON 객체 생성
        let mut map = Map::new();
        let mut services = Map::new();
        let mut web_service = Map::new();
        let mut loadbalancer = Map::new();
        let mut servers = Vec::new();
        
        let mut server = Map::new();
        server.insert("url".to_string(), Value::String("http://localhost:8080".to_string()));
        server.insert("weight".to_string(), Value::Number(2.into()));
        servers.push(Value::Object(server));
        
        loadbalancer.insert("servers".to_string(), Value::Array(servers));
        web_service.insert("loadbalancer".to_string(), Value::Object(loadbalancer));
        services.insert("web".to_string(), Value::Object(web_service));
        map.insert("services".to_string(), Value::Object(services));
        
        let json = Value::Object(map);
        
        println!("JSON: {}", json);
        
        // JSON을 레이블로 변환
        let labels = json_to_labels(&json, "rproxy");
        
        println!("Labels: {:?}", labels);
        
        // 레이블에 필요한 키가 있는지 확인
        assert!(labels.contains_key("rproxy.services.web.loadbalancer.servers.0.url"));
        assert!(labels.contains_key("rproxy.services.web.loadbalancer.servers.0.weight"));
        
        // 값이 올바른지 확인
        assert_eq!(labels.get("rproxy.services.web.loadbalancer.servers.0.url").unwrap(), "http://localhost:8080");
        assert_eq!(labels.get("rproxy.services.web.loadbalancer.servers.0.weight").unwrap(), "2");
    }
    
    #[test]
    fn test_extract_port_from_url() {
        // 기본 HTTP URL
        assert_eq!(extract_port_from_url("http://example.com:8080"), Some(8080));
        
        // HTTPS URL
        assert_eq!(extract_port_from_url("https://secure.example.com:8443"), Some(8443));
        
        // 경로가 있는 URL
        assert_eq!(extract_port_from_url("http://example.com:8080/path"), Some(8080));
        
        // 포트가 없는 URL - 기본 포트 추론
        assert_eq!(extract_port_from_url("http://example.com"), Some(80));
        assert_eq!(extract_port_from_url("https://example.com"), Some(443));
        
        // 잘못된 형식의 URL
        assert_eq!(extract_port_from_url("invalid-url"), None);
        
        // 잘못된 포트 번호
        assert_eq!(extract_port_from_url("http://example.com:invalid"), None);
    }
} 