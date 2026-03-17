//! Comprehensive unit tests for the DialogueState state machine.
//!
//! Tests verify the correctness properties required for the paper:
//! 1. No stale data leakage after supersede/cancel
//! 2. Turn ID monotonic progression
//! 3. Phase transitions are valid
//! 4. Cancel propagates to all workers

use bytes::Bytes;
use voxlane::core::commands::{CancelReason, Command};
use voxlane::core::events::{AudioConfig, Event, TurnId};
use voxlane::core::state::{DialogueState, Phase};

// ============================================================================
// Helper functions
// ============================================================================

fn new_state() -> DialogueState {
    DialogueState::new()
}

fn audio_frame(data: &[u8]) -> Event {
    Event::ClientAudioFrame {
        pcm16: Bytes::from(data.to_vec()),
        sample_rate: 16_000,
    }
}

/// Extract turn IDs from StartTurn commands.
fn start_turn_ids(cmds: &[Command]) -> Vec<u64> {
    cmds.iter()
        .filter_map(|c| match c {
            Command::StartTurn { turn } => Some(turn.0),
            _ => None,
        })
        .collect()
}

/// Extract turn IDs from CancelTurn commands.
fn cancel_turn_ids(cmds: &[Command]) -> Vec<u64> {
    cmds.iter()
        .filter_map(|c| match c {
            Command::CancelTurn { turn, .. } => Some(turn.0),
            _ => None,
        })
        .collect()
}

/// Check if commands contain a specific command type.
fn has_cmd(cmds: &[Command], pred: impl Fn(&Command) -> bool) -> bool {
    cmds.iter().any(pred)
}

// ============================================================================
// Phase 1: Basic lifecycle tests
// ============================================================================

#[test]
fn initial_state_is_listening() {
    let s = new_state();
    assert_eq!(s.phase, Phase::Listening);
    assert!(s.active_turn.is_none());
}

#[test]
fn client_connected_produces_no_commands() {
    let mut s = new_state();
    let cmds = s.handle(Event::ClientConnected);
    assert!(cmds.is_empty());
}

#[test]
fn client_disconnected_no_active_turn() {
    let mut s = new_state();
    let cmds = s.handle(Event::ClientDisconnected);
    assert!(cmds.is_empty());
}

#[test]
fn client_hello_sets_audio_config() {
    let mut s = new_state();
    let cmds = s.handle(Event::ClientHello {
        audio: AudioConfig {
            codec: voxlane::core::events::AudioCodec::Opus,
            sample_rate: 48_000,
            channels: 2,
            frame_ms: 10,
        },
    });
    assert!(cmds.is_empty());
    // Audio config is stored internally, verified by subsequent ASR starts.
}

// ============================================================================
// Phase 2: Normal conversation flow
// ============================================================================

#[test]
fn text_input_starts_turn_and_llm() {
    let mut s = new_state();
    let cmds = s.handle(Event::ClientText("hello".to_string()));

    assert_eq!(s.phase, Phase::Thinking);
    assert!(s.active_turn.is_some());

    let starts = start_turn_ids(&cmds);
    assert_eq!(starts.len(), 1);

    assert!(has_cmd(&cmds, |c| matches!(c, Command::LlmStart { .. })));
}

#[test]
fn audio_frame_starts_turn_and_asr() {
    let mut s = new_state();
    let cmds = s.handle(audio_frame(&[0u8; 640]));

    assert!(s.active_turn.is_some());
    assert!(s.active_turn_started);

    let starts = start_turn_ids(&cmds);
    assert_eq!(starts.len(), 1);

    assert!(has_cmd(&cmds, |c| matches!(c, Command::AsrStart { .. })));
    assert!(has_cmd(&cmds, |c| matches!(
        c,
        Command::AsrAudioFrame { .. }
    )));
}

#[test]
fn subsequent_audio_frames_reuse_turn() {
    let mut s = new_state();
    let cmds1 = s.handle(audio_frame(&[0u8; 640]));
    let turn1 = s.active_turn.unwrap();

    let cmds2 = s.handle(audio_frame(&[0u8; 640]));
    let turn2 = s.active_turn.unwrap();

    assert_eq!(turn1, turn2);
    // First frame has StartTurn + AsrStart, second only has AsrAudioFrame.
    assert!(start_turn_ids(&cmds1).len() == 1);
    assert!(start_turn_ids(&cmds2).is_empty());
}

