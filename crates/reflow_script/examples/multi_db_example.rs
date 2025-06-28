//! Example of using multiple database connections with the DB pool wrapper

use anyhow::Result;
use reflow_network::actor::{Actor, ActorConfig, ActorContext, ActorLoad};
use reflow_network::message::Message;
use reflow_script::db_actor::DatabaseActor;
use reflow_script::db_manager::get_db_pool_manager;
use reflow_script::db_pool::SQLiteConnection;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Get the global DB pool manager
    let pool_manager = get_db_pool_manager();

    // Create multiple SQLite connections
    let users_db_id = "users_db";
    let products_db_id = "products_db";

    // Create in-memory databases for this example
    let users_conn = SQLiteConnection::new(users_db_id, "users.db");
    let products_conn = SQLiteConnection::new(products_db_id, "products.db");

    // Register connections with the pool manager
    pool_manager.register_connection(users_conn).await?;
    pool_manager.register_connection(products_conn).await?;

    // Create tables in each database
    let create_users = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)";
    pool_manager
        .execute_command(users_db_id, create_users, vec![])
        .await?;

    let create_products = "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL)";
    pool_manager
        .execute_command(products_db_id, create_products, vec![])
        .await?;

    // Insert data into users database
    let insert_user = "INSERT INTO users (name, email) VALUES (?, ?)";

    // Insert users
    let users = vec![
        ("Alice", "alice@example.com"),
        ("Bob", "bob@example.com"),
        ("Charlie", "charlie@example.com"),
    ];

    for (name, email) in users {
        let params = vec![json!(name), json!(email)];
        pool_manager
            .execute_command(users_db_id, insert_user, params)
            .await?;
    }

    // Insert data into products database
    let insert_product = "INSERT INTO products (name, price) VALUES (?, ?)";

    // Insert products
    let products = vec![
        ("Laptop", 999.99),
        ("Phone", 699.99),
        ("Headphones", 149.99),
    ];

    for (name, price) in products {
        let params = vec![json!(name), json!(price)];
        pool_manager
            .execute_command(products_db_id, insert_product, params)
            .await?;
    }

    // Check connection health
    println!("\nChecking connection health...");
    pool_manager.check_connections_health().await?;

    // Get connection status
    let users_status = pool_manager.get_connection_status(users_db_id).await?;
    let products_status = pool_manager.get_connection_status(products_db_id).await?;

    println!("Users DB status: {:?}", users_status);
    println!("Products DB status: {:?}", products_status);

    // Demonstrate using the DatabaseActor
    println!("\nUsing DatabaseActor to query data:");

    // Create database actors for each database
    let users_actor = DatabaseActor::new("users_actor", users_db_id);
    let products_actor = DatabaseActor::new("products_actor", products_db_id);

    // Get behavior functions
    let users_behavior = users_actor.get_behavior();
    let products_behavior = products_actor.get_behavior();

    // Create state and ports
    let state = std::sync::Arc::new(parking_lot::Mutex::new(
        reflow_network::actor::MemoryState::default(),
    ));
    let users_outports = users_actor.get_outports();
    let products_outports = products_actor.get_outports();

    // Query users
    let mut users_query = HashMap::new();
    users_query.insert(
        "query".to_string(),
        Message::string("SELECT * FROM users".to_string()),
    );
    let actor_config = ActorConfig::default();
    let context = ActorContext::new(
        users_query,
        users_outports.clone(),
        state.clone(),
        actor_config,
        Arc::new(parking_lot::Mutex::new(ActorLoad::new(0))),
    );
    let users_result = users_behavior(context).await?;

    println!("\nUsers query result:");
    if let Message::Array(rows) = &users_result["rows"] {
        for row in rows.as_ref() {
            if let serde_json::Value::Object(user) = row.clone().into() {
                println!(
                    "User: id={}, name={}, email={}",
                    user["id"], user["name"], user["email"]
                );
            }
        }
    }

    // Query products
    let mut products_query = HashMap::new();
    products_query.insert(
        "query".to_string(),
        Message::string("SELECT * FROM products".to_string()),
    );
    let actor_config = ActorConfig::default();
    let context = ActorContext::new(
        products_query,
        products_outports.clone(),
        state.clone(),
        actor_config,
        Arc::new(parking_lot::Mutex::new(ActorLoad::new(0))),
    );
    let products_result = products_behavior(context).await?;

    println!("\nProducts query result:");
    if let Message::Array(rows) = &products_result["rows"] {
        for row in rows.as_ref() {
            if let serde_json::Value::Object(product) = row.clone().into() {
                println!(
                    "Product: id={}, name={}, price={}",
                    product["id"], product["name"], product["price"]
                );
            }
        }
    }

    // Demonstrate connection failure and reconnection
    println!("\nDemonstrating connection failure and reconnection:");

    // Simulate a connection failure by closing the connection
    if let Some(conn) = pool_manager.get_connection(users_db_id) {
        let mut conn_guard = conn.lock().await;
        conn_guard.close().await?;
        println!("Closed users_db connection to simulate failure");
    }

    // Check connection status after failure
    let users_status = pool_manager.get_connection_status(users_db_id).await?;
    println!("Users DB status after failure: {:?}", users_status);

    tokio::time::sleep(Duration::from_secs(60)).await;

    // Run health check to reconnect
    println!("Running health check to reconnect...");
    pool_manager.check_connections_health().await?;

    // Check connection status after reconnection
    let users_status = pool_manager.get_connection_status(users_db_id).await?;
    println!(
        "Users DB status after reconnection attempt: {:?}",
        users_status
    );

    // Try to query again after reconnection
    let mut users_query = HashMap::new();
    users_query.insert(
        "query".to_string(),
        Message::string("SELECT COUNT(*) as count FROM users".to_string()),
    );

    let actor_config = ActorConfig::default();

    let context = ActorContext::new(
        users_query,
        users_outports.clone(),
        state.clone(),
        actor_config,
        Arc::new(parking_lot::Mutex::new(ActorLoad::new(0))),
    );
    let users_result = users_behavior(context).await?;
    println!("\nUsers count after reconnection:");
    if let Message::Array(rows) = &users_result["rows"] {
        if let Some(row) = rows.first() {
            if let serde_json::Value::Object(count_row) = row.clone().into() {
                println!("Total users: {}", count_row["count"]);
            }
        }
    }

    // Clean up
    println!("\nCleaning up connections...");
    pool_manager.remove_connection(users_db_id).await?;
    pool_manager.remove_connection(products_db_id).await?;

    println!("Example completed successfully");
    Ok(())
}
