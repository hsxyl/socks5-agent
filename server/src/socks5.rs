use crate::db;
use sqlx::sqlite::SqlitePool;
use crate::session::SessionManager;
use common::protocol::ProxyRequest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{error, info};

pub async fn handle_socks5_client(
    mut socket: TcpStream,
    session_manager: SessionManager,
    db_pool: SqlitePool,
) -> std::io::Result<()> {
    let (card_key, target_device) = match handshake(&mut socket, &db_pool).await {
        Ok(Some((key, device))) => (Some(key), device),
        Ok(None) => (None, None),
        Err(e) => {
            error!("SOCKS5 handshake error: {}", e);
            return Ok(());
        }
    };

    let (addr, port) = match read_request(&mut socket).await {
        Ok(req) => req,
        Err(e) => {
            error!("SOCKS5 request error: {}", e);
            return Ok(());
        }
    };

    info!("Proxy request to {}:{} (User: {:?}, Target Device: {:?})", addr, port, card_key, target_device);

    let control_opt = if let Some(ref device_id) = target_device {
        session_manager.get_specific_control(device_id)
    } else {
        session_manager.get_available_control()
    };

    let (device_id_used, control_opt) = match control_opt {
        Some((dev_id, ctrl)) => (Some(dev_id), Some(ctrl)),
        None => (None, None),
    };

    // Try Edge Client first, if none available fallback to Third-Party Proxy
    let tx_rx = if let Some(mut control) = control_opt {
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
                socket.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
                tokio::io::copy_bidirectional(&mut socket, &mut stream).await
            }
            Err(e) => {
                error!("Failed to open stream to Edge Client: {}", e);
                socket.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
                return Ok(());
            }
        }
    } else {
        // Fallback to Third-Party Proxy
        if let Ok(Some(proxy)) = db::get_available_third_party_proxy(&db_pool).await {
            if proxy.proxy_type == "SOCKS5" {
                match TcpStream::connect(&proxy.address).await {
                    Ok(mut proxy_socket) => {
                        // Send SOCKS5 connect to third-party without auth (MVP)
                        proxy_socket.write_all(&[0x05, 0x01, 0x00]).await?;
                        let mut buf = [0u8; 2];
                        proxy_socket.read_exact(&mut buf).await?;
                        
                        proxy_socket.write_all(&[0x05, 0x01, 0x00, 0x03]).await?;
                        let target_bytes = addr.as_bytes();
                        proxy_socket.write_u8(target_bytes.len() as u8).await?;
                        proxy_socket.write_all(target_bytes).await?;
                        proxy_socket.write_u16(port).await?;
                        
                        // Read response (ignoring precise parsing for MVP)
                        let mut resp = [0u8; 10]; 
                        proxy_socket.read_exact(&mut resp).await?;
                        
                        socket.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
                        tokio::io::copy_bidirectional(&mut socket, &mut proxy_socket).await
                    }
                    Err(e) => {
                        error!("Failed to connect to third-party proxy: {}", e);
                        socket.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
                        return Ok(());
                    }
                }
            } else {
                error!("Unsupported proxy type {}, falling back to direct", proxy.proxy_type);
                match TcpStream::connect(format!("{}:{}", addr, port)).await {
                    Ok(mut target_socket) => {
                        socket.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
                        tokio::io::copy_bidirectional(&mut socket, &mut target_socket).await
                    }
                    Err(_) => {
                        socket.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
                        return Ok(());
                    }
                }
            }
        } else {
            error!("No Edge Clients or Third-Party Proxies available");
            socket.write_all(&[0x05, 0x03, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
            return Ok(());
        }
    };

    // Deduct balance & add log
    if let Ok((tx_bytes, rx_bytes)) = tx_rx {
        let total_bytes = tx_bytes + rx_bytes;
        info!("Card {} used {} bytes over SOCKS5", card_key.as_deref().unwrap_or("unknown"), total_bytes);
        if let Some(key) = card_key {
            if let Err(e) = db::deduct_balance(&db_pool, &key, total_bytes).await {
                error!("Failed to deduct balance for card {}: {}", key, e);
            }
            
            let target_url = format!("{}:{}", addr, port);
            if let Err(e) = db::add_proxy_log(&db_pool, &key, &target_url, device_id_used.as_deref(), "SOCKS5", total_bytes).await {
                error!("Failed to add proxy log: {}", e);
            }
        }
    }

    Ok(())
}

async fn handshake(socket: &mut TcpStream, pool: &SqlitePool) -> std::io::Result<Option<(String, Option<String>)>> {
    let mut buf = [0u8; 2];
    socket.read_exact(&mut buf).await?;
    if buf[0] != 0x05 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Not SOCKS5"));
    }
    let nmethods = buf[1] as usize;
    let mut methods = vec![0u8; nmethods];
    socket.read_exact(&mut methods).await?;

    let supports_auth = methods.contains(&0x02);

    if supports_auth {
        socket.write_all(&[0x05, 0x02]).await?;
        
        let mut auth_ver = [0u8; 2];
        socket.read_exact(&mut auth_ver).await?;
        if auth_ver[0] != 0x01 {
            socket.write_all(&[0x01, 0x01]).await?;
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid Auth Version"));
        }
        let ulen = auth_ver[1] as usize;
        let mut uname = vec![0u8; ulen];
        socket.read_exact(&mut uname).await?;
        
        let mut plen_buf = [0u8; 1];
        socket.read_exact(&mut plen_buf).await?;
        let plen = plen_buf[0] as usize;
        let mut passwd = vec![0u8; plen];
        socket.read_exact(&mut passwd).await?;

        let card_key = String::from_utf8_lossy(&uname).to_string();
        let passwd_str = String::from_utf8_lossy(&passwd).to_string();
        let target_device = if passwd_str.is_empty() { None } else { Some(passwd_str) };

        if db::check_balance(pool, &card_key).await.unwrap_or(false) {
            socket.write_all(&[0x01, 0x00]).await?;
            return Ok(Some((card_key, target_device)));
        } else {
            socket.write_all(&[0x01, 0x01]).await?;
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Invalid card or no balance"));
        }
    } else {
        socket.write_all(&[0x05, 0xFF]).await?;
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Username/Password Auth Required"));
    }
}

async fn read_request(socket: &mut TcpStream) -> std::io::Result<(String, u16)> {
    let mut req_header = [0u8; 4];
    socket.read_exact(&mut req_header).await?;
    if req_header[0] != 0x05 || req_header[1] != 0x01 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unsupported command"));
    }

    let atyp = req_header[3];
    match atyp {
        0x01 => {
            let mut ip = [0u8; 4];
            socket.read_exact(&mut ip).await?;
            let port = socket.read_u16().await?;
            Ok((std::net::Ipv4Addr::from(ip).to_string(), port))
        }
        0x03 => {
            let len = socket.read_u8().await? as usize;
            let mut domain = vec![0u8; len];
            socket.read_exact(&mut domain).await?;
            let port = socket.read_u16().await?;
            Ok((String::from_utf8_lossy(&domain).to_string(), port))
        }
        0x04 => {
            let mut ip = [0u8; 16];
            socket.read_exact(&mut ip).await?;
            let port = socket.read_u16().await?;
            Ok((std::net::Ipv6Addr::from(ip).to_string(), port))
        }
        _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unsupported address type")),
    }
}