#[test]
fn asr_partial_forwarded_when_turn_matches() {
    let mut s = new_state();
    s.handle(audio_frame(&[0u8; 640]));
    let turn = s.active_turn.unwrap();

    let cmds = s.handle(Event::AsrPartial {
        turn,
        text: "hello".to_string(),
        start_ms: 0,
        end_ms: 800,
    });

    assert!(has_cmd(&cmds, |c| matches!(
        c,
        Command::SendAsrPartial { .. }
    )));
}

#[test]
fn asr_partial_dropped_when_turn_mismatch() {
    let mut s = new_state();
    s.handle(audio_frame(&[0u8; 640]));

    // Send a partial from a different turn.
    let cmds = s.handle(Event::AsrPartial {
        turn: TurnId(999),
        text: "stale".to_string(),
        start_ms: 0,
        end_ms: 800,
    });

    assert!(cmds.is_empty(), "stale turn data must be dropped");
}

#[test]
fn asr_final_transitions_to_thinking_and_starts_llm() {
    let mut s = new_state();
    s.handle(audio_frame(&[0u8; 640]));
    let turn = s.active_turn.unwrap();

    let cmds = s.handle(Event::AsrFinal {
        turn,
        text: "hello world".to_string(),
        start_ms: 0,
        end_ms: 1200,
    });

    assert_eq!(s.phase, Phase::Thinking);
    assert!(has_cmd(&cmds, |c| matches!(
        c,
        Command::SendAsrFinal { .. }
    )));
    assert!(has_cmd(&cmds, |c| matches!(c, Command::LlmStart { .. })));
}

#[test]
fn llm_delta_transitions_to_speaking() {
    let mut s = new_state();
    s.handle(audio_frame(&[0u8; 640]));
    let turn = s.active_turn.unwrap();

    s.handle(Event::AsrFinal {
        turn,
        text: "hi".to_string(),
        start_ms: 0,
        end_ms: 500,
    });

    let cmds = s.handle(Event::LlmDelta {
        turn,
        seq: 0,
        text: "Hello".to_string(),
    });

    assert_eq!(s.phase, Phase::Speaking);
    assert!(has_cmd(&cmds, |c| matches!(
        c,
        Command::SendLlmDelta { .. }
    )));
}

// ============================================================================
// Phase 3: Cancel and supersede tests (CRITICAL for paper)
// ============================================================================

#[test]
fn client_cancel_during_thinking() {
    let mut s = new_state();
    s.handle(Event::ClientText("hello".to_string()));
    let turn = s.active_turn.unwrap();

    let cmds = s.handle(Event::ClientCancel { turn: Some(turn) });

    assert_eq!(s.phase, Phase::Listening);
    assert!(s.active_turn.is_none());

    // Must cancel all workers.
    assert!(has_cmd(&cmds, |c| matches!(c, Command::CancelTurn { .. })));
    assert!(has_cmd(&cmds, |c| matches!(c, Command::AsrCancel { .. })));
    assert!(has_cmd(&cmds, |c| matches!(c, Command::LlmCancel { .. })));
    assert!(has_cmd(&cmds, |c| matches!(c, Command::TtsCancel { .. })));
    assert!(has_cmd(&cmds, |c| matches!(
        c,
        Command::ClearAudioOutput { .. }
    )));
}

#[test]
fn client_cancel_without_turn_id_cancels_active() {
    let mut s = new_state();
    s.handle(Event::ClientText("hello".to_string()));

    let cmds = s.handle(Event::ClientCancel { turn: None });

    assert_eq!(s.phase, Phase::Listening);
    assert!(s.active_turn.is_none());
    assert!(!cancel_turn_ids(&cmds).is_empty());
}

#[test]
fn client_cancel_no_active_turn_is_noop() {
    let mut s = new_state();
    let cmds = s.handle(Event::ClientCancel { turn: None });
    assert!(cmds.is_empty());
}

