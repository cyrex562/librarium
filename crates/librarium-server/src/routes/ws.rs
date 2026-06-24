use crate::config::AppConfig;
use crate::middleware::AuthenticatedUser;
use crate::models::WsMessage;
use crate::routes::vaults::AppState;
use actix_web::{get, web, Error, HttpMessage, HttpRequest, HttpResponse};
use actix_ws::Message;
use std::time::Duration;
use tracing::info;

/// Return the vault ID that a `WsMessage` is scoped to, or `None` for
/// global messages that should be delivered to every connected client.
///
/// This function is an **exhaustive** match with no wildcard arm.  Any new
/// variant added to `WsMessage` will cause a compile error here, forcing the
/// author to decide whether it is vault-scoped (add a `Some(...)` arm) or
/// global (add a `None` arm).  This keeps auth filtering centralized rather
/// than scattered across the WebSocket handler as ad-hoc per-type checks.
fn ws_msg_vault_id(msg: &WsMessage) -> Option<&str> {
    match msg {
        WsMessage::FileChanged { vault_id, .. } => Some(vault_id),
        WsMessage::ReindexComplete { vault_id, .. } => Some(vault_id),
        WsMessage::OrganizeComplete { vault_id, .. } => Some(vault_id),
        // Global / connection-level messages — deliver to all authenticated clients.
        WsMessage::SyncPing | WsMessage::SyncPong { .. } | WsMessage::Error { .. } => None,
    }
}

#[get("/api/ws")]
async fn websocket(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
) -> Result<HttpResponse, Error> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    let mut event_rx = state.event_broadcaster.subscribe();
    let mut ws_rx = state.ws_broadcaster.subscribe();
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    let auth_enabled = config.auth.enabled;
    let current_user = req.extensions().get::<AuthenticatedUser>().cloned();

    actix_web::rt::spawn(async move {
        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
        ping_interval.tick().await; // discard the immediate first tick

        loop {
            tokio::select! {
                // Send a SyncPing every 30 seconds to keep idle connections alive
                // through reverse-proxy idle-timeout policies.
                _ = ping_interval.tick() => {
                    let server_time = chrono::Utc::now().timestamp_millis();
                    let msg = WsMessage::SyncPing;
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if session.text(json).await.is_err() {
                            break;
                        }
                    }
                    let _ = server_time; // available for future SyncPong matching
                }

                // Receive messages from the client
                Some(Ok(msg)) = msg_stream.recv() => {
                    match msg {
                        Message::Ping(bytes) => {
                            if session.pong(&bytes).await.is_err() {
                                break;
                            }
                        }
                        Message::Text(text) => {
                            info!("Received text message: {}", text);
                        }
                        Message::Close(_) => {
                            break;
                        }
                        _ => {}
                    }
                }

                // Receive file change events
                Ok(change_event) = event_rx.recv() => {
                    if auth_enabled {
                        let Some(current_user) = &current_user else {
                            continue;
                        };

                        match state.db.get_vault_role_for_user(&change_event.vault_id, &current_user.user_id).await {
                            Ok(Some(_)) => {}
                            _ => continue,
                        }
                    }

                    let etag = match &change_event.event_type {
                        crate::models::FileChangeType::Created | crate::models::FileChangeType::Modified => {
                            match state.db.get_vault(&change_event.vault_id).await {
                                Ok(vault) => crate::services::FileService::read_file(&vault.path, &change_event.path)
                                    .ok()
                                    .map(|content| format!("\"{:x}\"", content.modified.timestamp_millis())),
                                Err(_) => None,
                            }
                        }
                        _ => None,
                    };

                    let message = crate::models::WsMessage::FileChanged {
                        vault_id: change_event.vault_id.clone(),
                        path: change_event.path.clone(),
                        event_type: change_event.event_type.clone(),
                        etag,
                        timestamp: change_event.timestamp.timestamp_millis(),
                    };

                    if let Ok(json) = serde_json::to_string(&message) {
                        if session.text(json).await.is_err() {
                            break;
                        }
                    }
                }

                // General-purpose WS messages (e.g. ReindexComplete)
                Ok(ws_msg) = ws_rx.recv() => {
                    if auth_enabled {
                        let Some(current_user) = &current_user else {
                            continue;
                        };

                        // Vault-scoped messages are filtered by membership.
                        // Global messages (SyncPing, SyncPong, Error) pass through.
                        if let Some(vault_id) = ws_msg_vault_id(&ws_msg) {
                            match state.db.get_vault_role_for_user(vault_id, &current_user.user_id).await {
                                Ok(Some(_)) => {}
                                _ => continue,
                            }
                        }
                    }

                    if let Ok(json) = serde_json::to_string(&ws_msg) {
                        if session.text(json).await.is_err() {
                            break;
                        }
                    }
                }

                // Server is shutting down — send a Close frame so the client
                // knows to reconnect later rather than treating it as an error.
                _ = shutdown_rx.recv() => {
                    let _ = session.close(Some(actix_ws::CloseReason {
                        code: actix_ws::CloseCode::Away,
                        description: Some("server shutting down".to_string()),
                    })).await;
                    return; // session already consumed; skip the close() below
                }

                else => break,
            }
        }

        // Best-effort close for all non-shutdown exit paths.
        // Ignored if the session was already dropped by the client disconnect.
        drop(session);
    });

    Ok(response)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(websocket);
}
