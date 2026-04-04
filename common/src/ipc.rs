use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Write a length-prefixed message: 4-byte big-endian length + payload.
pub async fn write_message<S: AsyncWrite + Unpin>(
    stream: &mut S,
    msg: &[u8],
) -> std::io::Result<()> {
    let len = msg.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(msg).await?;
    stream.flush().await?;
    Ok(())
}

/// Read a length-prefixed message: 4-byte big-endian length, then that many bytes.
/// Returns an empty Vec on EOF (peer closed).
pub async fn read_message<S: AsyncRead + Unpin>(
    stream: &mut S,
) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(Vec::new()),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len_buf) as usize;

    // Sanity check: reject absurdly large messages (>16 MB)
    if len > 16 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("message too large: {} bytes", len),
        ));
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn write_then_read_roundtrip() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        let msg = b"hello from ipc";
        write_message(&mut client, msg).await.unwrap();
        let received = read_message(&mut server).await.unwrap();
        assert_eq!(received, msg.to_vec());
    }

    #[tokio::test]
    async fn empty_payload_roundtrip() {
        let (mut client, mut server) = tokio::io::duplex(64);
        write_message(&mut client, b"").await.unwrap();
        let received = read_message(&mut server).await.unwrap();
        assert_eq!(received, b"".to_vec());
    }

    #[tokio::test]
    async fn oversized_message_rejected() {
        // Write a 4-byte length prefix of 17 MB (above the 16 MB limit) with no body.
        // read_message must return Err(InvalidData) before trying to allocate the body.
        let (mut client, mut server) = tokio::io::duplex(64 * 1024 * 1024);
        let too_big: u32 = 17 * 1024 * 1024;
        client.write_all(&too_big.to_be_bytes()).await.unwrap();
        drop(client); // close write end so read_exact on body doesn't block
        let result = read_message(&mut server).await;
        assert!(result.is_err(), "expected error for oversized message");
        assert_eq!(
            result.unwrap_err().kind(),
            std::io::ErrorKind::InvalidData,
            "expected InvalidData error kind"
        );
    }
}
