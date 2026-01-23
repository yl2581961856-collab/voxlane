use bytes::Bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub uuid::Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TurnId(pub u64);

#[derive(Debug, Clone)]
pub enum Event {
    ClientConnected,
    ClientDisconnected,

    ClientAudioFrame {
        pcm16: Bytes,
        sample_rate: u32,
    },

    ClientText(String),
}
