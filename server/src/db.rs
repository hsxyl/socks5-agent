use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Error;
use tracing::info;

pub async fn init_db(db_url: &str) -> Result<SqlitePool, Error> {
    info!("Initializing database at {}", db_url);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await?;

    // Create tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS cards (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            card_key TEXT NOT NULL UNIQUE,
            balance BIGINT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS proxies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            address TEXT NOT NULL,
            username TEXT,
            password TEXT,
            proxy_type TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active'
        );
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS clients (
            device_id TEXT PRIMARY KEY,
            ip_address TEXT NOT NULL,
            status TEXT NOT NULL,
            connected_at DATETIME,
            last_heartbeat DATETIME,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS proxy_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            card_key TEXT NOT NULL,
            target TEXT NOT NULL,
            device_id TEXT,
            protocol TEXT NOT NULL,
            bytes_used BIGINT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

pub async fn check_balance(pool: &SqlitePool, card_key: &str) -> Result<bool, Error> {
    let card: Option<(i64,)> = sqlx::query_as(
        "SELECT balance FROM cards WHERE card_key = ?"
    )
    .bind(card_key)
    .fetch_optional(pool)
    .await?;

    if let Some((balance,)) = card {
        if balance > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

pub async fn deduct_balance(pool: &SqlitePool, card_key: &str, bytes_used: u64) -> Result<(), Error> {
    sqlx::query("UPDATE cards SET balance = MAX(0, balance - ?) WHERE card_key = ?")
        .bind(bytes_used as i64)
        .bind(card_key)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn add_card(pool: &SqlitePool, card_key: &str, balance: i64) -> Result<(), Error> {
    sqlx::query("INSERT INTO cards (card_key, balance) VALUES (?, ?)")
        .bind(card_key)
        .bind(balance)
        .execute(pool)
        .await?;
    info!("Card key created: {} (Balance: {} bytes)", card_key, balance);
    Ok(())
}

pub async fn add_proxy(pool: &SqlitePool, address: &str, username: Option<&str>, password: Option<&str>, ptype: &str) -> Result<(), Error> {
    sqlx::query("INSERT INTO proxies (address, username, password, proxy_type) VALUES (?, ?, ?, ?)")
        .bind(address)
        .bind(username)
        .bind(password)
        .bind(ptype)
        .execute(pool)
        .await?;
    info!("Proxy added: {} ({})", address, ptype);
    Ok(())
}

#[derive(Debug)]
pub struct ProxyInfo {
    pub address: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub proxy_type: String,
}

pub async fn get_available_third_party_proxy(pool: &SqlitePool) -> Result<Option<ProxyInfo>, Error> {
    let proxy = sqlx::query_as::<_, (String, Option<String>, Option<String>, String)>(
        "SELECT address, username, password, proxy_type FROM proxies WHERE status = 'active' ORDER BY RANDOM() LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;

    Ok(proxy.map(|(address, username, password, proxy_type)| ProxyInfo {
        address,
        username,
        password,
        proxy_type,
    }))
}

pub async fn update_client_status(
    pool: &SqlitePool,
    device_id: &str,
    ip_address: &str,
    status: &str,
) -> Result<(), Error> {
    sqlx::query(
        r#"
        INSERT INTO clients (device_id, ip_address, status, connected_at, last_heartbeat)
        VALUES (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(device_id) DO UPDATE SET
            ip_address = excluded.ip_address,
            status = excluded.status,
            connected_at = CURRENT_TIMESTAMP,
            last_heartbeat = CURRENT_TIMESTAMP
        "#,
    )
    .bind(device_id)
    .bind(ip_address)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_client_heartbeat(pool: &SqlitePool, device_id: &str) -> Result<(), Error> {
    sqlx::query("UPDATE clients SET last_heartbeat = CURRENT_TIMESTAMP WHERE device_id = ?")
        .bind(device_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_client_offline(pool: &SqlitePool, device_id: &str) -> Result<(), Error> {
    sqlx::query("UPDATE clients SET status = 'offline' WHERE device_id = ?")
        .bind(device_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct ClientInfo {
    pub device_id: String,
    pub ip_address: String,
    pub status: String,
    pub connected_at: Option<String>,
    pub last_heartbeat: Option<String>,
    pub created_at: String,
}

pub async fn get_all_clients(pool: &SqlitePool) -> Result<Vec<ClientInfo>, Error> {
    let clients = sqlx::query_as::<_, ClientInfo>(
        "SELECT device_id, ip_address, status, CAST(connected_at as TEXT) as connected_at, CAST(last_heartbeat as TEXT) as last_heartbeat, CAST(created_at as TEXT) as created_at FROM clients ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(clients)
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct CardInfo {
    pub card_key: String,
    pub balance: i64,
    pub created_at: String,
}

pub async fn get_all_cards(pool: &SqlitePool) -> Result<Vec<CardInfo>, Error> {
    let cards = sqlx::query_as::<_, CardInfo>(
        "SELECT card_key, balance, CAST(created_at as TEXT) as created_at FROM cards ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(cards)
}

pub async fn add_proxy_log(
    pool: &SqlitePool,
    card_key: &str,
    target: &str,
    device_id: Option<&str>,
    protocol: &str,
    bytes_used: u64,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO proxy_logs (card_key, target, device_id, protocol, bytes_used) VALUES (?, ?, ?, ?, ?)")
        .bind(card_key)
        .bind(target)
        .bind(device_id)
        .bind(protocol)
        .bind(bytes_used as i64)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct ProxyLog {
    pub id: i64,
    pub card_key: String,
    pub target: String,
    pub device_id: Option<String>,
    pub protocol: String,
    pub bytes_used: i64,
    pub created_at: String,
}

pub async fn get_proxy_logs(pool: &SqlitePool) -> Result<Vec<ProxyLog>, Error> {
    let logs = sqlx::query_as::<_, ProxyLog>(
        "SELECT id, card_key, target, device_id, protocol, bytes_used, CAST(created_at as TEXT) as created_at FROM proxy_logs ORDER BY created_at DESC LIMIT 1000"
    )
    .fetch_all(pool)
    .await?;
    Ok(logs)
}
