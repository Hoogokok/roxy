use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::Write;
use chrono::Local;
use anyhow::{Result, Context};
use roxy::settings::converter::labels_to_json;
use serde_json;
use std::collections::HashMap;

use crate::docker::{self, get_container_labels, get_all_running_containers, DockerClient, create_docker_client};

// 마이그레이션 상태를 나타내는 타입들
pub struct Uninitialized;
pub struct Initialized;
pub struct Validated;
pub struct Backed;
pub struct Migrated;

// 마이그레이션 컨텍스트
pub struct MigrationContext<State> {
    containers: Vec<String>,
    output_dir: PathBuf,
    backup_dir: PathBuf,
    log_file: PathBuf,
    prefix: String,
    filename_pattern: String,
    pretty: bool,
    fail_fast: bool,
    auto_apply: bool,
    docker_client: Option<Box<dyn DockerClient>>,
    _state: std::marker::PhantomData<State>,
}

impl MigrationContext<Uninitialized> {
    pub async fn new(
        containers: Option<Vec<String>>,
        all: bool,
        output_dir: PathBuf,
        backup_dir: Option<PathBuf>,
        log_file: Option<PathBuf>,
        prefix: String,
        filename_pattern: String,
        pretty: bool,
        fail_fast: bool,
        auto_apply: bool,
    ) -> Result<Self> {
        // Docker 클라이언트 생성
        let docker_client = Some(create_docker_client());

        // 컨테이너 목록 결정
        let containers = if all {
            get_all_running_containers().await?
        } else {
            containers.ok_or_else(|| anyhow::anyhow!("컨테이너 목록이나 --all 옵션이 필요합니다."))?
        };

        // 출력 디렉토리 생성
        fs::create_dir_all(&output_dir)?;

        // 백업 디렉토리 결정 및 생성
        let backup_dir = backup_dir.unwrap_or_else(|| output_dir.join("backups"));
        fs::create_dir_all(&backup_dir)?;

        // 로그 파일 경로 결정
        let log_file = log_file.unwrap_or_else(|| output_dir.join("migration.log"));

        Ok(Self {
            containers,
            output_dir,
            backup_dir,
            log_file,
            prefix,
            filename_pattern,
            pretty,
            fail_fast,
            auto_apply,
            docker_client,
            _state: std::marker::PhantomData,
        })
    }

    pub fn initialize(self) -> Result<MigrationContext<Initialized>> {
        // 로그 파일 초기화
        let mut log = File::create(&self.log_file)?;
        writeln!(log, "마이그레이션 시작: {}", Local::now())?;
        writeln!(log, "대상 컨테이너: {:?}", self.containers)?;

        Ok(MigrationContext {
            containers: self.containers,
            output_dir: self.output_dir,
            backup_dir: self.backup_dir,
            log_file: self.log_file,
            prefix: self.prefix,
            filename_pattern: self.filename_pattern,
            pretty: self.pretty,
            fail_fast: self.fail_fast,
            auto_apply: self.auto_apply,
            docker_client: self.docker_client,
            _state: std::marker::PhantomData,
        })
    }
}

impl MigrationContext<Initialized> {
    pub async fn validate(self) -> Result<MigrationContext<Validated>> {
        let mut log = fs::OpenOptions::new()
            .append(true)
            .open(&self.log_file)?;

        // 컨테이너 존재 여부 및 라벨 검증
        for container in &self.containers {
            match get_container_labels(container).await {
                Ok(_) => {
                    writeln!(log, "컨테이너 {} 검증 성공", container)?;
                }
                Err(e) => {
                    let msg = format!("컨테이너 {} 검증 실패: {}", container, e);
                    writeln!(log, "{}", msg)?;
                    if self.fail_fast {
                        return Err(anyhow::anyhow!(msg));
                    }
                }
            }
        }

        Ok(MigrationContext {
            containers: self.containers,
            output_dir: self.output_dir,
            backup_dir: self.backup_dir,
            log_file: self.log_file,
            prefix: self.prefix,
            filename_pattern: self.filename_pattern,
            pretty: self.pretty,
            fail_fast: self.fail_fast,
            auto_apply: self.auto_apply,
            docker_client: self.docker_client,
            _state: std::marker::PhantomData,
        })
    }
}

