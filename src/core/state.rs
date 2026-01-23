use super::events::{Event, TurnId};
use super::commands::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Listening,
    Thinking,
    Speaking,
}

#[derive(Debug)]
pub struct DialogueState {
    pub phase: Phase,
    pub active_turn: TurnId,
}

impl DialogueState {
    pub fn new() -> Self {
        Self {
            phase: Phase::Listening,
            active_turn: TurnId(0),
        }
    }

    pub fn handle(&mut self, ev: Event) -> Vec<Command> {
        match ev {
            Event::ClientConnected => vec![],
            Event::ClientDisconnected => vec![],

            Event::ClientText(s) => vec![Command::SendTextToClient {
                turn: self.active_turn,
                text: format!("echo: {s}"),
            }],

            Event::ClientAudioFrame { .. } => vec![],
        }
    }
}
