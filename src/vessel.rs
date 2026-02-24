use std::net::SocketAddr;
use std::sync::Arc;

use crate::api;
use crate::dashboard::DashboardStore;
use crate::module_manager::ModuleManager;
use crate::protocol::{IncomingMessage, OutgoingMessage};
use axum::extract::ConnectInfo;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::{Router, routing::get};
use dashmap::DashMap;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, info_span, trace, warn, Instrument};

pub struct AppState {
    pub module_manager: ModuleManager,
    pub assets: Arc<DashMap<String, (Vec<u8>, String)>>,
    pub dashboard_store: Arc<DashboardStore>,
    pub cancel_token: CancellationToken,
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/assets/{key}", get(assets_handler))
        .nest("/api", api::router().with_state(state.clone()))
        .with_state(state)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| {
        let span = info_span!("ws_connection", peer = %peer);
        async move {
            if let Err(e) = handle_websocket(socket, state).await {
                error!("WebSocket handler error: {e}");
            }
        }
        .instrument(span)
    })
}

async fn assets_handler(
    axum::extract::Path(key): axum::extract::Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    match state.assets.get(&key) {
        Some(entry) => {
            let (data, content_type) = entry.value();
            (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, content_type.clone())],
                data.clone(),
            )
                .into_response()
        }
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

async fn handle_websocket(mut socket: WebSocket, state: Arc<AppState>) -> anyhow::Result<()> {
    // Subscribe before snapshot to guarantee no events are missed between the two.
    let mut event_rx = state.module_manager.subscribe();

    info!("client connected");

    for event in state.module_manager.snapshot() {
        let msg = OutgoingMessage::from(event);
        let json = serde_json::to_string(&msg)?;
        socket.send(Message::Text(json.into())).await?;
    }

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
                                Ok(IncomingMessage::Call { request_id, module, name, params, .. }) => {
                                    debug!(module = %module, action = %name, "→ call");
                                    trace!(raw = %line, "→ raw");
                                    if let Err(e) = state.module_manager.route_command(&module, name, params).await {
                                        error!("route error: {e}");
                                    }
                                    // TODO: send Response back with request_id once request tracking is wired
                                    let _ = request_id;
                                }
                                Ok(IncomingMessage::Subscribe { module, name }) => {
                                    // Subscriptions are currently implicit — all clients receive all events.
                                    // Explicit filtering is a future task.
                                    debug!(module = %module, event = %name, "→ subscribe");
                                }
                                Err(e) => {
                                    error!(raw = %line, "invalid message: {e}");
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        error!("WebSocket read error: {e}");
                        break;
                    }
                    _ => {}
                }
            }

            event = event_rx.recv() => {
                match event {
                    Ok(event) => {
                        debug!(module = event.source(), event = event.event_name(), "← event");
                        let msg = OutgoingMessage::from(event);
                        let json = serde_json::to_string(&msg)?;
                        trace!(raw = %json, "← raw");
                        socket.send(Message::Text(json.into())).await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "event receiver lagged, events dropped");
                        continue;
                    }
                }
            }
        }
    }

    info!("client disconnected");

    Ok(())
}
