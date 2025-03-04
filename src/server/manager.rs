use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, warn, info, debug, instrument};
use crate::{
    docker::DockerManager, middleware::MiddlewareManager, routing_v2::RoutingTable, settings::{watcher::{ConfigEvent, ConfigWatcher}, JsonConfig, Settings}
};
use super::{
    handler::RequestHandler,
    listener::ServerListener,
    docker::DockerEventHandler,
    Result,
    error::Error,
};
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

/// Config file watcher settings
#[derive(Debug)]
struct WatcherConfig {
    enabled: bool,
    debounce_timeout: Duration,
    poll_interval: Duration,
    config_path: PathBuf,
}

pub struct ServerManager {
    pub config: Settings,
    pub docker_manager: DockerManager,
    pub routing_table: Arc<RwLock<RoutingTable>>,
    middleware_manager: MiddlewareManager,
    config_watcher: Option<ConfigWatcher>,
    shared_config: Option<Arc<RwLock<Settings>>>,
    shared_middleware_manager: Option<Arc<RwLock<MiddlewareManager>>>,
}

impl ServerManager {
    /// Create a new ServerManager with explicit dependencies
    /// 
    /// This is a pure function that takes all dependencies as parameters,
    /// following the dependency injection pattern.
    pub fn new(
        config: Settings,
        docker_manager: DockerManager,
        routing_table: Arc<RwLock<RoutingTable>>,
        middleware_manager: MiddlewareManager,
    ) -> Self {
        Self {
            config,
            docker_manager,
            routing_table,
            middleware_manager,
            config_watcher: None,
            shared_config: None,
            shared_middleware_manager: None,
        }
    }

