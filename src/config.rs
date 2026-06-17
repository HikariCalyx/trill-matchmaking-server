use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub server_host: String,
    pub server_port: u16,
    pub turn_addr: Option<String>,
    pub turn_user: Option<String>,
    pub turn_credential: Option<String>,
    pub debug: bool,
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            server_host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8000".to_string())
                .parse()
                .unwrap_or(8000),
            turn_addr: env::var("TURN_ADDR").ok(),
            turn_user: env::var("TURN_USER").ok(),
            turn_credential: env::var("TURN_CREDENTIAL").ok(),
            debug: env::var("DEBUG")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "INFO".to_string()),
        }
    }
}
