#[derive(Debug, Clone)]
pub struct VadConfig {
    pub sample_rate: u32,
}

pub struct Vad {
    _cfg: VadConfig,
}

impl Vad {
    pub fn new(cfg: VadConfig) -> Self {
        Self { _cfg: cfg }
    }
}
