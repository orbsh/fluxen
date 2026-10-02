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
    // 回显用 YAML 而不是单行 JSON（2026-10-02 用户裁决）：`<-` 后换行、
    // 结构化缩进——事件流是给人读的，与 /send 载体同形。
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            // UI 默认以 CBOR 上行：Binary 帧解码回 JSON 再打印，否则事件流
            // 在默认配置下静默消失（auto-detect 首字节，无需 pin 编解码）。
            let text = match &msg {
                Message::Text(t) => serde_json::from_str::<serde_json::Value>(t)
                    .ok()
                    .and_then(|v| serde_yaml::to_string(&v).ok())
                    // 非 JSON 的文本帧按原样显示
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
            {
                Ok(frame) => frame.to_string(),
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
