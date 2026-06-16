use crate::hub::{Connection, MatchmakingHub};
use crate::pb::{packet::Start, packet::Answer};
use crate::models::encode_connection_id;
use std::sync::Arc;
use tracing::{debug, warn};

use super::websocket::{send_answer_packet, send_offer_packet, send_ping_packet};

pub async fn handle_start(
    connection: &Arc<Connection>,
    hub: &Arc<MatchmakingHub>,
    session_id: &str,
    start: &Start,
) -> anyhow::Result<()> {
    let connection_id = encode_connection_id(&start.connection_id);

    // find_offerer reads every connection's attachment. We must not hold any
    // attachment lock while calling it.
    let offerer = hub.find_offerer(session_id).await;

    if offerer.is_none() {
        // No one waiting — become the offerer. Lock only our own attachment.
        let mut attachment = connection.attachment.write().await;
        attachment.offer_sdp = Some(start.offer_sdp.clone());
        attachment.connection_id = connection_id;
        debug!("[{}] Stored offer, waiting for answerer", session_id);
        return Ok(());
    }

    let offerer_conn = offerer.unwrap();

    // If the offerer we found is this very connection (e.g. a duplicate Start),
    // just refresh our own offer and return — avoids locking the same
    // attachment twice.
    if Arc::ptr_eq(&offerer_conn, connection) {
        let mut attachment = connection.attachment.write().await;
        attachment.offer_sdp = Some(start.offer_sdp.clone());
        attachment.connection_id = connection_id;
        debug!("[{}] Refreshed own offer", session_id);
        return Ok(());
    }

    // Read the offerer's current connection_id without holding the lock longer
    // than necessary.
    let offerer_connection_id = {
        let offerer_att = offerer_conn.attachment.read().await;
        offerer_att.connection_id.clone()
    };

    if let Some(ref conn_id) = connection_id {
        if Some(conn_id) == offerer_connection_id.as_ref() {
            // Same connection_id: offerer is reconnecting with a fresh offer.
            let mut new_att = offerer_conn.attachment.write().await;
            new_att.offer_sdp = Some(start.offer_sdp.clone());
            new_att.connection_id = connection_id;
            debug!(
                "[{}] Replaced stale offer from reconnecting offerer",
                session_id
            );
            return Ok(());
        }
    }

    // Different peer — hand it the offerer's SDP so it can answer.
    let offer_sdp = {
        let offerer_att = offerer_conn.attachment.read().await;
        offerer_att.offer_sdp.clone().unwrap_or_default()
    };

    debug!("[{}] Answerer arrived, sending offer SDP", session_id);
    send_offer_packet(connection, &offer_sdp).await?;

    Ok(())
}

pub async fn handle_answer(
    connection: &Arc<Connection>,
    hub: &Arc<MatchmakingHub>,
    session_id: &str,
    answer: &Answer,
) -> anyhow::Result<()> {
    let offerer = hub.find_offerer(session_id).await;

    if offerer.is_none() {
        warn!("[{}] Unexpected answer — no offerer found", session_id);
        return Err(anyhow::anyhow!("Unexpected answer - no offerer"));
    }

    let offerer_conn = offerer.unwrap();

    // Send answer to offerer with explicit error handling
    if let Err(e) = send_answer_packet(&offerer_conn, &answer.sdp).await {
        warn!("[{}] Failed to send answer packet to offerer: {}", session_id, e);
    }

    // Send ping to answerer to signal completion
    if let Err(e) = send_ping_packet(connection).await {
        warn!("[{}] Failed to send ping packet to answerer: {}", session_id, e);
    }

    debug!("[{}] Answer relayed, peers closed", session_id);

    Ok(())
}
