use std::sync::Arc;

use crate::module_manager::ModuleManager;
use crate::protocol::{IncomingMessage, OutgoingMessage};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use dashmap::DashMap;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub module_manager: ModuleManager,
    pub assets: Arc<DashMap<String, Vec<u8>>>,
    pub cancel_token: CancellationToken,
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/assets/{key}", get(assets_handler))
        .with_state(state)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(|socket| async move {
        if let Err(e) = handle_websocket(socket, &state).await {
            eprintln!("WebSocket error: {}", e);
        }
    })
}

async fn assets_handler(
    axum::extract::Path(key): axum::extract::Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    match state.assets.get(&key) {
        Some(data) => (axum::http::StatusCode::OK, data.clone()).into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_websocket(mut socket: WebSocket, state: &Arc<AppState>) -> anyhow::Result<()> {
    let mut event_rx = state.module_manager.subscribe();
    println!("WebSocket connection established");
    loop {
        tokio::select! {
            _ = state.cancel_token.cancelled() => {
                break;
            }

            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        for line in text.lines() {
                            if line.is_empty() {
                                continue;
                            }
                            match serde_json::from_str::<IncomingMessage>(line) {
                                Ok(msg) => {
                                    if let Err(e) = state.module_manager.route_command(
                                        &msg.module, msg.action, msg.params,
                                    ).await {
                                        eprintln!("Route error: {}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Invalid JSON: {}", e);
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        eprintln!("WebSocket read error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }

            event = event_rx.recv() => {
                match event {
                    Ok(event) => {
                        let msg = OutgoingMessage::from(event);
                        let json = serde_json::to_string(&msg)?;
                        socket.send(Message::Text(json.into())).await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }

    Ok(())
}
