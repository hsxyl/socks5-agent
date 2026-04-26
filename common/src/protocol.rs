use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ControlMessage {
    Register { device_id: String },
    Heartbeat { cpu_usage: u8, mem_usage: u8 },
    HeartbeatAck,
}

pub struct ProxyRequest {
    pub target: String,
}

impl ProxyRequest {
    pub async fn write_to<W: AsyncWriteExt + Unpin>(&self, writer: &mut W) -> std::io::Result<()> {
        let bytes = self.target.as_bytes();
        let len = bytes.len() as u16;
        writer.write_u16(len).await?;
        writer.write_all(bytes).await?;
        Ok(())
    }

    pub async fn read_from<R: AsyncReadExt + Unpin>(reader: &mut R) -> std::io::Result<Self> {
        let len = reader.read_u16().await? as usize;
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).await?;
        let target = String::from_utf8_lossy(&buf).to_string();
        Ok(ProxyRequest { target })
    }
}
