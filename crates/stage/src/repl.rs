//! REPL mode: /send <file>, /events, raw text.

use stage::proto::{parse_kdl_to_frame, Frame};
use futures::{SinkExt, StreamExt};
use rustyline::DefaultEditor;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub async fn run(url: &str) -> anyhow::Result<()> {
    let (ws, _) = connect_async(url).await?;
    let (mut sink, mut stream) = ws.split();
    sink.send(Message::Text("hello:cli".into())).await?;

    // Background task: print everything coming back from the mirror.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            if let Message::Text(t) = &msg {
                if tx.send(t.clone()).is_err() {
                    break;
                }
            }
        }
    });
    let printer = tokio::spawn(async move {
        while let Some(t) = rx.recv().await {
            println!("<- {t}");
        }
    });

    let mut rl = DefaultEditor::new()?;
    println!("stage repl — /send <file.kdl> /events /quit, raw text goes to the mirror");
    loop {
        let line = match rl.readline("stage> ") {
            Ok(l) => l,
            Err(rustyline::error::ReadlineError::Interrupted)
            | Err(rustyline::error::ReadlineError::Eof) => break,
            Err(e) => return Err(e.into()),
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let _ = rl.add_history_entry(line);
        let frame = if let Some(file) = line.strip_prefix("/send ") {
            let file = file.trim();
            match std::fs::read_to_string(file).map_err(|e| e.to_string()).and_then(|src| {
                parse_kdl_to_frame(&src).map_err(|e| e.to_string())
            }) {
                Ok(payload) => Frame::Ui { payload },
                Err(e) => {
                    println!("! {e}");
                    continue;
                }
            }
        } else if line == "/quit" || line == "/q" {
            break;
        } else {
            // raw text: broadcast to CLI peers as a Cli frame (incl. /events passthrough)
            Frame::Cli { text: line.to_string() }
        };
        sink.send(Message::Text(serde_json::to_string(&frame)?)).await?;
    }
    printer.abort();
    Ok(())
}
