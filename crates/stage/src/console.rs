//! Interactive console attached to the running mirror.
//!
//! `stage serve` starts the WS mirror and immediately drops into this REPL on
//! the same terminal — one process, one window. The console connects to the
//! mirror as a /cli peer, so /send frames go through the same routing as
//! external CLI clients.

use futures::{SinkExt, StreamExt};
use rustyline::DefaultEditor;
use stage::proto::parse_yaml_to_frame;
use tokio_tungstenite::tungstenite::Message;

pub async fn run(port: u16) -> anyhow::Result<()> {
    let url = format!("ws://127.0.0.1:{port}/cli");
    let (ws, _) = tokio_tungstenite::connect_async(&url).await?;
    let (mut sink, mut stream) = ws.split();

    // Background task: print everything coming back from the mirror (UI event
    // frames, peer chatter) so the console doubles as an event monitor.
    // Echo as YAML rather than one-line JSON (user ruling 2026-10-02): a newline after `<-`, and
    // structured indentation — the event stream is meant for humans, the same shape as the /send payload.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            // The UI sends CBOR by default: decode the Binary frame back to JSON before printing, otherwise the event stream
            // vanishes silently under the default config (auto-detect on the first byte, no codec pinning needed).
            let text = match &msg {
                Message::Text(t) => serde_json::from_str::<serde_json::Value>(t)
                    .ok()
                    .and_then(|v| serde_yaml::to_string(&v).ok())
                    // Text frames that are not JSON are shown as they are
                    .or_else(|| Some(t.clone())),
                Message::Binary(b) => content::codec::ActiveCodec::Cbor
                    .decode_auto::<serde_json::Value>(b)
                    .ok()
                    .and_then(|v| serde_yaml::to_string(&v).ok()),
                _ => None,
            };
            if let Some(t) = text {
                if tx.send(t).is_err() {
                    break;
                }
            }
        }
    });
    let printer = tokio::spawn(async move {
        while let Some(t) = rx.recv().await {
            println!("<-\n{}", t.trim_end());
        }
    });

    let rl = std::sync::Arc::new(tokio::sync::Mutex::new(DefaultEditor::new()?));
    println!("console — /send <file.yaml> /raw <text> /quit; UI replies stream below");
    loop {
        // readline blocks; spawn_blocking keeps the async runtime free.
        let rl = rl.clone();
        let line = {
            tokio::task::spawn_blocking(move || {
                let mut guard = rl.blocking_lock();
                match guard.readline("stage> ") {
                    Ok(l) => {
                        let _ = guard.add_history_entry(l.trim());
                        Ok(l)
                    }
                    Err(rustyline::error::ReadlineError::Interrupted)
                    | Err(rustyline::error::ReadlineError::Eof) => Err(()),
                    Err(e) => {
                        eprintln!("readline error: {e}");
                        Err(())
                    }
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("join error: {e}"))?
        };
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let text = if let Some(file) = line.strip_prefix("/send ") {
            let file = file.trim();
            match std::fs::read_to_string(file)
                .map_err(|e| e.to_string())
                .and_then(|src| parse_yaml_to_frame(&src).map_err(|e| e.msg))
                .and_then(|frame| {
                    // What goes on the wire is the action list itself (ADR 0021 decision 6)
                    serde_json::to_string(&frame).map_err(|e| e.to_string())
                })
            {
                Ok(text) => text,
                Err(e) => {
                    println!("! {e}");
                    continue;
                }
            }
        } else if let Some(raw) = line.strip_prefix("/raw ") {
            raw.to_string()
        } else if line == "/quit" || line == "/q" {
            break;
        } else {
            println!("! unknown command (try /send <file>, /raw <text>, /quit)");
            continue;
        };
        sink.send(Message::Text(text.into())).await?;
    }
    printer.abort();
    Ok(())
}
