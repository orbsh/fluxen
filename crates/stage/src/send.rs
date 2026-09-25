//! Batch mode: parse KDL, send to mirror as JSON tree frame.

use stage::proto::{parse_kdl_to_frame, Frame};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::connect_async;

pub async fn send(file: &str, url: &str) -> anyhow::Result<()> {
    let src = std::fs::read_to_string(file)?;
    let payload = parse_kdl_to_frame(&src)?;
    let frame = Frame::Ui { payload };
    let text = serde_json::to_string(&frame)?;

    let (ws, _) = connect_async(url).await?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text("hello:cli".into())).await?;
    sink.send(Message::Text(text)).await?;

    // Wait briefly for UI replies, then exit.
    let deadline = tokio::time::Duration::from_secs(1);
    let _ = tokio::time::timeout(deadline, async {
        while let Some(Ok(msg)) = stream.next().await {
            if let Message::Text(t) = &msg {
                println!("<- {t}");
            }
        }
    })
    .await;
    Ok(())
}
