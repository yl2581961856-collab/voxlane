use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use tokio::sync::mpsc;
use bytes::Bytes;
use uuid::Uuid;

use crate::core::events::{Event, SessionId};
use crate::core::session::Session;

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    let session_id = SessionId(Uuid::new_v4());
    tracing::info!(?session_id, "client connected");

    let (ev_tx, ev_rx) = mpsc::channel::<Event>(256);

    let session = Session::new(session_id, ev_rx);
    let session_task = tokio::spawn(session.run());

    let _ = ev_tx.send(Event::ClientConnected).await;

    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Binary(bin) => {
                let _ = ev_tx
                    .send(Event::ClientAudioFrame {
                        pcm16: Bytes::from(bin),
                        sample_rate: 16000,
                    })
                    .await;
            }
            Message::Text(s) => {
                let _ = ev_tx.send(Event::ClientText(s)).await;
            }
            Message::Close(_) => {
                let _ = ev_tx.send(Event::ClientDisconnected).await;
                break;
            }
            _ => {}
        }
    }

    let _ = session_task.await;
    tracing::info!(?session_id, "client disconnected");
}
