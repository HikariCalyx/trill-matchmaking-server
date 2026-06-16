use axum::extract::ws::{WebSocket, Message as WsMessage};
use crate::handlers::messages::{handle_answer, handle_start};
use crate::hub::{Connection, MatchmakingHub};
use crate::ice::get_ice_servers;
use crate::pb::{Packet, packet};
use crate::models::SessionAttachment;
use crate::config::Config;
use futures::{SinkExt, StreamExt};
use prost::Message;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{info, warn, debug};

pub async fn handle(
    ws: WebSocket,
    session_id: String,
    hub: Arc<MatchmakingHub>,
) {
    // Split the WebSocket into sender and receiver
    let (mut sender, mut receiver) = ws.split();

    // Create a message channel for sending data to this connection
    let (tx, mut rx) = mpsc::unbounded_channel();

    let connection_id = uuid::Uuid::new_v4();
    
    let connection = Arc::new(Connection {
        id: connection_id,
        tx: tx.clone(),
        attachment: Arc::new(RwLock::new(SessionAttachment::new(session_id.clone()))),
    });

    info!("Client connected to session {}", session_id);

    hub.add_connection(session_id.clone(), Arc::clone(&connection))
        .await;

    // Get ICE servers and send hello packet
    let config = Config::from_env();
    let ice_servers = get_ice_servers(&config).await;

    if let Err(e) = send_hello_packet(&connection, &ice_servers).await {
        warn!("[{}] Failed to send hello packet: {}", session_id, e);
        hub.remove_connection(&session_id, connection_id).await;
        return;
    }

    debug!("[{}] Sent hello", session_id);

    // Spawn a task to forward messages from the channel to the WebSocket sender
    let send_task = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if sender.send(WsMessage::Binary(data)).await.is_err() {
                break;
            }
        }
    });

    // Main message loop
    loop {
        match receiver.next().await {
            Some(Ok(msg)) => {
                if let Err(e) = handle_ws_message(&connection, &hub, &session_id, msg).await {
                    debug!("[{}] Error handling message: {}", session_id, e);
                    break;
                }
            }
            Some(Err(_)) => {
                break;
            }
            None => {
                break;
            }
        }
    }

    info!("[{}] Client disconnected", session_id);
    hub.remove_connection(&session_id, connection_id).await;
    send_task.abort();
}

async fn handle_ws_message(
    connection: &Arc<Connection>,
    hub: &Arc<MatchmakingHub>,
    session_id: &str,
    msg: WsMessage,
) -> anyhow::Result<()> {
    match msg {
        WsMessage::Binary(data) => {
            handle_packet(connection, hub, session_id, &data).await?;
        }
        WsMessage::Close(_) => {
            return Err(anyhow::anyhow!("Connection closed"));
        }
        _ => {}
    }
    Ok(())
}

async fn handle_packet(
    connection: &Arc<Connection>,
    hub: &Arc<MatchmakingHub>,
    session_id: &str,
    data: &[u8],
) -> anyhow::Result<()> {
    let packet = Packet::decode(data)?;

    if packet.is_server_only() {
        warn!(
            "[{}] Unexpected server-only packet type from client",
            session_id
        );
        send_abort_packet(connection, 1).await?;
        return Err(anyhow::anyhow!("Unexpected packet type from client"));
    }

    match &packet.which {
        Some(packet::Which::Start(start)) => {
            let mut att = connection.attachment.write().await;
            handle_start(connection, hub, session_id, start, &mut att).await?;
        }
        Some(packet::Which::Answer(answer)) => {
            let att = connection.attachment.read().await;
            handle_answer(connection, hub, session_id, answer, &att).await?;
        }
        Some(packet::Which::Ping(_)) => {
            send_ping_packet(connection).await?;
        }
        _ => {
            debug!("[{}] Unknown or unhandled packet type, ignoring", session_id);
        }
    }

    Ok(())
}

pub async fn send_hello_packet(
    connection: &Arc<Connection>,
    ice_servers: &[crate::models::IceServer],
) -> anyhow::Result<()> {
    let mut packet = Packet::default();

    let ice_servers_pb: Vec<_> = ice_servers
        .iter()
        .map(|s| crate::pb::packet::hello::IceServer {
            urls: s.urls.clone(),
            username: s.username.clone(),
            credential: s.credential.clone(),
        })
        .collect();

    packet.which = Some(packet::Which::Hello(crate::pb::packet::Hello { 
        ice_servers: ice_servers_pb 
    }));

    let encoded = packet.encode_to_vec();
    connection.tx.send(encoded)?;

    Ok(())
}

pub async fn send_offer_packet(
    connection: &Arc<Connection>,
    sdp: &str,
) -> anyhow::Result<()> {
    let mut packet = Packet::default();
    packet.which = Some(packet::Which::Offer(crate::pb::Offer {
        sdp: sdp.to_string(),
    }));

    let encoded = packet.encode_to_vec();
    connection.tx.send(encoded)?;

    Ok(())
}

pub async fn send_answer_packet(
    connection: &Arc<Connection>,
    sdp: &str,
) -> anyhow::Result<()> {
    let mut packet = Packet::default();
    packet.which = Some(packet::Which::Answer(crate::pb::Answer {
        sdp: sdp.to_string(),
    }));

    let encoded = packet.encode_to_vec();
    connection.tx.send(encoded)?;

    Ok(())
}

pub async fn send_ping_packet(connection: &Arc<Connection>) -> anyhow::Result<()> {
    let mut packet = Packet::default();
    packet.which = Some(packet::Which::Ping(crate::pb::Ping {}));

    let encoded = packet.encode_to_vec();
    connection.tx.send(encoded)?;

    Ok(())
}

pub async fn send_abort_packet(
    connection: &Arc<Connection>,
    reason: i32,
) -> anyhow::Result<()> {
    let mut packet = Packet::default();
    packet.which = Some(packet::Which::Abort(crate::pb::Abort { reason }));

    let encoded = packet.encode_to_vec();
    connection.tx.send(encoded)?;

    Ok(())
}
