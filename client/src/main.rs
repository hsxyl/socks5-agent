use clap::Parser;

mod engine;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct ConfigArgs {
    /// Server address to connect to
    #[arg(short, long, env = "SERVER_ADDR", default_value = "127.0.0.1:8080")]
    server_addr: String,

    /// Unique identifier for this device
    #[arg(short, long, env = "DEVICE_ID", default_value = "box-12345")]
    device_id: String,

    /// Heartbeat interval in seconds
    #[arg(long, env = "HEARTBEAT_INTERVAL", default_value_t = 30)]
    heartbeat_interval: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = ConfigArgs::parse();
    
    let client = engine::EdgeClient::new(args);
    client.run().await?;

    Ok(())
}
