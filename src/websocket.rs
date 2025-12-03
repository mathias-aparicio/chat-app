// TODO : Impl a trait to just send a message

use axum::extract::ws::{self, WebSocket};
use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::mpsc::channel;
use uuid::Uuid;

use crate::AppState;
/// The handler recieve message from the user_id associated mpsc::Sender
/// It writes them on the tcp socket
pub async fn handle_socket(socket: WebSocket, state: AppState, user_id: Uuid) {
    let (mut socket_sender, mut socket_reciever) = socket.split();

    // Insert a sender for redpanda topic to give messages
    let (channel_sender, mut channel_receiver) = channel(128);
    state
        .connections_map
        .write()
        .await
        .insert(user_id, channel_sender);

    // Spawn a task to receive the messages and send them over websocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = channel_receiver.recv().await {
            // Serialize to bytes
            if let Ok(json_msg) = serde_json::to_string(&msg) {
                // Send the message or close the connection if error
                if socket_sender
                    .send(ws::Message::Text(json_msg.into()))
                    .await
                    .is_err()
                {
                    break; // Client disconnected
                }
            }
        }
    });

    // Wait until the client close the websocket
    while let Some(Ok(msg)) = socket_reciever.next().await {
        if let ws::Message::Close(_) = msg {
            break;
        }
    }

    send_task.abort();

    state.connections_map.write().await.remove(&user_id);
}