impl MigrationContext<Validated> {
    pub fn backup(self) -> Result<MigrationContext<Backed>> {
        let mut log = fs::OpenOptions::new()
            .append(true)
            .open(&self.log_file)?;

        // 기존 설정 파일 백업
        for container in &self.containers {
            let output_file = self.get_output_path(container);
            if output_file.exists() {
                let backup_file = self.get_backup_path(container);
                fs::copy(&output_file, &backup_file)
                    .with_context(|| format!("설정 파일 백업 실패: {}", container))?;
                writeln!(log, "설정 파일 백업 완료: {} -> {}", 
                    output_file.display(), backup_file.display())?;
            }
        }

        Ok(MigrationContext {
            containers: self.containers,
            output_dir: self.output_dir,
            backup_dir: self.backup_dir,
            log_file: self.log_file,
            prefix: self.prefix,
            filename_pattern: self.filename_pattern,
            pretty: self.pretty,
            fail_fast: self.fail_fast,
            auto_apply: self.auto_apply,
            docker_client: self.docker_client,
            _state: std::marker::PhantomData,
        })
    }

    fn get_output_path(&self, container: &str) -> PathBuf {
        self.output_dir.join(
            self.filename_pattern.replace("{}", container)
        )
    }

    fn get_backup_path(&self, container: &str) -> PathBuf {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        self.backup_dir.join(format!(
            "{}_{}",
            timestamp,
            self.filename_pattern.replace("{}", container)
        ))
    }
}

impl MigrationContext<Backed> {
    pub async fn migrate(self) -> Result<MigrationContext<Migrated>> {
        let mut log = fs::OpenOptions::new()
            .append(true)
            .open(&self.log_file)?;

        let mut success_count = 0;
        let mut fail_count = 0;

        // 각 컨테이너의 라벨을 JSON으로 변환
        for container in &self.containers {
            match self.migrate_container(container).await {
                Ok(_) => {
                    success_count += 1;
                    writeln!(log, "컨테이너 {} 마이그레이션 성공", container)?;
                }
                Err(e) => {
                    fail_count += 1;
                    let msg = format!("컨테이너 {} 마이그레이션 실패: {}", container, e);
                    writeln!(log, "{}", msg)?;
                    if self.fail_fast {
                        return Err(anyhow::anyhow!(msg));
                    }
                }
            }
        }

        writeln!(log, "\n마이그레이션 완료")?;
        writeln!(log, "성공: {}, 실패: {}", success_count, fail_count)?;
        writeln!(log, "종료 시간: {}", Local::now())?;

        Ok(MigrationContext {
            containers: self.containers,
            output_dir: self.output_dir,
            backup_dir: self.backup_dir,
            log_file: self.log_file,
            prefix: self.prefix,
            filename_pattern: self.filename_pattern,
            pretty: self.pretty,
            fail_fast: self.fail_fast,
            auto_apply: self.auto_apply,
            docker_client: self.docker_client,
            _state: std::marker::PhantomData,
        })
    }

    async fn migrate_container(&self, container: &str) -> Result<()> {
        // 라벨 가져오기
        let labels = if let Some(client) = &self.docker_client {
            client.get_container_labels(container).await?
        } else {
            get_container_labels(container).await?
        };

        // JSON으로 변환
        let json = labels_to_json(&labels, &self.prefix);
        let json_str = if self.pretty {
            serde_json::to_string_pretty(&json)?
        } else {
            serde_json::to_string(&json)?
        };

        // 파일로 저장
        let output_file = self.get_output_path(container);
        fs::write(&output_file, json_str)?;

        // 자동 적용이 활성화된 경우 설정 적용
        if self.auto_apply {
            // TODO: 설정 적용 로직 구현
        }

        Ok(())
    }

    fn get_output_path(&self, container: &str) -> PathBuf {
        self.output_dir.join(
            self.filename_pattern.replace("{}", container)
        )
    }
}

impl MigrationContext<Migrated> {
    pub fn get_summary(&self) -> Result<String> {
        Ok(fs::read_to_string(&self.log_file)?)
    }
}

// 마이그레이션 실행 함수
pub async fn execute(
    containers: Option<Vec<String>>,
    all: bool,
    output_dir: PathBuf,
    backup_dir: Option<PathBuf>,
    log_file: Option<PathBuf>,
    prefix: String,
    filename_pattern: String,
    pretty: bool,
    fail_fast: bool,
    auto_apply: bool,
) -> Result<()> {
    let context = MigrationContext::new(
        containers,
        all,
        output_dir,
        backup_dir,
        log_file,
        prefix,
        filename_pattern,
        pretty,
        fail_fast,
        auto_apply,
    ).await?;

    let summary = context
        .initialize()?
        .validate().await?
        .backup()?
        .migrate().await?
        .get_summary()?;

    println!("{}", summary);
    Ok(())
}

