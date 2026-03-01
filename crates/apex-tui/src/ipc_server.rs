use apex_common::{McpToTui, TuiToMcp};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use crate::event::AppEvent;

/// Starts a Unix socket server that receives IPC messages from the MCP server.
/// Incoming messages are forwarded to the app event loop.
/// Outgoing responses are written back on the same connection.
pub async fn start_ipc_server(
    socket_path: String,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    mut response_rx: mpsc::UnboundedReceiver<TuiToMcp>,
) -> anyhow::Result<()> {
    // Clean up stale socket
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;

    loop {
        let (stream, _) = listener.accept().await?;
        let (reader, mut writer) = stream.into_split();
        let mut buf_reader = BufReader::new(reader);
        let event_tx = event_tx.clone();

        // Read incoming messages
        let read_handle = tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match buf_reader.read_line(&mut line).await {
                    Ok(0) => break, // Connection closed
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<McpToTui>(trimmed) {
                            Ok(msg) => {
                                let is_shutdown = matches!(msg, McpToTui::Shutdown);
                                if event_tx.send(AppEvent::Ipc(msg)).is_err() {
                                    break;
                                }
                                if is_shutdown {
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("IPC parse error: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("IPC read error: {}", e);
                        break;
                    }
                }
            }
        });

        // Write outgoing responses
        let write_handle = tokio::spawn(async move {
            while let Some(msg) = response_rx.recv().await {
                match serde_json::to_string(&msg) {
                    Ok(json) => {
                        let line = format!("{}\n", json);
                        if writer.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                        let _ = writer.flush().await;
                    }
                    Err(e) => {
                        eprintln!("IPC serialize error: {}", e);
                    }
                }
            }
        });

        let _ = read_handle.await;
        // After read finishes (connection dropped), we break to accept new connections
        // The write handle will end when response_rx is dropped
        drop(write_handle);

        // Re-create the response channel for the next connection
        // For simplicity, we only handle one connection at a time
        break;
    }

    Ok(())
}