#[test]
fn vad_speech_start_supersedes_old_turn() {
    let mut s = new_state();
    // Start a turn via text.
    s.handle(Event::ClientText("hello".to_string()));
    let old_turn = s.active_turn.unwrap();

    // VAD triggers a new speech start -> supersede.
    let cmds = s.handle(Event::VadSpeechStart);

    let new_turn = s.active_turn.unwrap();
    assert_ne!(old_turn, new_turn, "new turn must be different from old");
    assert!(
        new_turn.0 > old_turn.0,
        "turn IDs must be monotonically increasing"
    );

    // Old turn must be cancelled.
    let cancelled = cancel_turn_ids(&cmds);
    assert!(
        cancelled.contains(&old_turn.0),
        "old turn must be cancelled"
    );

    // New turn must be started.
    let started = start_turn_ids(&cmds);
    assert!(started.contains(&new_turn.0), "new turn must be started");

    // ASR must be started for the new turn.
    assert!(has_cmd(
        &cmds,
        |c| matches!(c, Command::AsrStart { turn, .. } if *turn == new_turn)
    ));
}

#[test]
fn vad_speech_start_without_previous_turn() {
    let mut s = new_state();
    let cmds = s.handle(Event::VadSpeechStart);

    assert!(s.active_turn.is_some());
    let started = start_turn_ids(&cmds);
    assert_eq!(started.len(), 1);
    assert!(has_cmd(&cmds, |c| matches!(c, Command::AsrStart { .. })));
    // No cancel commands since there was no previous turn.
    assert!(cancel_turn_ids(&cmds).is_empty());
}

#[test]
fn supersede_preserves_active_turn_state() {
    let mut s = new_state();
    s.handle(Event::VadSpeechStart);

    // Verify the state machine knows about the active turn.
    let turn = s.active_turn.unwrap();
    assert!(s.active_turn_started);

    // Subsequent ASR events for this turn should work.
    let cmds = s.handle(Event::AsrPartial {
        turn,
        text: "test".to_string(),
        start_ms: 0,
        end_ms: 200,
    });
    assert!(has_cmd(&cmds, |c| matches!(
        c,
        Command::SendAsrPartial { .. }
    )));
}

#[test]
fn no_stale_leakage_after_supersede() {
    let mut s = new_state();
    s.handle(audio_frame(&[0u8; 640]));
    let old_turn = s.active_turn.unwrap();

    // Supersede.
    s.handle(Event::VadSpeechStart);
    let new_turn = s.active_turn.unwrap();
    assert_ne!(old_turn, new_turn);

    // Late data from old turn must be dropped.
    let cmds = s.handle(Event::AsrFinal {
        turn: old_turn,
        text: "stale transcript".to_string(),
        start_ms: 0,
        end_ms: 1000,
    });
    assert!(cmds.is_empty(), "stale ASR final must be dropped");

    let cmds = s.handle(Event::LlmDelta {
        turn: old_turn,
        seq: 0,
        text: "stale response".to_string(),
    });
    assert!(cmds.is_empty(), "stale LLM delta must be dropped");

    let cmds = s.handle(Event::TtsAudio {
        turn: old_turn,
        chunk: Bytes::from(vec![0u8; 320]),
        is_last: false,
    });
    assert!(cmds.is_empty(), "stale TTS audio must be dropped");
}

// ============================================================================
// Phase 4: Reset tests
// ============================================================================

#[test]
fn reset_cancels_active_turn_and_clears_context() {
    let mut s = new_state();
    s.handle(Event::ClientText("hello".to_string()));

    let cmds = s.handle(Event::ClientReset);

    assert_eq!(s.phase, Phase::Listening);
    assert!(s.active_turn.is_none());
    assert!(has_cmd(&cmds, |c| matches!(c, Command::CancelTurn { .. })));
    assert!(has_cmd(&cmds, |c| matches!(c, Command::ResetContext)));
}

#[test]
fn reset_without_active_turn_just_resets_context() {
    let mut s = new_state();
    let cmds = s.handle(Event::ClientReset);

    assert!(has_cmd(&cmds, |c| matches!(c, Command::ResetContext)));
    assert!(cancel_turn_ids(&cmds).is_empty());
}

