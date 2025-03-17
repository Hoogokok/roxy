use std::collections::HashMap;
use std::path::{Path, PathBuf};
use dashmap::DashMap;
use tokio::sync::mpsc;
use crate::settings::error::SettingsError;
use crate::settings::json::JsonConfig;

use super::Settings;

pub struct ContainerConfigManager {
    pub container_configs: DashMap<String, JsonConfig>,
    config_updated_tx: mpsc::Sender<String>,
}

impl ContainerConfigManager {
    pub fn new() -> (Self, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(100);
        
        (Self {
            container_configs: DashMap::new(),
            config_updated_tx: tx,
        }, rx)
    }
    
    pub async fn load_container_config(&self, container_id: String, path: &Path) -> Result<(), SettingsError> {
        // JSON 설정 로드
        let config = JsonConfig::from_file(path)?;
        
        // 컨테이너 ID와 설정 매핑 저장
        self.container_configs.insert(container_id.clone(), config);
        
        // 설정 변경 알림 전송
        if let Err(e) = self.config_updated_tx.send(container_id).await {
            tracing::warn!("설정 변경 알림 전송 실패: {}", e);
        }
        
        Ok(())
    }
    
    pub fn get_container_config(&self, container_id: &str) -> Option<JsonConfig> {
        self.container_configs.get(container_id).map(|r| r.value().clone())
    }
    
    pub fn merge_config(&self, container_id: &str, docker_labels: &HashMap<String, String>) -> Settings {
        let mut settings = Settings::default();
        
        // 1. 도커 라벨 설정 적용
        settings.merge_docker_labels(docker_labels).unwrap_or_else(|e| {
            tracing::warn!("도커 라벨 설정 병합 실패: {}", e);
        });
        
        // 2. JSON 설정 적용 (우선순위 높음)
        if let Some(json_config) = self.get_container_config(container_id) {
            settings.merge_with_json_config(&json_config, true).unwrap_or_else(|e| {
                tracing::warn!("JSON 설정 병합 실패: {}", e);
            });
        }
        
        settings
    }
}
