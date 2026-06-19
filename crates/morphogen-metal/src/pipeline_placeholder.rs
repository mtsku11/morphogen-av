#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetalPipelinePlan {
    pub kernel_name: String,
}

impl MetalPipelinePlan {
    pub fn intended_responsibilities() -> &'static [&'static str] {
        &[
            "compile and cache Metal compute pipeline states",
            "bind flow displacement, feedback, pyramid, and analysis kernels",
            "track shader versioning for cache and render provenance",
            "mirror CPU reference parameter semantics",
        ]
    }
}
