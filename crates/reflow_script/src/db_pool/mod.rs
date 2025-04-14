//! Database connection pool module
//! 
//! This module provides a database connection pool manager that can manage
//! multiple database connections, check their health, and reconnect if necessary.

mod sqlite;
#[cfg(feature = "postgres")]
mod postgres;

pub use self::sqlite::SQLiteConnection;
#[cfg(feature = "postgres")]
pub use self::postgres::PostgresConnection;

// Re-export the main types from the parent module
pub use crate::db_manager::{ConnectionStatus, DatabaseConfig, DatabaseConnection, DatabaseType, DbPoolManager, get_db_pool_manager};