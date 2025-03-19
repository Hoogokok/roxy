use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use std::fs;

use crate::settings::core::{Settings, Result};
use crate::settings::json::JsonConfig;
use crate::settings::error::SettingsError;
use crate::settings::types::ValidMiddlewareId;
use crate::settings::typestate::TypeState;
use crate::settings::types::ValidPort;


/// 설정 병합 모듈
/// 
/// 이 모듈은 다양한 소스의 설정을 병합하는 기능을 담당합니다.
impl<State: TypeState, HttpsState> Settings<State, HttpsState> {
    /// Docker 라벨에서 설정 병합
    pub fn merge_docker_labels(&mut self, labels: &HashMap<String, String>) -> Result<()> {
        debug!("Docker 라벨에서 설정 병합");
        
        // 라우터-미들웨어 매핑 파싱
        let parsed_router_middlewares = Self::parse_router_middlewares(labels);
        
        // 라우터-미들웨어 매핑 병합
        for (router, middlewares) in parsed_router_middlewares {
            self.router_middlewares.insert(router, middlewares);
        }
        
        Ok(())
    }
    
    /// 라우터-미들웨어 매핑 파싱
    fn parse_router_middlewares(labels: &HashMap<String, String>) -> HashMap<String, Vec<ValidMiddlewareId>> {
        let mut router_middlewares = HashMap::new();
        
        for (key, value) in labels {
            if let Some((router, middlewares)) = Self::extract_router_middleware(key, value) {
                router_middlewares.insert(router, middlewares);
            }
        }
        
        router_middlewares
    }
    
    /// 라우터-미들웨어 맵핑 추출
    fn extract_router_middleware(key: &String, value: &String) -> Option<(String, Vec<ValidMiddlewareId>)> {
        // 라우터-미들웨어 맵핑 라벨 형식: rproxy.http.routers.{router-name}.middlewares
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() == 5 && parts[0] == "rproxy" && parts[1] == "http" && parts[2] == "routers" && parts[4] == "middlewares" {
            let router_name = parts[3].to_string();
            
            // 미들웨어 ID 파싱
            let middleware_ids = value.split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .filter_map(|s| ValidMiddlewareId::new(s))
                .collect();
                
            return Some((router_name, middleware_ids));
        }
        
        None
    }
    
    /// JSON 설정 병합 (덮어쓰기 옵션 포함)
    pub async fn load_json_config_with_override<P: AsRef<Path>>(&mut self, path: P, override_existing: bool) -> Result<()> {
        let config = JsonConfig::from_file(path)?;
        self.merge_with_json_config(&config, override_existing)
    }
    
