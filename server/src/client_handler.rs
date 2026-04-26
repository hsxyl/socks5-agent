use crate::session::SessionManager;
use common::{io_ext::read_json, protocol::ControlMessage};
use futures::StreamExt;
use tokio::net::TcpListener;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{error, info, warn};
use yamux::{Config, Connection, Mode};

pub async fn listen_for_clients(session_manager: SessionManager, bind_addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    info!("Listening for Edge Clients on {}", bind_addr);

    loop {
        let (socket, addr) = listener.accept().await?;
        info!("New Edge Client connected from {}", addr);
        let session_manager = session_manager.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_single_client(socket, session_manager).await {
                error!("Error handling client from {}: {}", addr, e);
            }
        });
    }
}

async fn handle_single_client(socket: tokio::net::TcpStream, session_manager: SessionManager) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::default();
    let connection = Connection::new(socket.compat(), config, Mode::Server);
    let control = connection.control();
    let mut conn_stream = Box::pin(yamux::into_stream(connection));

    // Client must initiate the first stream as the Control Stream
    if let Some(stream_res) = conn_stream.next().await {
        let mut stream = stream_res?.compat();
        match read_json::<_, ControlMessage>(&mut stream).await? {
            ControlMessage::Register { device_id } => {
                info!("Device {} registered", device_id);
                session_manager.register(device_id.clone(), control);

                // Wait until the connection is dropped
                while let Some(_) = conn_stream.next().await {}

                info!("Device {} disconnected", device_id);
                session_manager.unregister(&device_id);
            }
            _ => warn!("Invalid first message from client, expected Register"),
        }
    }
    
    Ok(())
}
