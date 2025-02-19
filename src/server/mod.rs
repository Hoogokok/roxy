pub mod handler;
pub mod listener;
pub mod docker;
pub mod error;

pub type Result<T> = std::result::Result<T, Error>;

mod manager;

use error::Error;
pub use manager::ServerManager; 