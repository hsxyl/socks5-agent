use crate::session::SessionManager;
use common::{io_ext::read_json, protocol::ControlMessage};
use futures::StreamExt;
use sqlx::sqlite::SqlitePool;
use tokio::net::TcpListener;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{error, info, warn};
use yamux::{Config, Connection, Mode};

pub async fn listen_for_clients(session_manager: SessionManager, db_pool: SqlitePool, bind_addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    info!("Listening for Edge Clients on {}", bind_addr);

    loop {
        let (socket, addr) = listener.accept().await?;
        info!("New Edge Client connected from {}", addr);
        let session_manager = session_manager.clone();
        let db_pool = db_pool.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_single_client(socket, addr.to_string(), session_manager, db_pool).await {
                error!("Error handling client from {}: {}", addr, e);
            }
        });
    }
}

async fn handle_single_client(
    socket: tokio::net::TcpStream,
    ip_address: String,
    session_manager: SessionManager,
    db_pool: SqlitePool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::default();
    let connection = Connection::new(socket.compat(), config, Mode::Server);
    let control = connection.control();
    
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    let conn_task = tokio::spawn(async move {
        let mut conn_stream = Box::pin(yamux::into_stream(connection));
        let mut is_first = true;
        while let Some(stream_res) = conn_stream.next().await {
            if is_first {
                is_first = false;
                if let Ok(stream) = stream_res {
                    let _ = tx.send(stream).await;
                }
            } else {
                warn!("Client opened unexpected stream");
            }
        }
    });

    // Client must initiate the first stream as the Control Stream
    if let Some(stream) = rx.recv().await {
        let mut stream = stream.compat();
        match read_json::<_, ControlMessage>(&mut stream).await {
            Ok(ControlMessage::Register { device_id }) => {
                info!("Device {} registered", device_id);
                session_manager.register(device_id.clone(), control);

                if let Err(e) = crate::db::update_client_status(&db_pool, &device_id, &ip_address, "online").await {
                    error!("Failed to update client status: {}", e);
                }

                // Spawn a task to read heartbeats from the control stream
                let db_pool_clone = db_pool.clone();
                let device_id_clone = device_id.clone();
                tokio::spawn(async move {
                    loop {
                        match read_json::<_, ControlMessage>(&mut stream).await {
                            Ok(ControlMessage::Heartbeat { .. }) => {
                                if let Err(e) = crate::db::update_client_heartbeat(&db_pool_clone, &device_id_clone).await {
                                    error!("Failed to update client heartbeat: {}", e);
                                }
                            }
                            _ => break, // Error or connection closed
                        }
                    }
                });

                // Wait until the connection is dropped
                let _ = conn_task.await;

                info!("Device {} disconnected", device_id);
                session_manager.unregister(&device_id);
                if let Err(e) = crate::db::mark_client_offline(&db_pool, &device_id).await {
                    error!("Failed to mark client offline: {}", e);
                }
            }
            _ => warn!("Invalid first message from client, expected Register"),
        }
    } else {
        warn!("Client disconnected before opening control stream");
    }
    
    Ok(())
}
