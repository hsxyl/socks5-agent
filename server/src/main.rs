use axum::{extract::State, routing::{get, post}, Json, Router};
use clap::{Parser, Subcommand};
use sqlx::sqlite::SqlitePool;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

mod client_handler;
mod db;
mod http_proxy;
mod session;
mod socks5;

use session::SessionManager;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct ConfigArgs {
    /// Address to bind for Edge Clients
    #[arg(short, long, env = "EDGE_BIND_ADDR", default_value = "0.0.0.0:8080")]
    edge_bind_addr: String,

    /// Address to bind for SOCKS5 proxy users
    #[arg(short, long, env = "SOCKS5_BIND_ADDR", default_value = "0.0.0.0:1080")]
    socks5_bind_addr: String,

    /// Address to bind for HTTP CONNECT proxy users
    #[arg(long, env = "HTTP_BIND_ADDR", default_value = "0.0.0.0:8081")]
    http_bind_addr: String,

    /// Database URL (e.g., sqlite://data.db)
    #[arg(long, env = "DATABASE_URL", default_value = "sqlite://data.db")]
    database_url: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Add a new Card Key (卡密)
    AddCard {
        #[arg(long)]
        balance: i64, // bytes
    },
    /// Add a third-party proxy
    AddProxy {
        #[arg(long)]
        address: String,
        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        password: Option<String>,
        #[arg(long, default_value = "SOCKS5")]
        ptype: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = ConfigArgs::parse();

    // Setup Database
    // If not in memory, ensure file exists
    if args.database_url.starts_with("sqlite:") && !args.database_url.starts_with("sqlite::memory:") {
        let path = args.database_url.strip_prefix("sqlite://").unwrap_or(&args.database_url[9..]);
        if !std::path::Path::new(path).exists() {
            std::fs::File::create(path)?;
        }
    }
    let db_pool = db::init_db(&args.database_url).await?;

    if let Some(cmd) = args.command {
        match cmd {
            Commands::AddCard { balance } => {
                let card_key = uuid::Uuid::new_v4().to_string();
                db::add_card(&db_pool, &card_key, balance).await?;
            }
            Commands::AddProxy { address, username, password, ptype } => {
                db::add_proxy(&db_pool, &address, username.as_deref(), password.as_deref(), &ptype).await?;
            }
        }
        return Ok(());
    }

    // Start Admin API Server
    let api_db_pool = db_pool.clone();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/api/clients", get(get_clients_handler))
            .route("/api/cards", get(get_cards_handler).post(create_card_handler))
            .route("/api/logs", get(get_logs_handler))
            .layer(CorsLayer::permissive())
            .with_state(api_db_pool);

        if let Ok(listener) = tokio::net::TcpListener::bind("0.0.0.0:3000").await {
            info!("Starting HTTP Admin API on 0.0.0.0:3000");
            let _ = axum::serve(listener, app).await;
        }
    });

    let session_manager = SessionManager::new();

    let session_manager_clone = session_manager.clone();
    let edge_bind_addr = args.edge_bind_addr.clone();
    let edge_db_pool = db_pool.clone();
    
    tokio::spawn(async move {
        if let Err(e) = client_handler::listen_for_clients(session_manager_clone, edge_db_pool, &edge_bind_addr).await {
            error!("Client listener error: {}", e);
        }
    });

    // Start HTTP Proxy Server
    info!("Starting HTTP CONNECT proxy on {}", args.http_bind_addr);
    let http_listener = TcpListener::bind(&args.http_bind_addr).await?;
    let http_session_manager = session_manager.clone();
    let http_db_pool = db_pool.clone();
    tokio::spawn(async move {
        loop {
            if let Ok((socket, addr)) = http_listener.accept().await {
                let session_manager_clone = http_session_manager.clone();
                let db_pool_clone = http_db_pool.clone();
                tokio::spawn(async move {
                    if let Err(e) = http_proxy::handle_http_client(socket, session_manager_clone, db_pool_clone).await {
                        error!("HTTP client {} error: {}", addr, e);
                    }
                });
            }
        }
    });

    info!("Starting SOCKS5 server on {}", args.socks5_bind_addr);
    let socks5_listener = TcpListener::bind(&args.socks5_bind_addr).await?;

    loop {
        let (socket, addr) = socks5_listener.accept().await?;
        let session_manager_clone = session_manager.clone();
        let db_pool_clone = db_pool.clone();
        tokio::spawn(async move {
            if let Err(e) = socks5::handle_socks5_client(socket, session_manager_clone, db_pool_clone).await {
                error!("SOCKS5 client {} error: {}", addr, e);
            }
        });
    }
}

async fn get_clients_handler(State(db_pool): State<SqlitePool>) -> Json<Vec<db::ClientInfo>> {
    let clients = db::get_all_clients(&db_pool).await.unwrap_or_default();
    Json(clients)
}

#[derive(serde::Deserialize)]
struct CreateCardReq {
    balance_gb: u32,
}

#[derive(serde::Serialize)]
struct CreateCardResp {
    card_key: String,
    balance: i64,
}

async fn get_cards_handler(State(db_pool): State<SqlitePool>) -> Json<Vec<db::CardInfo>> {
    let cards = db::get_all_cards(&db_pool).await.unwrap_or_default();
    Json(cards)
}

async fn get_logs_handler(State(db_pool): State<SqlitePool>) -> Json<Vec<db::ProxyLog>> {
    let logs = db::get_proxy_logs(&db_pool).await.unwrap_or_default();
    Json(logs)
}

async fn create_card_handler(
    State(db_pool): State<SqlitePool>,
    Json(payload): Json<CreateCardReq>,
) -> Result<Json<CreateCardResp>, axum::http::StatusCode> {
    let card_key = uuid::Uuid::new_v4().to_string();
    let balance_bytes = (payload.balance_gb as i64) * 1024 * 1024 * 1024;
    
    if let Err(e) = db::add_card(&db_pool, &card_key, balance_bytes).await {
        error!("Failed to create card: {}", e);
        return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    }
    
    Ok(Json(CreateCardResp {
        card_key,
        balance: balance_bytes,
    }))
}