/// 예시 라벨 가져오기
pub fn get_example_labels() -> HashMap<String, String> {
    let mut labels = HashMap::new();
    
    // 미들웨어 예시
    labels.insert("roxy.http.middlewares.api-cors.type".to_string(), "cors".to_string());
    labels.insert("roxy.http.middlewares.api-cors.cors.allowOrigins".to_string(), 
                "http://localhost:3000,https://example.com".to_string());
    labels.insert("roxy.http.middlewares.api-cors.cors.allowMethods".to_string(), 
                "GET,POST,PUT,DELETE,OPTIONS".to_string());
    labels.insert("roxy.http.middlewares.api-cors.enabled".to_string(), "true".to_string());
    
    // 라우터 예시
    labels.insert("roxy.http.routers.api.rule".to_string(), 
                "Host(`test.localhost`) && PathPrefix(`/api`)".to_string());
    labels.insert("roxy.http.routers.api.middlewares".to_string(), "api-cors".to_string());
    labels.insert("roxy.http.routers.api.service".to_string(), "api".to_string());
    
    // 서비스 예시
    labels.insert("roxy.http.services.api.loadbalancer.server.port".to_string(), "80".to_string());
    
    labels
}

// 테스트 도우미 메서드
impl<State> MigrationContext<State> {
    pub fn with_test_docker_client(mut self, client: Box<dyn DockerClient>) -> Self {
        self.docker_client = Some(client);
        self
    }
}

// 단위 테스트
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_migrate_single_container() -> Result<()> {
        // 임시 디렉토리 생성
        let temp_dir = tempdir()?;
        let output_dir = temp_dir.path().to_path_buf();
        let backup_dir = temp_dir.path().join("backups");
        let log_file = temp_dir.path().join("migration.log");

        // Docker 라벨을 직접 가져와서 확인
        let container_id = "test-container";
        let labels = crate::docker::get_container_labels(container_id).await?;
        println!("원본 라벨: {:?}", labels);
        
        // 라벨에 basic-auth 관련 키가 있는지 확인
        assert!(labels.contains_key("roxy.http.middlewares.test-container-auth.type"));
        assert_eq!(labels.get("roxy.http.middlewares.test-container-auth.type").unwrap(), "basic-auth");

        // 직접 JSON 변환 테스트
        let json = roxy::settings::converter::labels_to_json(&labels, "roxy.http.");
        let json_str = serde_json::to_string_pretty(&json)?;
        println!("직접 변환한 JSON: {}", json_str);
        
        // 마이그레이션 실행
        execute(
            Some(vec![container_id.to_string()]),
            false,
            output_dir.clone(),
            Some(backup_dir.clone()),
            Some(log_file.clone()),
            "roxy.http.".to_string(),
            "{}.json".to_string(),
            true,
            true,
            false,
        ).await?;

        // 결과 검증
        assert!(output_dir.exists());
        assert!(backup_dir.exists());
        assert!(log_file.exists());
        
        let output_file = output_dir.join(format!("{}.json", container_id));
        assert!(output_file.exists());

        // 출력 파일 내용 검증
        let content = std::fs::read_to_string(&output_file)?;
        println!("출력 파일 내용: {}", content);
        
        // 문제 디버깅: 출력 파일과 직접 변환 결과 비교
        let file_json: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(file_json, json, "파일의 JSON과 직접 변환한 JSON이 다릅니다");
        
        // 로그 파일 내용 확인
        let log_content = std::fs::read_to_string(log_file)?;
        assert!(log_content.contains("마이그레이션 시작"));
        assert!(log_content.contains(container_id));
        
        Ok(())
    }

    #[tokio::test]
    async fn test_migrate_all_containers() -> Result<()> {
        let temp_dir = tempdir()?;
        let output_dir = temp_dir.path().to_path_buf();

        // 모든 컨테이너 마이그레이션
        execute(
            None,
            true,
            output_dir.clone(),
            None,
            None,
            "roxy.http.".to_string(),
            "{}.json".to_string(),
            true,
            false,
            false,
        ).await?;

        // 기본 디렉토리 구조 확인
        assert!(output_dir.exists());
        assert!(output_dir.join("backups").exists());
        assert!(output_dir.join("migration.log").exists());

        // 모의 컨테이너에 대한 파일 생성 확인
        assert!(output_dir.join("test-container.json").exists());
        assert!(output_dir.join("test-container2.json").exists());

        Ok(())
    }
} 