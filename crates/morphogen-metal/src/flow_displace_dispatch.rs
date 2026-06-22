use thiserror::Error;

pub const FLOW_DISPLACE_KERNEL_NAME: &str = "flow_displace";
pub const FLOW_DISPLACE_SHADER_SOURCE: &str = include_str!("../shaders/flow_displace.metal");
pub const ADVECT_FEEDBACK_KERNEL_NAME: &str = "advect_feedback";
pub const ADVECT_FEEDBACK_SHADER_SOURCE: &str = include_str!("../shaders/advect_feedback.metal");
pub const GRANULAR_MOSAIC_KERNEL_NAME: &str = "granular_mosaic";
pub const GRANULAR_MOSAIC_SHADER_SOURCE: &str = include_str!("../shaders/granular_mosaic.metal");
pub const GRANULAR_MOSAIC_POOL_KERNEL_NAME: &str = "granular_mosaic_pool";
pub const GRANULAR_MOSAIC_POOL_SHADER_SOURCE: &str =
    include_str!("../shaders/granular_mosaic_pool.metal");
pub const VIDEO_VOCODER_MATCH_KERNEL_NAME: &str = "video_vocoder_match";
pub const VIDEO_VOCODER_SHADER_SOURCE: &str = include_str!("../shaders/video_vocoder.metal");

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowDisplaceDispatchPlan {
    pub width: u32,
    pub height: u32,
    pub amount: f32,
    pub threads_per_threadgroup: ThreadgroupSize,
    pub threadgroups_per_grid: ThreadgroupSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadgroupSize {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GranularMosaicDispatchPlan {
    pub width: u32,
    pub height: u32,
    pub grain_size: u32,
    pub rearrangement: f32,
    pub threads_per_threadgroup: ThreadgroupSize,
    pub threadgroups_per_grid: ThreadgroupSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureRole {
    CarrierRgbaFloatSampled,
    FlowRgFloatRead,
    OutputRgbaFloatWrite,
}

#[derive(Debug, Error, PartialEq)]
pub enum MetalDispatchError {
    #[error("dispatch dimensions must be greater than zero")]
    EmptyDimensions,
    #[error("dispatch amount must be finite")]
    NonFiniteAmount,
    #[error("carrier and flow dimensions must match: {0}")]
    IncompatibleInputs(String),
    #[error("Metal system device is unavailable")]
    DeviceUnavailable,
    #[error("Metal shader compilation failed: {0}")]
    ShaderCompilation(String),
    #[error("Metal function lookup failed: {0}")]
    FunctionLookup(String),
    #[error("Metal compute pipeline creation failed: {0}")]
    PipelineCreation(String),
    #[error("Metal command buffer did not complete successfully: {0}")]
    CommandBufferFailed(String),
    #[error("Metal texture byte length is too large")]
    TextureByteLengthTooLarge,
    #[error("flow_displace.metal does not contain the expected kernel entry point")]
    MissingKernelEntryPoint,
    #[error("flow_displace.metal does not contain the expected texture binding layout")]
    MissingTextureBindingLayout,
    #[error("advect_feedback.metal does not contain the expected kernel entry point")]
    MissingFeedbackKernelEntryPoint,
    #[error("advect_feedback.metal does not contain the expected texture binding layout")]
    MissingFeedbackTextureBindingLayout,
    #[error("invalid flow feedback settings: {0}")]
    InvalidFeedbackSettings(String),
    #[error("invalid granular mosaic settings: {0}")]
    InvalidGranularMosaicSettings(String),
    #[error("granular_mosaic.metal does not contain the expected kernel entry point")]
    MissingGranularMosaicKernelEntryPoint,
    #[error("granular_mosaic.metal does not contain the expected texture and buffer bindings")]
    MissingGranularMosaicBindingLayout,
    #[error("granular_mosaic_pool.metal does not contain the expected kernel entry point")]
    MissingGranularMosaicPoolKernelEntryPoint,
    #[error("granular_mosaic_pool.metal does not contain the expected texture and buffer bindings")]
    MissingGranularMosaicPoolBindingLayout,
    #[error("video_vocoder.metal does not contain the expected kernel entry point")]
    MissingVideoVocoderKernelEntryPoint,
    #[error("video_vocoder.metal does not contain the expected texture and buffer bindings")]
    MissingVideoVocoderBindingLayout,
}

impl FlowDisplaceDispatchPlan {
    pub fn new(width: u32, height: u32, amount: f32) -> Result<Self, MetalDispatchError> {
        if width == 0 || height == 0 {
            return Err(MetalDispatchError::EmptyDimensions);
        }
        if !amount.is_finite() {
            return Err(MetalDispatchError::NonFiniteAmount);
        }

        let threads_per_threadgroup = ThreadgroupSize {
            width: 16,
            height: 16,
            depth: 1,
        };
        let threadgroups_per_grid = ThreadgroupSize {
            width: div_ceil(width, threads_per_threadgroup.width),
            height: div_ceil(height, threads_per_threadgroup.height),
            depth: 1,
        };

        Ok(Self {
            width,
            height,
            amount,
            threads_per_threadgroup,
            threadgroups_per_grid,
        })
    }

    pub fn kernel_name(&self) -> &'static str {
        FLOW_DISPLACE_KERNEL_NAME
    }

    pub fn texture_roles(&self) -> [TextureRole; 3] {
        [
            TextureRole::CarrierRgbaFloatSampled,
            TextureRole::FlowRgFloatRead,
            TextureRole::OutputRgbaFloatWrite,
        ]
    }
}

impl GranularMosaicDispatchPlan {
    pub fn new(
        width: u32,
        height: u32,
        grain_size: u32,
        rearrangement: f32,
    ) -> Result<Self, MetalDispatchError> {
        if width == 0 || height == 0 {
            return Err(MetalDispatchError::EmptyDimensions);
        }
        if grain_size == 0 {
            return Err(MetalDispatchError::InvalidGranularMosaicSettings(
                "grain_size must be greater than zero".to_string(),
            ));
        }
        if !rearrangement.is_finite() || !(0.0..=1.0).contains(&rearrangement) {
            return Err(MetalDispatchError::InvalidGranularMosaicSettings(
                "rearrangement must be a finite value between zero and one".to_string(),
            ));
        }

        let threads_per_threadgroup = ThreadgroupSize {
            width: 16,
            height: 16,
            depth: 1,
        };
        let threadgroups_per_grid = ThreadgroupSize {
            width: div_ceil(width, threads_per_threadgroup.width),
            height: div_ceil(height, threads_per_threadgroup.height),
            depth: 1,
        };
        Ok(Self {
            width,
            height,
            grain_size,
            rearrangement,
            threads_per_threadgroup,
            threadgroups_per_grid,
        })
    }

    pub fn kernel_name(&self) -> &'static str {
        GRANULAR_MOSAIC_KERNEL_NAME
    }
}

pub fn validate_flow_displace_shader_source() -> Result<(), MetalDispatchError> {
    if !FLOW_DISPLACE_SHADER_SOURCE.contains("kernel void flow_displace") {
        return Err(MetalDispatchError::MissingKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::sample> carrier [[texture(0)]]",
        "texture2d<float, access::read> flow [[texture(1)]]",
        "texture2d<float, access::write> output [[texture(2)]]",
    ] {
        if !FLOW_DISPLACE_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingTextureBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_advect_feedback_shader_source() -> Result<(), MetalDispatchError> {
    if !ADVECT_FEEDBACK_SHADER_SOURCE.contains("kernel void advect_feedback") {
        return Err(MetalDispatchError::MissingFeedbackKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::sample> currentCarrier [[texture(0)]]",
        "texture2d<float, access::sample> previousOutput [[texture(1)]]",
        "texture2d<float, access::read> velocityField [[texture(2)]]",
        "texture2d<float, access::write> output [[texture(3)]]",
    ] {
        if !ADVECT_FEEDBACK_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingFeedbackTextureBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_granular_mosaic_shader_source() -> Result<(), MetalDispatchError> {
    if !GRANULAR_MOSAIC_SHADER_SOURCE.contains("kernel void granular_mosaic") {
        return Err(MetalDispatchError::MissingGranularMosaicKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::sample> carrier [[texture(0)]]",
        "texture2d<float, access::write> output [[texture(1)]]",
        "constant GranularMosaicParams& params [[buffer(0)]]",
        "device const uint* selectionIndices [[buffer(1)]]",
    ] {
        if !GRANULAR_MOSAIC_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingGranularMosaicBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_granular_mosaic_pool_shader_source() -> Result<(), MetalDispatchError> {
    if !GRANULAR_MOSAIC_POOL_SHADER_SOURCE.contains("kernel void granular_mosaic_pool") {
        return Err(MetalDispatchError::MissingGranularMosaicPoolKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::read> carrier [[texture(0)]]",
        "texture2d<float, access::write> output [[texture(1)]]",
        "texture2d_array<float, access::read> poolFrames [[texture(2)]]",
        "constant GranularMosaicPoolParams& params [[buffer(0)]]",
        "device const uint* selectionIndices [[buffer(1)]]",
        "device const uint* grainMeta [[buffer(2)]]",
    ] {
        if !GRANULAR_MOSAIC_POOL_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingGranularMosaicPoolBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_video_vocoder_shader_source() -> Result<(), MetalDispatchError> {
    if !VIDEO_VOCODER_SHADER_SOURCE.contains("kernel void video_vocoder_match") {
        return Err(MetalDispatchError::MissingVideoVocoderKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::read> carrier [[texture(0)]]",
        "texture2d<float, access::write> output [[texture(1)]]",
        "constant float *tone [[buffer(0)]]",
        "constant VideoVocoderParams &params [[buffer(1)]]",
    ] {
        if !VIDEO_VOCODER_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingVideoVocoderBindingLayout);
        }
    }

    Ok(())
}

fn div_ceil(value: u32, divisor: u32) -> u32 {
    value / divisor + u32::from(value % divisor != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_plan_calculates_threadgroups_for_non_multiple_dimensions() {
        let plan = FlowDisplaceDispatchPlan::new(1920, 1081, 2.0).expect("valid dispatch");

        assert_eq!(plan.kernel_name(), "flow_displace");
        assert_eq!(
            plan.threads_per_threadgroup,
            ThreadgroupSize {
                width: 16,
                height: 16,
                depth: 1
            }
        );
        assert_eq!(
            plan.threadgroups_per_grid,
            ThreadgroupSize {
                width: 120,
                height: 68,
                depth: 1
            }
        );
    }

    #[test]
    fn dispatch_plan_rejects_invalid_inputs() {
        assert_eq!(
            FlowDisplaceDispatchPlan::new(0, 1, 1.0).expect_err("empty dimensions"),
            MetalDispatchError::EmptyDimensions
        );
        assert_eq!(
            FlowDisplaceDispatchPlan::new(1, 1, f32::NAN).expect_err("nan amount"),
            MetalDispatchError::NonFiniteAmount
        );
    }

    #[test]
    fn checked_in_shader_matches_dispatch_plan_bindings() {
        validate_flow_displace_shader_source().expect("shader preflight");
        let plan = FlowDisplaceDispatchPlan::new(16, 16, 1.0).expect("valid dispatch");

        assert_eq!(
            plan.texture_roles(),
            [
                TextureRole::CarrierRgbaFloatSampled,
                TextureRole::FlowRgFloatRead,
                TextureRole::OutputRgbaFloatWrite,
            ]
        );
    }

    #[test]
    fn checked_in_feedback_shader_has_expected_bindings() {
        validate_advect_feedback_shader_source().expect("feedback shader preflight");
    }

    #[test]
    fn granular_mosaic_dispatch_plan_and_shader_are_valid() {
        let plan = GranularMosaicDispatchPlan::new(17, 18, 8, 0.75).expect("valid plan");

        assert_eq!(plan.kernel_name(), "granular_mosaic");
        assert_eq!(plan.threadgroups_per_grid.width, 2);
        assert_eq!(plan.threadgroups_per_grid.height, 2);
        validate_granular_mosaic_shader_source().expect("granular shader preflight");
    }

    #[test]
    fn granular_mosaic_pool_shader_has_expected_bindings() {
        validate_granular_mosaic_pool_shader_source().expect("granular pool shader preflight");
    }

    #[test]
    fn video_vocoder_shader_has_expected_bindings() {
        validate_video_vocoder_shader_source().expect("video vocoder shader preflight");
    }
}
