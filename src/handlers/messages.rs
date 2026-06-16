use crate::hub::{Connection, MatchmakingHub};
use crate::pb::{packet::Start, packet::Answer};
use crate::models::{encode_connection_id, SessionAttachment};
use std::sync::Arc;
use tracing::{debug, warn};

use super::websocket::{send_answer_packet, send_offer_packet, send_ping_packet};

pub async fn handle_start(
    connection: &Arc<Connection>,
    hub: &Arc<MatchmakingHub>,
    session_id: &str,
    start: &Start,
    attachment: &mut SessionAttachment,
) -> anyhow::Result<()> {
    let connection_id = encode_connection_id(&start.connection_id);

    let offerer = hub.find_offerer(session_id).await;

    if offerer.is_none() {
        // No one waiting — become the offerer
        attachment.offer_sdp = Some(start.offer_sdp.clone());
        attachment.connection_id = connection_id;
        debug!(
            "[{}] Stored offer, waiting for answerer",
            session_id
        );
        return Ok(());
    }

    let offerer_conn = offerer.unwrap();
    let offerer_att = offerer_conn.attachment.read().await;

    if let Some(ref conn_id) = connection_id {
        if Some(conn_id) == offerer_att.connection_id.as_ref() {
            // Same connection_id: offerer is reconnecting with a fresh offer.
            drop(offerer_att);

            let mut new_att = offerer_conn.attachment.write().await;
            new_att.offer_sdp = Some(start.offer_sdp.clone());
            new_att.connection_id = connection_id;
            drop(new_att);

            // Clear stale socket's offer before closing it
            // Note: We'll close the old connection
            debug!(
                "[{}] Replaced stale offer from reconnecting offerer",
                session_id
            );
            return Ok(());
        }
    }

    // Different peer — hand it the offerer's SDP so it can answer
    let offer_sdp = offerer_att.offer_sdp.clone().unwrap_or_default();
    drop(offerer_att);

    debug!("[{}] Answerer arrived, sending offer SDP", session_id);
    send_offer_packet(connection, &offer_sdp).await?;

    Ok(())
}

pub async fn handle_answer(
    connection: &Arc<Connection>,
    hub: &Arc<MatchmakingHub>,
    session_id: &str,
    answer: &Answer,
    _attachment: &SessionAttachment,
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
