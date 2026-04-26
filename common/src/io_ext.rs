use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn write_json<W: AsyncWriteExt + Unpin, T: Serialize>(writer: &mut W, msg: &T) -> std::io::Result<()> {
    let data = serde_json::to_vec(msg).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = data.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&data).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_json<R: AsyncReadExt + Unpin, T: DeserializeOwned>(reader: &mut R) -> std::io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 10 * 1024 * 1024 { // max 10MB limit
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "message too large"));
    }
    let mut data = vec![0u8; len];
    reader.read_exact(&mut data).await?;
    let msg = serde_json::from_slice(&data).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(msg)
}
