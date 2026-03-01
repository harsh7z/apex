use apex_common::{McpToTui, TuiToMcp};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use std::time::Duration;

/// Connect to a TUI widget's Unix socket with retry logic
pub async fn connect(socket_path: &str) -> Result<UnixStream, String> {
    let mut attempts = 0;
    let max_attempts = 10;
    let backoff = Duration::from_millis(300);

    loop {
        match UnixStream::connect(socket_path).await {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                attempts += 1;
                if attempts >= max_attempts {
                    return Err(format!(
                        "Failed to connect to {} after {} attempts: {}",
                        socket_path, max_attempts, e
                    ));
                }
                tokio::time::sleep(backoff * attempts).await;
            }
        }
    }
}

/// Send a message to a widget and wait for a response
pub async fn send_and_receive(
    socket_path: &str,
    msg: McpToTui,
) -> Result<TuiToMcp, String> {
    let stream = connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();

    // Send
    let json = serde_json::to_string(&msg).map_err(|e| format!("Serialize error: {}", e))?;
    writer
        .write_all(format!("{}\n", json).as_bytes())
        .await
        .map_err(|e| format!("Write error: {}", e))?;
    writer.flush().await.map_err(|e| format!("Flush error: {}", e))?;

    // Read response with timeout
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    let result = tokio::time::timeout(Duration::from_secs(5), buf_reader.read_line(&mut line))
        .await
        .map_err(|_| "Timeout waiting for response".to_string())?
        .map_err(|e| format!("Read error: {}", e))?;

    if result == 0 {
        return Err("Connection closed".to_string());
    }

    serde_json::from_str::<TuiToMcp>(line.trim())
        .map_err(|e| format!("Parse error: {}", e))
}

/// Send a shutdown message (fire and forget)
pub async fn send_shutdown(socket_path: &str) -> Result<(), String> {
    let stream = connect(socket_path).await?;
    let (_, mut writer) = stream.into_split();

    let msg = McpToTui::Shutdown;
    let json = serde_json::to_string(&msg).map_err(|e| format!("Serialize error: {}", e))?;
    writer
        .write_all(format!("{}\n", json).as_bytes())
        .await
        .map_err(|e| format!("Write error: {}", e))?;
    writer.flush().await.map_err(|e| format!("Flush error: {}", e))?;

    Ok(())
}
