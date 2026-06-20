use thiserror::Error;

pub const FLOW_DISPLACE_KERNEL_NAME: &str = "flow_displace";
pub const FLOW_DISPLACE_SHADER_SOURCE: &str = include_str!("../shaders/flow_displace.metal");
pub const ADVECT_FEEDBACK_KERNEL_NAME: &str = "advect_feedback";
pub const ADVECT_FEEDBACK_SHADER_SOURCE: &str = include_str!("../shaders/advect_feedback.metal");

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
}
