use std::path::PathBuf;
use tokio::sync::mpsc;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config, Event, Result as NotifyResult};
use crate::settings::{Result, error::SettingsError};
use tracing::{debug, error};

/// 설정 파일 변경 이벤트 타입
#[derive(Debug, PartialEq, Clone)]
pub enum ConfigEvent {
    /// 파일이 수정됨
    Modified(PathBuf),
    /// 파일이 생성됨
    Created(PathBuf),
    /// 파일이 삭제됨
    Deleted(PathBuf),
}

/// 설정 파일 감시자
pub struct ConfigWatcher {
    /// 감시할 파일/디렉토리 경로 목록
    paths: Vec<PathBuf>,
    /// 이벤트 송신자
    event_tx: mpsc::Sender<ConfigEvent>,
    /// 이벤트 수신자
    event_rx: mpsc::Receiver<ConfigEvent>,
    /// 파일 시스템 감시자
    watcher: Option<RecommendedWatcher>,
}

impl ConfigWatcher {
    /// 새로운 ConfigWatcher 인스턴스 생성
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(32);
        Self {
            paths: Vec::new(),
            event_tx,
            event_rx,
            watcher: None,
        }
    }

    /// 감시할 경로 추가
    pub fn add_path<P: Into<PathBuf>>(&mut self, path: P) {
        self.paths.push(path.into());
    }

    /// 테스트용 이벤트 송신자 반환
    #[cfg(test)]
    pub fn get_sender(&self) -> mpsc::Sender<ConfigEvent> {
        self.event_tx.clone()
    }

    /// 감시 시작
    pub async fn start(&mut self) -> Result<()> {
        let event_tx = self.event_tx.clone();
        
        // notify의 이벤트를 ConfigEvent로 변환하여 채널로 전송하는 핸들러
        let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res: NotifyResult<Event>| {
            match res {
                Ok(event) => {
                    use notify::EventKind::*;
                    
                    for path in event.paths {
                        match event.kind {
                            Modify(_) => {
                                debug!("파일 수정됨: {}", path.display());
                                let _ = event_tx.blocking_send(ConfigEvent::Modified(path));
                            },
                            Create(_) => {
                                debug!("파일 생성됨: {}", path.display());
                                let _ = event_tx.blocking_send(ConfigEvent::Created(path));
                            },
                            Remove(_) => {
                                debug!("파일 삭제됨: {}", path.display());
                                let _ = event_tx.blocking_send(ConfigEvent::Deleted(path));
                            },
                            _ => {}
                        }
                    }
                }
                Err(e) => error!("감시 오류: {}", e),
            }
        }).map_err(|e| SettingsError::WatchError(e.to_string()))?;

        // 모든 경로에 대해 감시 설정
        for path in &self.paths {
            debug!("경로 감시 시작: {}", path.display());
            watcher.watch(path, RecursiveMode::Recursive)
                .map_err(|e| SettingsError::WatchError(e.to_string()))?;
        }

        self.watcher = Some(watcher);
        Ok(())
    }

    /// 이벤트 수신 대기
    pub async fn watch(&mut self) -> Option<ConfigEvent> {
        self.event_rx.recv().await
    }
}

impl Default for ConfigWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn test_config_watcher() {
        let mut watcher = ConfigWatcher::new();
        let tx = watcher.get_sender();
        let test_path = Path::new("/test/config.json").to_path_buf();

        // 이벤트 전송
        tx.send(ConfigEvent::Created(test_path.clone())).await.unwrap();
        
        // 이벤트 수신 확인
        if let Some(event) = watcher.watch().await {
            assert_eq!(event, ConfigEvent::Created(test_path));
        } else {
            panic!("이벤트를 받지 못했습니다");
        }
    }

    #[tokio::test]
    async fn test_multiple_events() {
        let mut watcher = ConfigWatcher::new();
        let tx = watcher.get_sender();
        let test_path = Path::new("/test/config.json").to_path_buf();

        // 여러 이벤트 전송
        let events = vec![
            ConfigEvent::Created(test_path.clone()),
            ConfigEvent::Modified(test_path.clone()),
            ConfigEvent::Deleted(test_path.clone()),
        ];

        for event in events.clone() {
            tx.send(event).await.unwrap();
        }

        // 모든 이벤트 수신 확인
        for expected_event in events {
            match watcher.watch().await {
                Some(event) => assert_eq!(event, expected_event),
                None => panic!("이벤트를 받지 못했습니다"),
            }
        }
    }
} 