    /// JSON 설정 병합 (기본적으로 덮어쓰지 않음)
    pub async fn load_json_config<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.load_json_config_with_override(path, false).await
    }
    
    /// 디렉토리에서 JSON 설정 로드
    pub async fn load_config_directory<P: AsRef<Path>>(&mut self, dir_path: P) -> Result<()> {
        let dir_path = dir_path.as_ref();
        debug!("디렉토리에서 설정 로드: {:?}", dir_path);
        
        if !dir_path.exists() {
            return Err(SettingsError::FileNotFound {
                path: dir_path.to_string_lossy().to_string(),
                context: "설정 디렉토리".to_string(),
            });
        }
        
        if !dir_path.is_dir() {
            return Err(SettingsError::InvalidConfig(
                format!("{:?}는 디렉토리가 아닙니다", dir_path)
            ));
        }
        
        let entries = fs::read_dir(dir_path).map_err(|e| SettingsError::FileError {
            path: dir_path.to_string_lossy().to_string(),
            error: e,
        })?;
        
        for entry in entries {
            let entry = entry.map_err(|e| SettingsError::FileError {
                path: dir_path.to_string_lossy().to_string(),
                error: e,
            })?;
            
            let path = entry.path();
            if path.is_file() {
                let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if extension == "json" {
                    debug!("설정 파일 로드: {:?}", path);
                    self.load_json_config(&path).await?;
                }
            }
        }
        
        Ok(())
    }
    
    /// 환경 변수에서 JSON 설정 로드
    pub async fn load_json_from_env(&mut self) -> Result<()> {
        debug!("환경 변수에서 JSON 설정 로드");
        
        if let Ok(json_str) = std::env::var("PROXY_CONFIG_JSON") {
            if !json_str.is_empty() {
                debug!("환경 변수 PROXY_CONFIG_JSON에서 설정 로드");
                let config = JsonConfig::from_file_strongly_typed(json_str)?;
                return self.merge_with_json_config(&config, true);
            }
        }
        
        Ok(())
    }
    
    /// Docker 라벨에서 JSON 설정 로드
    pub async fn load_json_from_labels(&mut self, labels: &HashMap<String, String>) -> Result<()> {
        debug!("Docker 라벨에서 JSON 설정 로드");
        
        // rproxy.http. 접두사를 가진 라벨 필터링
        let filtered_labels: HashMap<String, String> = labels.iter()
            .filter(|(k, _)| k.starts_with("rproxy.http."))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
            
        if !filtered_labels.is_empty() {
            debug!("Docker 라벨에서 설정 로드: {} 개", filtered_labels.len());
            let config = JsonConfig::from_docker_labels(&filtered_labels, "rproxy.http.");
            return self.merge_with_json_config(&config, true);
        }
        
        Ok(())
    }
    
    /// 모든 설정 소스 병합
    pub async fn merge_all_config_sources(&mut self, labels: &HashMap<String, String>) -> Result<()> {
        // 1. 기본 설정 디렉토리에서 설정 로드
        let config_dir = std::env::var("PROXY_CONFIG_DIR").unwrap_or_else(|_| "/etc/rproxy".to_string());
        let config_dir_path = PathBuf::from(config_dir);
        
        if config_dir_path.exists() && config_dir_path.is_dir() {
            self.load_config_directory(&config_dir_path).await?;
        }
        
        // 2. 환경 변수에서 JSON 설정 로드
        self.load_json_from_env().await?;
        
        // 3. Docker 라벨에서 설정 로드
        self.load_json_from_labels(labels).await?;
        
        // 4. 라우터-미들웨어 매핑 병합
        self.merge_docker_labels(labels)?;
        
        Ok(())
    }
    
    /// JSON 설정 병합
    pub fn merge_with_json_config(&mut self, config: &JsonConfig, override_existing: bool) -> Result<()> {
        debug!("JSON 설정 병합 (덮어쓰기: {})", override_existing);
        
        // 설정 유효성 검증
        let mut config_clone = config.clone();
        config_clone.validate()?;
        
        // 미들웨어 설정 병합
        for (name, middleware) in &config.middlewares {
            self.add_middleware_with_override(name.clone(), middleware.clone(), override_existing)?;
        }
        
        // 라우터-미들웨어 매핑 병합
        for (router, middlewares) in &config.router_middlewares {
            let middleware_ids = middlewares.iter()
                .filter_map(|id| ValidMiddlewareId::new(id))
                .collect::<Vec<_>>();
                
            if !middleware_ids.is_empty() {
                if override_existing || !self.router_middlewares.contains_key(router) {
                    self.router_middlewares.insert(router.clone(), middleware_ids);
                }
            }
        }
        
        // JSON 파일에서 추가 설정 적용
        self.apply_json_file_settings(config, override_existing)?;
        
        Ok(())
    }
    
    /// JSON 파일에서 추가 설정 적용
    fn apply_json_file_settings(&mut self, config: &JsonConfig, override_existing: bool) -> Result<()> {
        // 소스 파일이 없으면 건너뜀
        let path = match &config.source_path {
            Some(path) if path.exists() => path,
            _ => return Ok(()),
        };
        
        // JSON 파일 내용 읽기
        let file_content = std::fs::read_to_string(path)
            .map_err(|e| SettingsError::FileError { 
                path: path.to_string_lossy().to_string(),
                error: e 
            })?;
        
        // JSON 파싱
        let json: serde_json::Value = serde_json::from_str(&file_content)
            .map_err(|e| SettingsError::JsonParseError { source: e })?;
        
        // 서버 설정 적용
        if let Some(server) = json.get("server") {
            self.apply_server_settings(server, override_existing)?;
        }
        
        // 로깅 설정 적용
        if let Some(logging) = json.get("logging") {
            self.apply_logging_settings(logging, override_existing)?;
        }
        
        // 도커 설정 적용
        if let Some(docker) = json.get("docker") {
            self.apply_docker_settings(docker, override_existing)?;
        }
        
        Ok(())
    }
    
    /// 서버 설정 적용
    fn apply_server_settings(&mut self, server: &serde_json::Value, _override_existing: bool) -> Result<()> {
        debug!("서버 설정 적용: {:?}", server);
        
        // HTTP 포트 설정
        if let Some(http_port) = server.get("http_port") {
            if let Some(port) = http_port.as_u64() {
                debug!("HTTP 포트 설정: {}", port);
                if port > 0 && port <= 65535 {
                    if let Some(valid_port) = ValidPort::new(port as u16) {
                        self.server.http_port = valid_port;
                        debug!("HTTP 포트를 {}로 설정했습니다", port);
                    } else {
                        warn!("유효하지 않은 HTTP 포트: {}", port);
                    }
                } else {
                    warn!("범위를 벗어난 HTTP 포트: {}", port);
                }
            }
        }
        
        // HTTPS 설정 (HTTPS가 활성화된 경우에만 적용)
        if let Some(https_enabled) = server.get("https_enabled") {
            if https_enabled.as_bool() == Some(true) {
                debug!("HTTPS 설정 처리");
                
                // HTTPS 포트 설정
                if let Some(https_port) = server.get("https_port") {
                    if let Some(port) = https_port.as_u64() {
                        // HTTPS 포트 처리 로직은 타입 상태에 따라 다르게 구현해야 함
                        // 여기서는 기본 로깅만 수행
                        debug!("HTTPS 포트 설정: {}", port);
                    }
                }
                
                // TLS 인증서 및 키 경로 설정
                if let Some(tls_cert) = server.get("tls_cert_path") {
                    if let Some(cert_path) = tls_cert.as_str() {
                        debug!("TLS 인증서 경로 설정: {}", cert_path);
                        // 실제 설정 로직은 타입 상태에 따라 다름
                    }
                }
                
                if let Some(tls_key) = server.get("tls_key_path") {
                    if let Some(key_path) = tls_key.as_str() {
                        debug!("TLS 키 경로 설정: {}", key_path);
                        // 실제 설정 로직은 타입 상태에 따라 다름
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 로깅 설정 적용
    fn apply_logging_settings(&mut self, logging: &serde_json::Value, _override_existing: bool) -> Result<()> {
        debug!("로깅 설정 적용: {:?}", logging);
        
        // 로그 레벨 설정
        if let Some(level) = logging.get("level") {
            if let Some(level_str) = level.as_str() {
                debug!("로그 레벨 설정: {}", level_str);
                // 실제 로그 레벨 설정 로직
            }
        }
        
        // 로그 포맷 설정
        if let Some(format) = logging.get("format") {
            if let Some(format_str) = format.as_str() {
                debug!("로그 포맷 설정: {}", format_str);
                // 실제 로그 포맷 설정 로직
            }
        }
        
        // 로그 출력 설정
        if let Some(output) = logging.get("output") {
            if let Some(output_str) = output.as_str() {
                debug!("로그 출력 설정: {}", output_str);
                // 실제 로그 출력 설정 로직
            }
        }
        
        Ok(())
    }
    
    /// 도커 설정 적용
    fn apply_docker_settings(&mut self, docker: &serde_json::Value, _override_existing: bool) -> Result<()> {
        debug!("도커 설정 적용: {:?}", docker);
        
        // 네트워크 설정
        if let Some(network) = docker.get("network") {
            if let Some(network_str) = network.as_str() {
                debug!("도커 네트워크 설정: {}", network_str);
                // 실제 네트워크 설정 로직
            }
        }
        
        // 라벨 접두사 설정
        if let Some(label_prefix) = docker.get("label_prefix") {
            if let Some(prefix_str) = label_prefix.as_str() {
                debug!("도커 라벨 접두사 설정: {}", prefix_str);
                // 실제 라벨 접두사 설정 로직
            }
        }
        
        // 헬스체크 설정
        if let Some(health_check) = docker.get("health_check") {
            debug!("도커 헬스체크 설정: {:?}", health_check);
            
            // 활성화 여부
            if let Some(enabled) = health_check.get("enabled") {
                if let Some(enabled_bool) = enabled.as_bool() {
                    debug!("헬스체크 활성화 설정: {}", enabled_bool);
                    // 실제 헬스체크 활성화 설정 로직
                }
            }
            
            // 인터벌 설정
            if let Some(interval) = health_check.get("interval") {
                if let Some(interval_num) = interval.as_u64() {
                    debug!("헬스체크 인터벌 설정: {}초", interval_num);
                    // 실제 헬스체크 인터벌 설정 로직
                }
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::middleware::config::{MiddlewareConfig, MiddlewareType};
    use crate::settings::typestate::Validated;
    
    #[test]
    fn test_extract_router_middleware() {
        let key = "rproxy.http.routers.api.middlewares".to_string();
        let value = "cors,ratelimit".to_string();
        
        let result = Settings::<crate::settings::server::HttpsDisabled>::extract_router_middleware(&key, &value);
        assert!(result.is_some());
        
        let (router, middlewares) = result.unwrap();
        assert_eq!(router, "api");
        assert_eq!(middlewares.len(), 2);
        assert_eq!(middlewares[0].to_string(), "cors");
        assert_eq!(middlewares[1].to_string(), "ratelimit");
    }
    
    #[test]
    fn test_parse_router_middlewares() {
        let mut labels = HashMap::new();
        labels.insert("rproxy.http.routers.api.middlewares".to_string(), "cors,ratelimit".to_string());
        labels.insert("rproxy.http.routers.admin.middlewares".to_string(), "basicauth".to_string());
        
        let result = Settings::<crate::settings::server::HttpsDisabled>::parse_router_middlewares(&labels);
        assert_eq!(result.len(), 2);
        assert!(result.contains_key("api"));
        assert!(result.contains_key("admin"));
        assert_eq!(result["api"].len(), 2);
        assert_eq!(result["admin"].len(), 1);
    }
    
    #[test]
    fn test_merge_with_json_config() {
        let mut settings = Settings::<Validated, crate::settings::server::HttpsDisabled>::default();
        
        // 테스트용 JSON 설정 생성
        let mut config = JsonConfig::default();
        
        // 미들웨어 추가
        let middleware = MiddlewareConfig {
            middleware_type: MiddlewareType::BasicAuth,
            enabled: true,
            order: 0,
            settings: {
                let mut settings = HashMap::new();
                settings.insert("users".to_string(), "user:pass".to_string());
                settings
            },
        };
        
        config.middlewares.insert("auth".to_string(), middleware);
        
        // 라우터-미들웨어 매핑 추가
        config.router_middlewares.insert("api".to_string(), vec!["auth".to_string()]);
        
        // 설정 병합
        let result = settings.merge_with_json_config(&config, false);
        assert!(result.is_ok());
        
        // 결과 확인
        assert_eq!(settings.middleware.len(), 1);
        assert!(settings.middleware.contains_key("auth"));
        assert_eq!(settings.router_middlewares.len(), 1);
        assert!(settings.router_middlewares.contains_key("api"));
    }
} 