use crate::db;
use sqlx::sqlite::SqlitePool;
use crate::session::SessionManager;
use common::protocol::ProxyRequest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{error, info, warn};
use base64::Engine;

pub async fn handle_http_client(
    mut socket: TcpStream,
    session_manager: SessionManager,
    db_pool: SqlitePool,
) -> std::io::Result<()> {
    // Read HTTP Request headers (simplified, looking for CONNECT)
    let mut header_buf = Vec::new();
    let mut temp_buf = [0u8; 1024];
    
    let (target, card_key, target_device) = loop {
        let n = socket.read(&mut temp_buf).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Connection closed"));
        }
        header_buf.extend_from_slice(&temp_buf[..n]);
        
        if header_buf.windows(4).any(|w| w == b"\r\n\r\n") {
            let headers_str = String::from_utf8_lossy(&header_buf);
            let mut lines = headers_str.lines();
            
            let first_line = lines.next().unwrap_or("");
            let parts: Vec<&str> = first_line.split_whitespace().collect();
            if parts.len() < 3 || parts[0] != "CONNECT" {
                // Not a CONNECT request, we only support HTTPS/CONNECT for simplicity in proxies
                let response = "HTTP/1.1 405 Method Not Allowed\r\n\r\n";
                socket.write_all(response.as_bytes()).await?;
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Only CONNECT method supported"));
            }
            
            let target = parts[1].to_string();
            
            // Look for Proxy-Authorization: Basic <base64>
            let mut auth_key = None;
            let mut target_device = None;
            for line in lines {
                let lower = line.to_lowercase();
                if lower.starts_with("proxy-authorization: basic ") {
                    let base64_str = line[27..].trim();
                    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(base64_str) {
                        let decoded_str = String::from_utf8_lossy(&decoded);
                        // format is username:password
                        let auth_parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
                        if auth_parts.len() >= 1 {
                            auth_key = Some(auth_parts[0].to_string());
                        }
                        if auth_parts.len() >= 2 && !auth_parts[1].is_empty() {
                            target_device = Some(auth_parts[1].to_string());
                        }
                    }
                }
            }
            
            if let Some(key) = auth_key {
                break (target, key, target_device);
            } else {
                let response = "HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"EdgeProxy\"\r\n\r\n";
                socket.write_all(response.as_bytes()).await?;
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Proxy-Authorization required"));
            }
        }
        
        if header_buf.len() > 8192 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Headers too large"));
        }
    };

    // Verify Card Key Balance
    if !db::check_balance(&db_pool, &card_key).await.unwrap_or(false) {
        let response = "HTTP/1.1 403 Forbidden\r\n\r\n";
        socket.write_all(response.as_bytes()).await?;
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Invalid card or no balance"));
    }

    info!("HTTP CONNECT proxy request to {} (User: {}, Target Device: {:?})", target, card_key, target_device);

    let control_opt = if let Some(ref device_id) = target_device {
        session_manager.get_specific_control(device_id)
    } else {
        session_manager.get_available_control()
    };

    let (device_id_used, control_opt) = match control_opt {
        Some((dev_id, ctrl)) => (Some(dev_id), Some(ctrl)),
        None => (None, None),
    };

    let tx_rx = if let Some(mut control) = control_opt {
        match control.open_stream().await {
            Ok(stream) => {
                let mut stream = stream.compat();
                let proxy_req = ProxyRequest {
                    target: target.clone(),
                };
                if let Err(e) = proxy_req.write_to(&mut stream).await {
                    error!("Failed to write proxy request to edge client: {}", e);
                    return Ok(());
                }
                
                // Respond 200 OK Connection Established
                socket.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
                tokio::io::copy_bidirectional(&mut socket, &mut stream).await
            }
            Err(e) => {
                error!("Failed to open stream to Edge Client: {}", e);
                socket.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await?;
                return Ok(());
            }
        }
    } else {
        // Fallback to direct (or third party proxy). Simplified to direct for MVP.
        match TcpStream::connect(&target).await {
            Ok(mut target_socket) => {
                socket.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
                tokio::io::copy_bidirectional(&mut socket, &mut target_socket).await
            }
            Err(e) => {
                error!("Fallback connection failed: {}", e);
                socket.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await?;
                return Ok(());
            }
        }
    };

    // Deduct balance & add log
    if let Ok((tx_bytes, rx_bytes)) = tx_rx {
        let total_bytes = tx_bytes + rx_bytes;
        info!("Card {} used {} bytes over HTTP", card_key, total_bytes);
        if let Err(e) = db::deduct_balance(&db_pool, &card_key, total_bytes).await {
            error!("Failed to deduct balance for card {}: {}", card_key, e);
        }
        
        if let Err(e) = db::add_proxy_log(&db_pool, &card_key, &target, device_id_used.as_deref(), "HTTP", total_bytes).await {
            error!("Failed to add proxy log: {}", e);
        }
    }

    Ok(())
}
