use std::path::Path;
use std::collections::HashMap;
use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::settings::error::SettingsError;
use crate::settings::json::JsonConfig;
use crate::settings::core::Settings;
use crate::settings::typestate::Validated;

/// 컨테이너별 설정 관리자
/// 
/// 각 컨테이너의 JSON 설정 파일을 관리하고, 설정 병합 및 변경 알림을 처리합니다.
/// 성능 최적화된 구현입니다.
pub struct ContainerConfigManager {
    /// 컨테이너 ID → JSON 설정 맵핑 (DashMap 사용으로 세밀한 잠금 제공)
    pub container_configs: DashMap<String, JsonConfig>,
    /// 설정 변경 알림을 위한 송신 채널
    config_updated_tx: mpsc::Sender<String>,
    /// 공유 기본 설정
    shared_config: Settings<Validated>,
}

impl ContainerConfigManager {
    /// 새 설정 관리자 생성
    pub fn new() -> (Self, mpsc::Receiver<String>) {
        // 설정 변경 알림용 채널 생성 - 더 큰 버퍼 사용
        let (tx, rx) = mpsc::channel(1000);
        
        let manager = Self {
            container_configs: DashMap::new(),
            config_updated_tx: tx,
            shared_config: Settings::default(),
        };
        
        (manager, rx)
    }

    /// 공유 설정 설정
    pub fn with_shared_config(mut self, config: Settings<Validated>) -> Self {
        self.shared_config = config;
        self
    }
    
    /// 컨테이너 설정 파일 로드 - 성능 최적화 버전
    pub async fn load_container_config(&self, container_id: String, path: &Path) -> Result<(), SettingsError> {
        info!("컨테이너 {} 설정 파일 로드: {}", container_id, path.display());
        
        // JSON 설정 로드
        let config = JsonConfig::from_file(path)?;
        
        // 컨테이너 ID와 설정 매핑 저장
        // DashMap을 통한 원자적 업데이트 (이전 값이 있으면 덮어씀)
        self.container_configs.insert(container_id.clone(), config);
        
        // 설정 변경 알림 전송 - 비동기적으로 처리하되 오류는 로그만 남김
        if let Err(e) = self.config_updated_tx.try_send(container_id.clone()) {
            // 채널이 가득 차거나 닫혔을 때만 경고
            warn!("설정 변경 알림 전송 실패 (try_send): {}", e);
            
            // 비동기 전송 재시도 - 100ms 타임아웃 설정
            if let Err(e) = tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                self.config_updated_tx.send(container_id.clone())
            ).await {
                warn!("설정 변경 알림 전송 타임아웃: {}", e);
            }
        }
        
        info!("컨테이너 {} 설정 로드 완료", container_id);
        Ok(())
    }
    
    /// 컨테이너 설정 조회 - 참조 반환으로 최적화
    pub fn get_container_config(&self, container_id: &str) -> Option<JsonConfig> {
        // DashMap의 get() 대신 get_ref()를 사용하여 불필요한 복제를 방지
        self.container_configs.get(container_id)
            .map(|ref_multi| ref_multi.clone())
    }
    
    /// 컨테이너 설정 병합 - 최적화 버전
    /// 
    /// 1. 공유 설정을 기반으로 시작
    /// 2. 도커 라벨 설정 적용 (낮은 우선순위)
    /// 3. JSON 설정 적용 (높은 우선순위)
    pub fn merge_config(&self, container_id: &str, docker_labels: &HashMap<String, String>) -> Settings<Validated> {
        // 1. 공유 설정 복제
        let mut settings = self.shared_config.clone();
        
        // 2. 도커 라벨 설정 적용 (낮은 우선순위)
        if let Err(e) = settings.merge_docker_labels(docker_labels) {
            warn!("도커 라벨 설정 병합 실패: {}", e);
        }
        
        // 3. JSON 설정 적용 (높은 우선순위)
        // 최적화: 직접 get() 사용하고 불필요한 복제 최소화
        if let Some(json_config) = self.container_configs.get(container_id) {
            if let Err(e) = settings.merge_with_json_config(&json_config, true) {
                warn!("JSON 설정 병합 실패: {}", e);
            }
        }
        
        settings
    }
    
    /// 컨테이너별 설정을 일괄 조회 - 벌크 작업 최적화
    pub fn get_all_container_configs(&self) -> Vec<(String, JsonConfig)> {
        self.container_configs.iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }
    
    /// 여러 컨테이너의 설정을 병합 (벌크 작업 최적화)
    pub fn merge_configs_batch(&self, container_ids: &[&str], docker_labels: &HashMap<String, String>) -> HashMap<String, Settings<Validated>> {
        let mut results = HashMap::with_capacity(container_ids.len());
        
        for &id in container_ids {
            results.insert(id.to_string(), self.merge_config(id, docker_labels));
        }
        
        results
    }
}
