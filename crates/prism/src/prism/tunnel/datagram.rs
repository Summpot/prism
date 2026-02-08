use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::prism::tunnel::protocol::{MAX_DATAGRAM_BYTES, ProtocolError};

/// Datagram framing over a tunnel stream.
///
/// Each datagram is encoded as: `u32be len` + `payload`.
///
/// This is used for UDP proxying over a multiplexed stream (see DESIGN.md).
pub struct DatagramConn<RW> {
    inner: RW,
}

impl<RW> DatagramConn<RW> {
    pub fn new(inner: RW) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> RW {
        self.inner
    }
}

impl<RW> DatagramConn<RW>
where
    RW: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn read_datagram(&mut self, out: &mut [u8]) -> Result<usize, ProtocolError> {
        let n = self.inner.read_u32().await?;
        if n > MAX_DATAGRAM_BYTES {
            return Err(ProtocolError::PayloadTooLarge(n));
        }
        let n = n as usize;
        if n > out.len() {
            // Drain to keep stream aligned.
            let mut drain = vec![0u8; n];
            self.inner.read_exact(&mut drain).await?;
            return Err(ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "short buffer",
            )));
        }
        self.inner.read_exact(&mut out[..n]).await?;
        Ok(n)
    }

    pub async fn write_datagram(&mut self, payload: &[u8]) -> Result<(), ProtocolError> {
        let n: u32 = payload
            .len()
            .try_into()
            .map_err(|_| ProtocolError::PayloadTooLarge(u32::MAX))?;
        if n > MAX_DATAGRAM_BYTES {
            return Err(ProtocolError::PayloadTooLarge(n));
        }
        self.inner.write_u32(n).await?;
        self.inner.write_all(payload).await?;
        Ok(())
    }
}