// ============================================================================
// Phase 5: Error and timeout tests
// ============================================================================

#[test]
fn backend_error_sends_error_to_client() {
    let mut s = new_state();
    let cmds = s.handle(Event::BackendError {
        turn: None,
        code: "ERR_INTERNAL".to_string(),
        message: "something broke".to_string(),
    });

    assert!(has_cmd(&cmds, |c| matches!(c, Command::SendError { .. })));
}

#[test]
fn timeout_cancels_active_turn() {
    let mut s = new_state();
    s.handle(Event::ClientText("hello".to_string()));

    let cmds = s.handle(Event::Timeout {
        kind: voxlane::core::events::TimeoutKind::ReadIdle,
    });

    assert_eq!(s.phase, Phase::Listening);
    assert!(s.active_turn.is_none());
    assert!(!cancel_turn_ids(&cmds).is_empty());
}

#[test]
fn timeout_no_active_turn_is_noop() {
    let mut s = new_state();
    let cmds = s.handle(Event::Timeout {
        kind: voxlane::core::events::TimeoutKind::ReadIdle,
    });
    assert!(cmds.is_empty());
}

// ============================================================================
// Phase 6: VAD speech end
// ============================================================================

#[test]
fn vad_speech_end_finalizes_asr() {
    let mut s = new_state();
    s.handle(audio_frame(&[0u8; 640]));
    let turn = s.active_turn.unwrap();

    let cmds = s.handle(Event::VadSpeechEnd);

    assert!(has_cmd(
        &cmds,
        |c| matches!(c, Command::AsrFinalize { turn: t } if *t == turn)
    ));
}

#[test]
fn vad_speech_end_no_active_turn_is_noop() {
    let mut s = new_state();
    let cmds = s.handle(Event::VadSpeechEnd);
    assert!(cmds.is_empty());
}

// ============================================================================
// Phase 7: Disconnect during active turn
// ============================================================================

#[test]
fn disconnect_during_speaking_cancels_turn() {
    let mut s = new_state();
    s.handle(audio_frame(&[0u8; 640]));
    let turn = s.active_turn.unwrap();

    // Simulate speaking phase.
    s.handle(Event::AsrFinal {
        turn,
        text: "hi".to_string(),
        start_ms: 0,
        end_ms: 500,
    });
    s.handle(Event::LlmDelta {
        turn,
        seq: 0,
        text: "Hello!".to_string(),
    });
    assert_eq!(s.phase, Phase::Speaking);

    let cmds = s.handle(Event::ClientDisconnected);
    assert!(has_cmd(&cmds, |c| matches!(
        c,
        Command::CancelTurn {
            reason: CancelReason::Disconnect,
            ..
        }
    )));
}

// ============================================================================
// Phase 8: Turn ID monotonicity (property-based)
// ============================================================================

#[test]
fn turn_ids_are_monotonically_increasing() {
    let mut s = new_state();
    let mut last_turn_id = 0u64;

    for _ in 0..100 {
        s.handle(Event::VadSpeechStart);
        let turn = s.active_turn.unwrap();
        assert!(
            turn.0 > last_turn_id,
            "turn IDs must be strictly increasing"
        );
        last_turn_id = turn.0;
    }
}

#[test]
fn rapid_supersede_produces_correct_sequence() {
    let mut s = new_state();

    // Simulate rapid barge-in: 10 consecutive VadSpeechStart events.
    let mut all_cancelled = vec![];
    let mut all_started = vec![];

    for _ in 0..10 {
        let cmds = s.handle(Event::VadSpeechStart);
        all_cancelled.extend(cancel_turn_ids(&cmds));
        all_started.extend(start_turn_ids(&cmds));
    }

    // The last turn should be active.
    let final_turn = s.active_turn.unwrap();
    assert_eq!(final_turn.0, 10);

    // Turn 1 has no cancel (nothing before it), turns 2-10 each cancel the previous.
    // Actually turn 1 is from the first VadSpeechStart (no prev), so cancelled = [1,2,...,9].
    assert_eq!(all_cancelled.len(), 9, "9 turns should be cancelled");
    assert_eq!(all_started.len(), 10, "10 turns should be started");
}
