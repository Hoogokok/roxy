mod manager;
mod listener;
mod handler;

pub use manager::ServerManager;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>; 