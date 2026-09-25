//! Mirror server. Routes:
//!   /ui  — UI clients (renderers) receive everything sent by CLI clients
//!   /cli — CLI clients send brick frames and receive UI replies
//!
//! The mirror is dumb: it does not parse brick/content, only routes whole
//! binary messages. A peer declares its route with the first text message
//! ("hello:ui" / "hello:cli").

use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;

type Peer = mpsc::UnboundedSender<Message>;
type Peers = Arc<Mutex<Vec<Peer>>>;

pub async fn serve(port: u16) -> anyhow::Result<()> {
    let ui: Peers = Arc::new(Mutex::new(Vec::new()));
    let cli: Peers = Arc::new(Mutex::new(Vec::new()));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("stage mirror listening on 0.0.0.0:{port} (routes: /ui, /cli)");
    loop {
        let (stream, _) = listener.accept().await?;
        let ui = ui.clone();
        let cli = cli.clone();
        tokio::spawn(async move {
            let ws = match tokio_tungstenite::accept_async(stream).await {
                Ok(ws) => ws,
                Err(_) => return,
            };
            // First message from a peer declares its route.
            let (ws, first) = match ws.into_future().await {
                (Some(Ok(Message::Text(t))), rest) => (rest, t.to_string()),
                _ => return,
            };
            match first.as_str() {
                "hello:ui" => run_peer(ws, ui, cli).await,
                "hello:cli" => run_peer(ws, ui, cli).await,
                _ => {}
            }
        });
    }
}

async fn run_peer<S>(ws: S, ui: Peers, cli: Peers)
where
    S: futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
        + futures::Sink<Message>
        + Unpin,
{
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let tx_for_cleanup = tx.clone();
    let tx_for_broadcast = tx.clone();
    {
        let mut list = cli.lock().await;
        list.push(tx);
    }

    let (mut sink, mut stream) = ws.split();

    // outgoing: mirror queue -> this peer
    let out = async move {
        while let Some(msg) = rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    };

    // incoming: this peer -> broadcast to all other CLI peers (no UI clients
    // exist yet in stage; UI is reached via its own transport later).
    let ui_b = ui.clone();
    let cli_b = cli.clone();
    let inp = async move {
        while let Some(Ok(msg)) = stream.next().await {
            if !msg.is_text() && !msg.is_binary() {
                continue;
            }
            let mut ui_list = ui_b.lock().await;
            let mut cli_list = cli_b.lock().await;
            let all = ui_list.iter_mut().chain(cli_list.iter_mut());
            for p in all {
                if !p.same_channel(&tx_for_broadcast) {
                    let _ = p.send(msg.clone());
                }
            }
        }
    };

    tokio::join!(out, inp);

    cli.lock().await.retain(|p| !p.same_channel(&tx_for_cleanup));
    ui.lock().await.retain(|p| !p.same_channel(&tx_for_cleanup));
}
