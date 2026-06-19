#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetalDevicePlan {
    pub prefer_low_power_preview: bool,
}

impl MetalDevicePlan {
    pub fn intended_responsibilities() -> &'static [&'static str] {
        &[
            "select the Apple Silicon Metal device",
            "own command queues for preview and offline render work",
            "report texture format and threadgroup limits",
            "coordinate deterministic offline command submission",
        ]
    }
}
