use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use hmac::{Hmac, Mac};
use rand::Rng;
use sha1::Sha1;

use crate::config::Config;
use crate::models::IceServer;

type HmacSha1 = Hmac<Sha1>;

pub async fn get_ice_servers(config: &Config) -> Vec<IceServer> {
    if config.use_static_turn_credential {
        // Static mode: advertise the configured TURN server with fixed credentials.
        if let (Some(addr), Some(user), Some(cred)) = (
            &config.turn_addr,
            &config.turn_user,
            &config.turn_credential,
        ) {
            return vec![IceServer {
                urls: vec![format!("turn:{}", addr)],
                username: Some(user.clone()),
                credential: Some(cred.clone()),
            }];
        }
        tracing::warn!(
            "USE_STATIC_TURN_CREDENTIAL is enabled but TURN_ADDR/TURN_USER/TURN_CREDENTIAL are \
             not fully set; falling back to default STUN servers"
        );
    } else {
        // Dynamic mode: mint time-limited credentials accepted by the co-located coturn
        // instance via its REST API mechanism (shared `static-auth-secret`).
        match (&config.turn_addr, &config.turn_secret) {
            (Some(addr), Some(secret)) => {
                return vec![generate_ephemeral_ice_server(
                    addr,
                    secret,
                    config.turn_credential_ttl,
                )];
            }
            _ => {
                tracing::warn!(
                    "Dynamic TURN credentials requested but TURN_ADDR/TURN_SECRET are not set; \
                     falling back to default STUN servers"
                );
            }
        }
    }

    get_default_ice_servers()
}

/// Generate an ephemeral TURN credential compatible with coturn's REST API
/// (`use-auth-secret` + `static-auth-secret`).
///
/// The username encodes the UNIX expiry timestamp plus a random component, and
/// the credential is `base64(HMAC-SHA1(secret, username))`. coturn validates the
/// pair itself, so no network round-trip is required even though it runs on the
/// same instance.
fn generate_ephemeral_ice_server(addr: &str, secret: &str, ttl: u64) -> IceServer {
    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        + ttl;

    // Random component so each connection gets a unique username.
    let random: u64 = rand::thread_rng().gen();
    let username = format!("{}:{:016x}", expiry, random);

    let credential = sign_credential(secret, &username);

    IceServer {
        urls: vec![format!("turn:{}", addr)],
        username: Some(username),
        credential: Some(credential),
    }
}

fn sign_credential(secret: &str, username: &str) -> String {
    let mut mac =
        HmacSha1::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(username.as_bytes());
    let digest = mac.finalize().into_bytes();
    base64::engine::general_purpose::STANDARD.encode(digest)
}

fn get_default_ice_servers() -> Vec<IceServer> {
    crate::models::DEFAULT_ICE_SERVERS
        .iter()
        .map(|&s| IceServer::from(s))
        .collect()
}
