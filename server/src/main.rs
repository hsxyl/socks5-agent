use clap::Parser;
use tokio::net::TcpListener;
use tracing::{error, info};

mod client_handler;
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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = ConfigArgs::parse();
    let session_manager = SessionManager::new();

    let session_manager_clone = session_manager.clone();
    let edge_bind_addr = args.edge_bind_addr.clone();
    
    tokio::spawn(async move {
        if let Err(e) = client_handler::listen_for_clients(session_manager_clone, &edge_bind_addr).await {
            error!("Client listener error: {}", e);
        }
    });

    info!("Starting SOCKS5 server on {}", args.socks5_bind_addr);
    let socks5_listener = TcpListener::bind(&args.socks5_bind_addr).await?;

    loop {
        let (socket, addr) = socks5_listener.accept().await?;
        let session_manager_clone = session_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = socks5::handle_socks5_client(socket, session_manager_clone).await {
                error!("SOCKS5 client {} error: {}", addr, e);
            }
        });
    }
}
