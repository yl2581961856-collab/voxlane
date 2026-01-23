use bytes::Bytes;
use super::events::TurnId;

#[derive(Debug, Clone)]
pub enum Command {
    SendTextToClient { turn: TurnId, text: String },
    SendAudioToClient { turn: TurnId, chunk: Bytes, is_last: bool },
}
