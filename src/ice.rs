use crate::config::Config;
use crate::models::IceServer;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareCredentialsResponse {
    #[serde(rename = "iceServers")]
    ice_servers: Vec<CloudflareIceServer>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareIceServer {
    urls: Vec<String>,
    username: Option<String>,
    credential: Option<String>,
}

pub async fn get_ice_servers(config: &Config) -> Vec<IceServer> {
    // Check for explicit TURN configuration
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

    // Check for Cloudflare TURN service
    if let (Some(service_id), Some(api_token)) = (
        &config.cloudflare_turn_service_id,
        &config.cloudflare_turn_service_api_token,
    ) {
        return fetch_cloudflare_ice_servers(service_id, api_token).await;
    }

    // Return default STUN servers
    get_default_ice_servers()
}

fn get_default_ice_servers() -> Vec<IceServer> {
    crate::models::DEFAULT_ICE_SERVERS
        .iter()
        .map(|&s| IceServer::from(s))
        .collect()
}

async fn fetch_cloudflare_ice_servers(service_id: &str, api_token: &str) -> Vec<IceServer> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://rtc.live.cloudflare.com/v1/turn/keys/{}/credentials/generate",
        service_id
    );

    match client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "ttl": 86400 }))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => match resp.json::<CloudflareCredentialsResponse>().await {
            Ok(data) => {
                let mut result = Vec::new();

                for server in data.ice_servers {
                    for url in server.urls {
                        result.push(IceServer {
                            urls: vec![url.clone()],
                            username: if url.starts_with("stun:") {
                                None
                            } else {
                                server.username.clone()
                            },
                            credential: if url.starts_with("stun:") {
                                None
                            } else {
                                server.credential.clone()
                            },
                        });
                    }
                }

                if !result.is_empty() {
                    return result;
                }

                warn!("Cloudflare returned empty ICE servers, using defaults");
                get_default_ice_servers()
            }
            Err(e) => {
                error!("Failed to parse Cloudflare response: {}", e);
                get_default_ice_servers()
            }
        },
        Err(e) => {
            error!("Failed to request ICE servers from Cloudflare: {}", e);
            get_default_ice_servers()
        }
    }
}
