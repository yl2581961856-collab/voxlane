# VoxLane（中文说明）

基于 WebSocket 的实时语音网关（Rust）。生产环境为 Linux（io_uring/Monoio），本地 Windows 仅用于 Debug/Demo，协议与状态机保持一致。

## 目标
- 单 session 仅允许 1 个 active turn（新语音 supersede 旧 turn）
- ASR/LLM/TTS 全链路流式，携带对齐元数据
- 队列有界、可反压、可降级
- 本地部署，轻量模型优先（端到端延迟可控）

## 延迟目标（SLA）
- p50：400-900ms
- p90：800-1500ms
- p99：1.5-3s

## 运行环境
- 生产：Linux only（Kernel 5.10+，io_uring）
- 开发：Windows（仅 Debug），确保协议一致
- Rust：生产可用 nightly（Monoio），开发可用 stable（Tokio）

## 模型选型与资源（最终结论）
- ASR（耳朵）：Paraformer-zh-streaming（FunASR）+ ONNX 优化  
  - Backend：CPU（ONNX Runtime / Sherpa-onnx），0 GB VRAM
- LLM（大脑）：Qwen2.5-14B-Instruct（Int4）  
  - Backend：GPU（llama.cpp + Rust bindings），~9.5 GB VRAM
- TTS（嘴巴）：GPT-SoVITS（情感优先）或 CosyVoice-300M-SFT（速度优先）  
  - Backend：GPU（PyTorch / ONNX），~3.5 GB VRAM
- Infra（神经）：Monoio（io_uring）+ Tokio，Rust Native，~100 MB RAM
- 前端（躯体）：Unity Pixel Character（PC/Mobile）

### 为什么选择 llama.cpp
- GBNF Grammar 支持：本地 LLM 稳定 Tool Call 的低成本方案  
- KV Cache 可控：长对话不易 OOM  
- GGUF/量化生态成熟，流式输出稳定  
- 纯 Native，易与 Rust 集成

## 三车道架构（Tri-Lane）
### 快车道（Monoio）
- 只做 WebSocket、Jitter Buffer、VAD 判停、协议封包/解包
- 核心指标：音频转发延迟 < 5ms
- 约束：不执行复杂业务逻辑或阻塞操作

### 计算车道（Workers）
- ASR：CPU 线程池，实时语音转文字
- LLM/TTS：GPU 独占线程，流式输出
- LLM 产出通过 Channel 异步送入 TTS，形成流水线

### 慢车道（Tokio）
- 意图识别、外部 API（天气/股票）、数据库读写
- 生成富媒体指令（UI 控制）并回传前端

## 交互策略（Decoupled Race）
- 文字流：LLM 逐 token 输出即发 JSON（极速显示）
- 音频流：TTS 生成音频即发 Binary（稍慢但不断）
- 不做强对齐等待（文本和音频解耦竞速）

## 音频格式
- 默认输入：PCM16 / 16kHz / 单声道 / 20ms 帧（320 samples）
- 可选输入：Opus（移动端/省带宽）
- FFmpeg 仅用于离线或文件转码，实时 PCM 传输不强依赖

## WebSocket 协议（严格草案）
### 通用规则
- Text 消息必须是 UTF-8 JSON，且包含 `type` 字段
- 所有时间字段单位为毫秒（整数）
- Binary 传输原始音频包（PCM16/Opus），不使用 Base64
- Client 必须先发送 `hello`，收到 `session` 后才能发送音频
### Client -> Server（Text JSON）
- hello / config  
  - {"type":"hello","audio":{"codec":"pcm16|opus","sample_rate":16000,"channels":1,"frame_ms":20}}
- 文本补充  
  - {"type":"text","text":"..."}
- cancel（仅停止当前 turn，不清上下文）  
  - {"type":"cancel","turn_id":123}
- reset（清空上下文/记忆）  
  - {"type":"reset"}
- ping  
  - {"type":"ping","ts":1700000000}

### Client -> Server（Binary）
- 音频帧（PCM16 或 Opus，取决于协商结果）

### Server -> Client（Text JSON）
- session ready  
  - {"type":"session","session_id":"..."}
- asr partial/final  
  - {"type":"asr.partial","turn_id":123,"text":"...","start_ms":0,"end_ms":800}
  - {"type":"asr.final","turn_id":123,"text":"...","start_ms":0,"end_ms":1200}
- llm delta  
  - {"type":"llm.delta","turn_id":123,"seq":42,"text":"..."}
- tts meta  
  - {"type":"tts.meta","turn_id":123,"audio_offset_ms":0,"text_span":[0,12]}
- ui action（前端控制指令）  
  - {"type":"ui.action","name":"weather_panel","data":"Sunny"}
- error  
  - {"type":"error","code":"...","message":"..."}

### Server -> Client（Binary）
- TTS 音频帧（PCM16 或 Opus）

### 错误码（草案）
- ERR_BAD_REQUEST
- ERR_UNSUPPORTED_CODEC
- ERR_UNAUTHORIZED
- ERR_RATE_LIMIT
- ERR_BUSY
- ERR_TURN_MISMATCH

### WebSocket 关闭码（建议）
- 1008 policy violation
- 1009 message too big
- 1011 internal error

## Turn 与 Cancel 语义
- 单 session 只有一个 active turn
- 新 SpeechStart 直接 supersede 旧 turn
- Cancel 优先级最高：立即打断 ASR/LLM/TTS 并清空待播队列
- Cancel 不清上下文；需要彻底清空时使用 reset
- 旧 turn 的任何延迟返回一律丢弃

## VAD 策略
- 优先使用 ASR 自带 VAD
- 无内置时使用服务端 VAD
- SpeechEnd 后 finalize，停止 partial 输出

## 可观测性
- tracing 字段：session_id, turn_id, phase
- metrics：TTFT、TTFA、token jitter、buffer 深度、cancel 原因分布

## 运行
- cargo run
- ws://127.0.0.1:9000/ws
