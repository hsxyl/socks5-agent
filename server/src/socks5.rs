use crate::session::SessionManager;
use common::protocol::ProxyRequest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{error, info};

pub async fn handle_socks5_client(
    mut socket: TcpStream,
    session_manager: SessionManager,
) -> std::io::Result<()> {
    if handshake(&mut socket).await.is_err() {
        return Ok(());
    }

    let (addr, port) = match read_request(&mut socket).await {
        Ok(req) => req,
        Err(e) => {
            error!("SOCKS5 request error: {}", e);
            return Ok(());
        }
    };

    info!("Proxy request to {}:{}", addr, port);

    if let Some(mut control) = session_manager.get_available_control() {
        match control.open_stream().await {
            Ok(stream) => {
                let mut stream = stream.compat();
                let proxy_req = ProxyRequest {
                    target: format!("{}:{}", addr, port),
                };
                
                if let Err(e) = proxy_req.write_to(&mut stream).await {
                    error!("Failed to write proxy request to edge client: {}", e);
                    return Ok(());
                }

                // SOCKS5 response Success
                socket.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
                tokio::io::copy_bidirectional(&mut socket, &mut stream).await?;
            }
            Err(e) => {
                error!("Failed to open stream to Edge Client: {}", e);
                socket.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?; // Host unreachable
            }
        }
    } else {
        error!("No Edge Clients available");
        socket.write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?; // Network unreachable
    }

    Ok(())
}

async fn handshake(socket: &mut TcpStream) -> std::io::Result<()> {
    let mut buf = [0u8; 2];
    socket.read_exact(&mut buf).await?;
    if buf[0] != 0x05 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Not SOCKS5"));
    }
    let nmethods = buf[1] as usize;
    let mut methods = vec![0u8; nmethods];
    socket.read_exact(&mut methods).await?;
    socket.write_all(&[0x05, 0x00]).await?;
    Ok(())
}

async fn read_request(socket: &mut TcpStream) -> std::io::Result<(String, u16)> {
    let mut req_header = [0u8; 4];
    socket.read_exact(&mut req_header).await?;
    if req_header[0] != 0x05 || req_header[1] != 0x01 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unsupported command"));
    }

    let atyp = req_header[3];
    match atyp {
        0x01 => { // IPv4
            let mut ip = [0u8; 4];
            socket.read_exact(&mut ip).await?;
            let port = socket.read_u16().await?;
            Ok((std::net::Ipv4Addr::from(ip).to_string(), port))
        }
        0x03 => { // Domain
            let len = socket.read_u8().await? as usize;
            let mut domain = vec![0u8; len];
            socket.read_exact(&mut domain).await?;
            let port = socket.read_u16().await?;
            Ok((String::from_utf8_lossy(&domain).to_string(), port))
        }
        0x04 => { // IPv6
            let mut ip = [0u8; 16];
            socket.read_exact(&mut ip).await?;
            let port = socket.read_u16().await?;
            Ok((std::net::Ipv6Addr::from(ip).to_string(), port))
        }
        _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unsupported address type")),
    }
}
