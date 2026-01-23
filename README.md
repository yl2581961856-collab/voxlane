# Voice Gateway (High-Performance AI Agent Runtime)

Realtime voice gateway over WebSocket (Rust). Production targets Linux (io_uring/Monoio). Windows is for debug/demo only; protocol and state machine are kept identical.

## Goals
- Single active turn per session (supersede on new speech)
- Streamed ASR/LLM/TTS with alignment metadata
- Backpressure-safe queues and bounded buffers
- Local deployment with lightweight models and predictable latency

## Latency targets (SLA)
- p50: 400-900ms
- p90: 800-1500ms
- p99: 1.5-3s

## Runtime environment
- Production: Linux only (Kernel 5.10+ for io_uring)
- Development: Windows (debug only)
- Rust: nightly for Monoio in prod, stable for Tokio in dev

## Model stack and resources (final)
- ASR (ears): Paraformer-zh-streaming (FunASR) + ONNX optimization  
  - Backend: CPU (ONNX Runtime / Sherpa-onnx), 0 GB VRAM
- LLM (brain): Qwen2.5-14B-Instruct (Int4)  
  - Backend: GPU (llama.cpp via Rust bindings), ~9.5 GB VRAM
- TTS (mouth): GPT-SoVITS (emotion) or CosyVoice-300M-SFT (speed)  
  - Backend: GPU (PyTorch / ONNX), ~3.5 GB VRAM
- Infra (nerves): Monoio (io_uring) + Tokio, Rust native, ~100 MB RAM
- Frontend (body): Unity Pixel Character (PC/Mobile)

### Why llama.cpp
- GBNF grammar for reliable tool calls
- Tight KV-cache control to avoid OOM on long dialogs
- Mature GGUF/quantization ecosystem, stable token streaming
- Native stack, easy Rust integration

## Tri-Lane architecture
### Fast Lane (Monoio)
- WebSocket, Jitter Buffer, VAD, protocol framing
- Target: audio forwarding latency < 5ms
- No blocking IO or complex logic

### Compute Lane (Workers)
- ASR: CPU thread pool, streaming transcription
- LLM/TTS: GPU-dedicated workers, streaming output
- LLM emits tokens to TTS over bounded channels (pipeline)

### Slow Lane (Tokio)
- Intent routing, external APIs, database/memory
- Emit rich UI commands to client

## Interaction strategy (Decoupled Race)
- Text stream: send JSON deltas immediately
- Audio stream: send binary audio chunks when ready
- No hard alignment blocking (text and audio race)

## Audio formats
- Default input: PCM16, 16kHz, mono, 20ms frames (320 samples)
- Optional input: Opus (recommended for mobile and bandwidth saving)
- FFmpeg is only needed for offline or file-based conversion, not for realtime PCM streaming

## WebSocket protocol (strict draft)
### Common rules
- Text messages are UTF-8 JSON objects with a required `type` field.
- All time fields are integers in milliseconds.
- Binary frames carry raw audio packets (PCM16 or Opus); no Base64.
- Client must send `hello` before any binary audio. Server replies with `session`.

### Client -> Server (Text JSON)
- hello / config  
  - {"type":"hello","audio":{"codec":"pcm16|opus","sample_rate":16000,"channels":1,"frame_ms":20}}
- text supplement  
  - {"type":"text","text":"..."}
- cancel (stop current turn, keep context)  
  - {"type":"cancel","turn_id":123}
- reset (clear context)  
  - {"type":"reset"}
- ping  
  - {"type":"ping","ts":1700000000}

### Client -> Server (Binary)
- Audio frames (PCM16 or Opus depending on negotiated codec)

### Server -> Client (Text JSON)
- session ready  
  - {"type":"session","session_id":"..."}
- asr partial/final  
  - {"type":"asr.partial","turn_id":123,"text":"...","start_ms":0,"end_ms":800}
  - {"type":"asr.final","turn_id":123,"text":"...","start_ms":0,"end_ms":1200}
- llm delta  
  - {"type":"llm.delta","turn_id":123,"seq":42,"text":"..."}
- tts meta  
  - {"type":"tts.meta","turn_id":123,"audio_offset_ms":0,"text_span":[0,12]}
- ui action  
  - {"type":"ui.action","name":"weather_panel","data":"Sunny"}
- error  
  - {"type":"error","code":"...","message":"..."}

### Server -> Client (Binary)
- TTS audio frames (PCM16 or Opus)

### Error codes (draft)
- ERR_BAD_REQUEST
- ERR_UNSUPPORTED_CODEC
- ERR_UNAUTHORIZED
- ERR_RATE_LIMIT
- ERR_BUSY
- ERR_TURN_MISMATCH

### WebSocket close codes (recommended)
- 1008 policy violation
- 1009 message too big
- 1011 internal error

## Turn and cancel semantics
- Only one active turn per session
- New SpeechStart supersedes old turn
- Cancel is highest priority: immediately abort ASR/LLM/TTS and clear pending audio
- Cancel does not clear context; use reset to clear
- Any late data from old turn must be dropped

## VAD strategy
- Prefer ASR built-in VAD when available
- If not available, use server-side VAD
- SpeechEnd finalizes ASR stream and stops partials

## Observability
- tracing fields: session_id, turn_id, phase
- metrics: TTFT, TTFA, token jitter, buffer depth, cancel reasons

## Run
- cargo run
- ws://127.0.0.1:9000/ws
