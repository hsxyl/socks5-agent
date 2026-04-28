use crate::ConfigArgs;
use common::{io_ext::write_json, protocol::{ControlMessage, ProxyRequest}};
use futures::StreamExt;
use tokio::net::TcpStream;
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::{error, info};
use yamux::{Config, Connection, Mode};

pub struct EdgeClient {
    args: ConfigArgs,
}

impl EdgeClient {
    pub fn new(args: ConfigArgs) -> Self {
        Self { args }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Connecting to Server at {}", self.args.server_addr);
        
        let socket = TcpStream::connect(&self.args.server_addr).await?;
        info!("Connected to server");

        let config = Config::default();
        let connection = Connection::new(socket.compat(), config, Mode::Client);
        let mut control = connection.control();
        
        let conn_task = tokio::spawn(async move {
            let mut conn_stream = Box::pin(yamux::into_stream(connection));
            while let Some(stream_res) = conn_stream.next().await {
                match stream_res {
                    Ok(stream) => {
                        tokio::spawn(Self::handle_proxy_stream(stream.compat()));
                    }
                    Err(e) => {
                        error!("Yamux stream error: {}", e);
                        break;
                    }
                }
            }
        });

        let control_stream = control.open_stream().await?;
        let mut control_stream_compat = control_stream.compat();
        
        self.register_device(&mut control_stream_compat).await?;
        self.spawn_heartbeat_task(control_stream_compat);

        let _ = conn_task.await;

        info!("Disconnected from server");
        Ok(())
    }

    async fn register_device<W: tokio::io::AsyncWriteExt + Unpin>(&self, stream: &mut W) -> std::io::Result<()> {
        write_json(
            stream,
            &ControlMessage::Register {
                device_id: self.args.device_id.clone(),
            },
        )
        .await?;
        info!("Sent Register for device {}", self.args.device_id);
        Ok(())
    }

    fn spawn_heartbeat_task<W: tokio::io::AsyncWriteExt + Unpin + Send + 'static>(&self, mut stream: W) {
        let interval = self.args.heartbeat_interval;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                let msg = ControlMessage::Heartbeat {
                    cpu_usage: 0,
                    mem_usage: 0,
                };
                if let Err(e) = write_json(&mut stream, &msg).await {
                    error!("Failed to send heartbeat: {}", e);
                    break;
                }
            }
        });
    }

    async fn handle_proxy_stream(mut stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin) {
        let req = match ProxyRequest::read_from(&mut stream).await {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to read proxy request: {}", e);
                return;
            }
        };

        info!("Proxying connection to {}", req.target);

        match TcpStream::connect(&req.target).await {
            Ok(mut target_socket) => {
                info!("Connected to target {}", req.target);
                if let Err(e) = tokio::io::copy_bidirectional(&mut stream, &mut target_socket).await {
                    info!("Proxy copy finished for {}: {}", req.target, e);
                }
            }
            Err(e) => {
                error!("Failed to connect to target {}: {}", req.target, e);
            }
        }
    }
}