    // Factory method for application use
    #[instrument(skip(settings), level = "debug", err)]
    pub async fn with_defaults(mut settings: Settings) -> Result<Self> {
        // 1. Initialize Docker manager
        let docker_manager = DockerManager::with_defaults(settings.docker.clone())
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to initialize Docker manager");
                e
            })?;

        // 2. Setup initial health checks
        if let Err(e) = docker_manager.setup_initial_health_checks().await {
            error!(error = %e, "Failed to setup initial health checks");
        }

        // 3. Merge config sources (env vars, JSON, Docker labels)
        if let Ok(labels) = docker_manager.get_container_labels().await {
            // Merge all config sources
            settings.merge_all_config_sources(&labels).await?;
        } else {
            // If Docker labels not available, load JSON config from env only
            settings.load_json_from_env().await?;
        }

        // 4. Initialize routing table
        let routing_table = Arc::new(RwLock::new(RoutingTable::new()));
        
        // 5. Setup initial routes
        let initial_routes = docker_manager.get_container_routes().await?;
        
        {
            let mut table = routing_table.write().await;
            table.sync_docker_routes(initial_routes);
        }

        // 6. Initialize middleware manager
        let middleware_manager = MiddlewareManager::new(&settings.middleware, &settings.router_middlewares);

        Ok(Self::new(
            settings,
            docker_manager,
            routing_table,
            middleware_manager,
        ))
    }

    /// Get config watcher settings from environment variables
    fn get_watcher_config_from_env() -> WatcherConfig {
        // Check if watcher is enabled
        let enabled = env::var("PROXY_CONFIG_WATCH_ENABLED")
            .map(|val| val.to_lowercase() != "false")
            .unwrap_or(true);
        
        // Debounce timeout setting
        let debounce_timeout_ms = env::var("PROXY_CONFIG_WATCH_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300);
        
        // Polling interval setting
        let poll_interval_ms = env::var("PROXY_CONFIG_WATCH_INTERVAL")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(200);
        
        // Config file path
        let config_path = env::var("PROXY_JSON_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let mut path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                path.push("config");
                path.push("config.json");
                path
            });
        
        WatcherConfig {
            enabled,
            debounce_timeout: Duration::from_millis(debounce_timeout_ms),
            poll_interval: Duration::from_millis(poll_interval_ms),
            config_path,
        }
    }

    /// Initialize file watcher
    #[instrument(level = "debug", err)]
    async fn initialize_watcher(config: &WatcherConfig) -> Result<ConfigWatcher> {
        let mut watcher = ConfigWatcher::new();
        watcher.add_path(&config.config_path);
        watcher.start_with_interval(config.poll_interval).await
            .map_err(|e| Error::ConfigWatchError(format!("Failed to start file watcher: {}", e)))?;
        Ok(watcher)
    }

    /// Log configuration events
    fn log_config_events(events: &[ConfigEvent]) {
        info!("Detected configuration file events: {} events", events.len());
        
        for event in events {
            match event {
                ConfigEvent::Created(path) => info!("Config file created: {}", path.display()),
                ConfigEvent::Modified(path) => info!("Config file modified: {}", path.display()),
                ConfigEvent::Deleted(path) => warn!("Config file deleted: {}", path.display()),
            }
        }
    }

    /// Classify events and create a list of files to process
    fn classify_events(events: Vec<ConfigEvent>) -> (Vec<PathBuf>, bool) {
        let mut files_to_process = Vec::new();
        let mut has_deleted_files = false;
        
        for event in events {
            match event {
                ConfigEvent::Created(path) | ConfigEvent::Modified(path) => {
                    if !files_to_process.contains(&path) && path.exists() {
                        files_to_process.push(path);
                    }
                },
                ConfigEvent::Deleted(_) => {
                    has_deleted_files = true;
                }
            }
        }
        
        (files_to_process, has_deleted_files)
    }

    /// Update middleware manager from shared config
    #[instrument(skip(shared_config, shared_middleware_manager), level = "debug", err)]
    async fn update_middleware_manager(
        shared_config: &Arc<RwLock<Settings>>,
        shared_middleware_manager: &Arc<RwLock<MiddlewareManager>>
    ) -> Result<()> {
        let config = shared_config.read().await;
        let mut middleware_lock = shared_middleware_manager.write().await;
        *middleware_lock = MiddlewareManager::new(
            &config.middleware,
            &config.router_middlewares
        );
        
        debug!("Middleware manager updated successfully");
        Ok(())
    }

    /// Load JSON configuration from file and validate it
    #[instrument(level = "debug", err)]
    async fn load_and_validate_json_config(path: &Path) -> Result<JsonConfig> {
        // Load JsonConfig
        let json_config = JsonConfig::from_file(path).await
            .map_err(|e| Error::ConfigError(format!("Failed to load config file: {}: {}", path.display(), e)))?;
        
        info!("JSON config loaded: {}", path.display());
        
        // Validate configuration
        if let Err(e) = json_config.validate() {
            return Err(Error::ConfigError(format!("Config validation failed: {}: {}", path.display(), e)));
        }
        
        Ok(json_config)
    }
    
    /// Update middleware settings
    fn update_middleware_settings(
        config_lock: &mut Settings,
        json_config: &JsonConfig,
        config_id: &str
    ) -> bool {
        let mut updated = false;
        
        // Update middleware settings
        for (name, middleware_config) in &json_config.middlewares {
            let full_name = if name.contains('.') {
                name.clone()
            } else {
                format!("{}.{}", config_id, name)
            };
            
            debug!("Updating middleware: {}, config: {:?}", full_name, middleware_config.settings);
            if let Some(settings) = &middleware_config.settings.get("users") {
                debug!("Middleware users settings value: {}", settings);
            }
            
            // Remove existing config and replace with new one
            config_lock.middleware.remove(&full_name);
            config_lock.middleware.insert(full_name, middleware_config.clone());
            updated = true;
        }
        
        updated
    }
    
    /// Update router-middleware mappings
    fn update_router_middleware_mappings(
        config_lock: &mut Settings,
        json_config: &JsonConfig,
        config_id: &str
    ) -> bool {
        let mut updated = false;
        
        // Update router-middleware mappings
        for (router_name, router_config) in &json_config.routers {
            if let Some(middlewares) = &router_config.middlewares {
                let full_name = if router_name.contains('.') {
                    router_name.clone()
                } else {
                    format!("{}.{}", config_id, router_name)
                };
                
                config_lock.router_middlewares.insert(full_name, middlewares.clone());
                updated = true;
            }
        }
        
        updated
    }
    
    /// Validate middleware manager and handle rollback if needed
    fn validate_middleware_manager(
        config_lock: &mut Settings,
        config_backup: &Settings,
        config_updated: bool
    ) -> bool {
        if !config_updated {
            return false;
        }
        
        // Try to update middleware manager with new settings
        let new_middleware_manager = MiddlewareManager::new(
            &config_lock.middleware,
            &config_lock.router_middlewares
        );
        
        // Check if rollback is needed
        if let Err(e) = new_middleware_manager.validate() {
            error!("Middleware manager update failed, rolling back: {}", e);
            
            // Rollback: restore settings from backup
            *config_lock = config_backup.clone();
            return false;
        }
        
        true
    }

    /// Process a single configuration file
    #[instrument(skip(shared_config), level = "debug", err, fields(path = %path.display()))]
    async fn process_config_file(
        path: &Path, 
        shared_config: &Arc<RwLock<Settings>>
    ) -> Result<bool> {
        info!("Processing config file: {}", path.display());
        
        // Load and validate JSON configuration
        let json_config = Self::load_and_validate_json_config(path).await?;
        
        // Extract configuration ID
        let config_id = json_config.get_id(path);
        debug!("Config ID: {}", config_id);
        
        // Update shared configuration
        let mut config_updated = false;
        {
            // Create backup for rollback
            let config_backup = {
                let config_lock = shared_config.read().await;
                config_lock.clone()
            };
            
            let mut config_lock = shared_config.write().await;
            
            // Update middleware settings
            let middleware_updated = Self::update_middleware_settings(&mut config_lock, &json_config, &config_id);
            
            // Update router-middleware mappings
            let router_updated = Self::update_router_middleware_mappings(&mut config_lock, &json_config, &config_id);
            
            // Check if configuration was updated
            config_updated = middleware_updated || router_updated;
            
            // Validate middleware manager and handle rollback
            if config_updated {
                config_updated = Self::validate_middleware_manager(&mut config_lock, &config_backup, config_updated);
            }
        }
        
        Ok(config_updated)
    }

    /// Process multiple configuration files
    #[instrument(skip(paths, shared_config, shared_middleware_manager), level = "debug", err, 
                fields(file_count = paths.len()))]
    async fn process_config_files(
        paths: Vec<PathBuf>,
        shared_config: Arc<RwLock<Settings>>,
        shared_middleware_manager: Arc<RwLock<MiddlewareManager>>
    ) -> Result<bool> {
        let mut configs_updated = false;
        
        // Process all changed files
        for path in paths {
            match Self::process_config_file(&path, &shared_config).await {
                Ok(updated) => {
                    if updated {
                        configs_updated = true;
                    }
                },
                Err(e) => {
                    error!("{}", e.to_string());
                }
            }
        }
        
        // If configuration was updated, update middleware manager as well
        if configs_updated {
            Self::update_middleware_manager(&shared_config, &shared_middleware_manager).await?;
        }
        
        Ok(configs_updated)
    }

    /// Send config update notification
    #[instrument(skip(tx), level = "debug", err, fields(updated = updated))]
    async fn send_config_update_notification(
        tx: &mpsc::Sender<()>, 
        updated: bool
    ) -> Result<()> {
        if updated {
            // Config change notification
            debug!("Sending config update notification");
            tx.send(()).await
                .map_err(|e| Error::ConfigWatchError(format!("Failed to send config update notification: {}", e)))?;
            
            debug!("Config update notification sent successfully");
            info!("Configuration reload completed");
        } else {
            debug!("No valid configuration changes, skipping notification");
        }
        
        Ok(())
    }

    /// Start config file watcher
    #[instrument(skip(self), level = "debug", err)]
    pub async fn start_config_watcher(&mut self) -> Result<(tokio::sync::mpsc::Receiver<()>, tokio::task::JoinHandle<()>)> {
        // Get config from environment variables
        let watcher_config = Self::get_watcher_config_from_env();
        
        if !watcher_config.enabled {
            return Err(Error::ConfigWatchError("Config file watching is disabled".to_string()));
        }

        // Check if file exists
        if !watcher_config.config_path.exists() {
            return Err(Error::ConfigError(format!("Config file not found: {}", watcher_config.config_path.display())));
        }
        
        info!("Starting config file watcher");
        
        // Initialize file watcher
        let mut watcher = Self::initialize_watcher(&watcher_config).await?;
        
        // Config change notification channel
        let (notify_tx, notify_rx) = tokio::sync::mpsc::channel(1);
        
        // Create shared config and middleware manager
        let shared_config = Arc::new(RwLock::new(self.config.clone()));
        let shared_middleware_manager = Arc::new(RwLock::new(self.middleware_manager.clone()));
        
        // Store shared config in ServerManager
        self.shared_config = Some(shared_config.clone());
        self.shared_middleware_manager = Some(shared_middleware_manager.clone());

        // Transfer ownership of ConfigWatcher
        self.config_watcher = None;
        
        // Start config watching task
        let handle = tokio::spawn(async move {
            info!("Config watch task started (debounce timeout: {}ms, polling interval: {}ms)", 
                  watcher_config.debounce_timeout.as_millis(), 
                  watcher_config.poll_interval.as_millis());
            
            while let Some(events) = watcher.watch_debounced(watcher_config.debounce_timeout).await {
                // Log events
                ServerManager::log_config_events(&events);
                
                // Classify events
                let (files_to_process, has_deleted_files) = ServerManager::classify_events(events);
                
                if has_deleted_files {
                    warn!("Some config files were deleted. No special handling implemented for this case.");
                }
                
                // Process config files
                if !files_to_process.is_empty() {
                    // Process config files and handle data flow
                    let should_notify = match ServerManager::process_config_files(
                        files_to_process, 
                        shared_config.clone(), 
                        shared_middleware_manager.clone()
                    ).await {
                        Ok(updated) => updated,
                        Err(e) => {
                            error!("Failed to process config file: {}", e.to_string());
                            false
                        }
                    };
                    
                    // Separate data processing from async call
                    if should_notify {
                        if let Err(e) = ServerManager::send_config_update_notification(&notify_tx, true).await {
                            error!("Failed to send notification: {}", e.to_string());
                        }
                    }
                }
            }
            
            info!("Config watch task terminated");
        });
        
        Ok((notify_rx, handle))
    }

    /// Run server
    #[instrument(skip(self), level = "info", err)]
    pub async fn run(mut self) -> Result<()> {
        // Start config file watcher
        if let Err(e) = self.start_config_watcher().await {
            error!("Failed to start config watcher: {}", e);
        }

        // Subscribe to Docker events
        let mut event_rx = self.docker_manager.subscribe_to_events().await;
        let event_handler = DockerEventHandler::new(
            self.routing_table.clone(),
            Arc::new(RwLock::new(self.middleware_manager.clone())),
        );

        // Start Docker event handling task
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Err(e) = event_handler.handle_event(event).await {
                    error!("Event handling error: {}", e);
                }
            }
            warn!("Docker event stream ended");
        });

        // Create listener
        let listener = ServerListener::new(&self.config).await?;
        
        // Create RequestHandler
        let handler = Arc::new(RequestHandler::new(
            self.routing_table,
            self.middleware_manager,
        ));

        // Run listener
        listener.run(handler).await
    }
} 

