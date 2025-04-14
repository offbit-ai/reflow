//! Example of using the database connection pool

use anyhow::Result;
use reflow_script::{db_manager::get_db_pool_manager, db_pool::SQLiteConnection};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Get the global DB pool manager
    let pool_manager = get_db_pool_manager();
    
    // Create a SQLite connection
    let connection_id = "example_db";
    let db_path = ":memory:"; // In-memory database for this example
    let sqlite_conn = SQLiteConnection::new(connection_id, db_path);
    
    // Register the connection with the pool manager
    pool_manager.register_connection(sqlite_conn).await?;
    
    // Create a table
    let create_table = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)";
    pool_manager.execute_command(connection_id, create_table, vec![]).await?;
    
    // Insert some data
    let insert = "INSERT INTO users (name, email) VALUES (?, ?)";
    
    // Insert first user
    let params1 = vec![
        json!("Alice"),
        json!("alice@example.com"),
    ];
    pool_manager.execute_command(connection_id, insert, params1).await?;
    
    // Insert second user
    let params2 = vec![
        json!("Bob"),
        json!("bob@example.com"),
    ];
    pool_manager.execute_command(connection_id, insert, params2).await?;
    
    // Query the data
    let query = "SELECT * FROM users";
    let results = pool_manager.execute_query(connection_id, query, vec![]).await?;
    
    // Display the results
    println!("Query results:");
    for row in results {
        println!("User: id={}, name={}, email={}", 
            row["id"], row["name"], row["email"]);
    }
    
    // Check connection health
    pool_manager.check_connections_health().await?;
    
    // Clean up
    pool_manager.remove_connection(connection_id).await?;
    Ok(())
}