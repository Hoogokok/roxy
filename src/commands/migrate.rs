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
}

#[cfg(test)]
impl MigrationContext<Uninitialized> {
    pub fn with_test_docker_client(mut self, client: Box<dyn DockerClient>) -> Self {
        self.docker_client = Some(client);
        self
    }
} 