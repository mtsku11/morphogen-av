#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetalTexturePlan {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
}

impl MetalTexturePlan {
    pub fn intended_responsibilities() -> &'static [&'static str] {
        &[
            "bridge decoded frames and analysis caches into Metal textures",
            "prefer 16-bit or 32-bit float formats for offline render stages",
            "define read/write texture lifetimes for feedback chains",
            "support CPU readback for deterministic tests and exports",
        ]
    }
}
