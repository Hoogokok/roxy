use std::path::PathBuf;
use tokio::sync::mpsc;
use crate::settings::Result;

/// 설정 파일 변경 이벤트 타입
#[derive(Debug)]
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
}

impl ConfigWatcher {
    /// 새로운 ConfigWatcher 인스턴스 생성
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(32);
        Self {
            paths: Vec::new(),
            event_tx,
            event_rx,
        }
    }

    /// 감시할 경로 추가
    pub fn add_path<P: Into<PathBuf>>(&mut self, path: P) {
        self.paths.push(path.into());
    }

    /// 감시 시작
    pub async fn start(&mut self) -> Result<()> {
        // TODO: 실제 파일 시스템 감시 구현
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