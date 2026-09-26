//! Mirror server: axum app with WS routes + HTTP POST /send.
//!
//! Routes:
//!   GET  /ui | /channel — UI clients (renderers); receive accrete frames
//!   GET  /cli           — CLI/peer clients; send frames, receive UI replies
//!   POST /send          — batch: body is KDL; converted and broadcast to all
//!                         peers. No dedicated client needed — curl works.
//!
//! The mirror does not parse accrete/content for routing; POST /send is the one
//! place that converts (KDL → Message<Accrete>) since its input is a file, not
//! a peer.

use axum::extract::ws::Message;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

type Peer = mpsc::UnboundedSender<Message>;
type Peers = Arc<Mutex<Vec<Peer>>>;

#[derive(Clone)]
struct Ctx {
    ui: Peers,
    cli: Peers,
}

pub async fn serve(port: u16, trunk_port: u16, dist: std::path::PathBuf) -> anyhow::Result<()> {
    let ctx = Ctx {
        ui: Arc::new(Mutex::new(Vec::new())),
        cli: Arc::new(Mutex::new(Vec::new())),
    };
    // startup probe (sticky): trunk up -> reverse-proxy the UI, else serve dist
    let taddr = std::net::SocketAddr::from(([127, 0, 0, 1], trunk_port));
    let proxy = crate::ui::port_up(taddr).await.then_some(taddr);
    let ui = crate::ui::UiState {
        proxy,
        dist: Arc::new(dist.clone()),
    };
    let app = Router::new()
        .route("/ui", get(ws_ui))
        .route("/channel", get(ws_ui)) // UI's WsTransport path
        .route("/cli", get(ws_cli))
        .route("/send", post(http_send))
        .fallback(get(crate::ui::ui_fallback))
        .layer(axum::Extension(ui.clone()))
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("stage listening on 0.0.0.0:{port}");
    println!("  ws   /channel (ui)  /cli (peers)");
    println!("  http POST /send    (body: KDL)");
    match ui.proxy {
        Some(t) => println!("  http *          -> proxy to trunk at {t}"),
        None => println!(
            "  http *          -> static {} (trunk port {trunk_port} down)",
            ui.dist.display()
        ),
    }
    axum::serve(listener, app).await?;
    Ok(())
}

async fn ws_ui(ws: WebSocketUpgrade, State(ctx): State<Ctx>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| run_peer(socket, ctx, true))
}

async fn ws_cli(ws: WebSocketUpgrade, State(ctx): State<Ctx>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| run_peer(socket, ctx, false))
}

/// POST /send: body is KDL (default) or YAML (`?fmt=yaml`), converted to a
/// Message<Accrete> frame and broadcast. Returns the wire JSON so `curl -fsS`
/// output is inspectable.
async fn http_send(
    State(ctx): State<Ctx>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
    body: String,
) -> impl IntoResponse {
    let frame = match q.get("fmt").map(|s| s.as_str()) {
        Some("yaml") => crate::proto::parse_yaml_to_frame(&body),
        // explicit selection, no content sniffing: absent or any other fmt = KDL
        _ => crate::proto::parse_kdl_to_frame(&body),
    };
    match frame {
        Ok(frame) => {
            let text = frame.to_string();
            let mut ui = ctx.ui.lock().await;
            let mut cli = ctx.cli.lock().await;
            let all = ui.iter_mut().chain(cli.iter_mut());
            let mut sent = 0;
            for p in all {
                if p.send(Message::text(text.clone())).is_ok() {
                    sent += 1;
                }
            }
            format!("ok, delivered to {sent} peer(s)\n").into_response()
        }
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, format!("error: {e}\n")).into_response(),
    }
}

async fn run_peer(socket: WebSocket, ctx: Ctx, is_ui: bool) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let tx_for_cleanup = tx.clone();
    let tx_for_broadcast = tx.clone();
    {
        let list = if is_ui { &ctx.ui } else { &ctx.cli };
        list.lock().await.push(tx);
    }

    let (mut sink, mut stream) = socket.split();

    let out = async move {
        while let Some(msg) = rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    };

    let ui_b = ctx.ui.clone();
    let cli_b = ctx.cli.clone();
    let inp = async move {
        while let Some(Ok(msg)) = stream.next().await {
            if !matches!(msg, Message::Text(_) | Message::Binary(_)) {
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

    ctx.cli
        .lock()
        .await
        .retain(|p| !p.same_channel(&tx_for_cleanup));
    ctx.ui
        .lock()
        .await
        .retain(|p| !p.same_channel(&tx_for_cleanup));
}
