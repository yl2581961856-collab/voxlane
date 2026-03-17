# VoxLane

Realtime voice gateway over WebSocket (Rust).

This README intentionally focuses on two things only:
- WebSocket interface standard
- WebSocket testing and protocol compliance

## Quick Start

```bash
cargo run
```

Server endpoint:

```text
ws://127.0.0.1:9000/ws
```

## WebSocket Interface Standard (v0.1)

### Transport Rules

- Text frame: UTF-8 JSON with required `type` field
- Binary frame: raw audio packet (no Base64)
- Time fields: integer milliseconds
- Client must send `hello` first
- Server sends `session` after successful handshake

### Audio Contract

- Default input codec: `pcm16`
- Optional input codec: `opus`
- Recommended default config:
  - `sample_rate`: `16000`
  - `channels`: `1`
  - `frame_ms`: `20`

### Client -> Server (Text JSON)

`hello`

```json
{"type":"hello","audio":{"codec":"pcm16|opus","sample_rate":16000,"channels":1,"frame_ms":20}}
```

`text`

```json
{"type":"text","text":"..."}
```

`cancel`

```json
{"type":"cancel","turn_id":123}
```

`reset`

```json
{"type":"reset"}
```

`ping`

```json
{"type":"ping","ts":1700000000}
```

### Client -> Server (Binary)

- Audio frame stream, encoded according to negotiated `audio.codec`

### Server -> Client (Text JSON)

`session`

```json
{"type":"session","session_id":"..."}
```

`asr.partial`

```json
{"type":"asr.partial","turn_id":123,"text":"...","start_ms":0,"end_ms":800}
```

`asr.final`

```json
{"type":"asr.final","turn_id":123,"text":"...","start_ms":0,"end_ms":1200}
```

`llm.delta`

```json
{"type":"llm.delta","turn_id":123,"seq":42,"text":"..."}
```

`tts.meta`

```json
{"type":"tts.meta","turn_id":123,"audio_offset_ms":0,"text_span":[0,12]}
```

`error`

```json
{"type":"error","code":"...","message":"..."}
```

### Server -> Client (Binary)

- TTS audio frame stream

### Turn and Cancel Semantics

- One active turn per session
- New speech supersedes old turn
- `cancel` has highest priority and interrupts current turn immediately
- `reset` clears context
- Late packets from non-active turn must be dropped

### Error and Close Codes

Error codes:
- `ERR_BAD_REQUEST`
- `ERR_UNSUPPORTED_CODEC`
- `ERR_UNAUTHORIZED`
- `ERR_RATE_LIMIT`
- `ERR_BUSY`
- `ERR_TURN_MISMATCH`

Recommended close codes:
- `1008` policy violation
- `1009` message too big
- `1011` internal error

## WebSocket Testing

### 1) Manual Smoke Test (websocat)

Install `websocat`, then run:

```bash
websocat ws://127.0.0.1:9000/ws
```

Send handshake:

```json
{"type":"hello","audio":{"codec":"pcm16","sample_rate":16000,"channels":1,"frame_ms":20}}
```

Expected response:
- one `session` message

Then send:

```json
{"type":"ping","ts":1700000000}
```

Expected response:
- protocol-level `pong` or equivalent heartbeat handling

### 2) Protocol Validation Checklist

- Reject JSON without `type`
- Reject binary audio before `hello`
- Reject unsupported `audio.codec`
- Drop stale packets from superseded turn
- `cancel` should stop current turn immediately
- `reset` should clear context

### 3) Regression Test Command

```bash
cargo test
```

Current baseline includes protocol parsing tests and state-machine tests.

## Versioning Rules for Interface Changes

- Backward-compatible field additions: patch/minor update
- Message type rename/removal: major update
- Any protocol change must update this README examples first
