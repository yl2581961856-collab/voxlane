use tokio::select;
use tokio::sync::mpsc;

use super::commands::Command;
use super::events::{Event, SessionId};
use super::state::DialogueState;

pub struct Session {
    id: SessionId,
    ev_rx: mpsc::Receiver<Event>,
    state: DialogueState,
}

impl Session {
    pub fn new(id: SessionId, ev_rx: mpsc::Receiver<Event>) -> Self {
        Self {
            id,
            ev_rx,
            state: DialogueState::new(),
        }
    }

    async fn exec(&self, cmd: Command) {
        tracing::debug!(?cmd, "exec command");
    }

    pub async fn run(mut self) {
        loop {
            select! {
                maybe_ev = self.ev_rx.recv() => {
                    let Some(ev) = maybe_ev else { break; };
                    tracing::debug!(session=?self.id, ?ev, "event");

                    let cmds = self.state.handle(ev);
                    for c in cmds {
                        self.exec(c).await;
                    }
                }
            }
        }
    }
}
