use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("websocket error: {0}")]
    Ws(String),

    #[error("internal error: {0}")]
    Internal(String),
}
