use crate::config::Config;
use crate::models::IceServer;

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

    // Return default STUN servers
    get_default_ice_servers()
}

fn get_default_ice_servers() -> Vec<IceServer> {
    crate::models::DEFAULT_ICE_SERVERS
        .iter()
        .map(|&s| IceServer::from(s))
        .collect()
}
