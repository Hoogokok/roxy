pub mod handler;
pub mod listener;
pub mod docker;
pub mod error;

pub type Result<T> = std::result::Result<T, Error>;

pub mod manager_v2;

use error::Error;
pub use manager_v2::ServerInterface; 