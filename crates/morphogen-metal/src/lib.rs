#![forbid(unsafe_code)]

pub mod device_placeholder;
pub mod flow_displace_dispatch;
pub mod pipeline_placeholder;
#[cfg(target_os = "macos")]
pub mod runtime;
pub mod texture_placeholder;

pub use device_placeholder::MetalDevicePlan;
pub use flow_displace_dispatch::{
    validate_advect_feedback_shader_source, validate_flow_displace_shader_source,
    FlowDisplaceDispatchPlan, MetalDispatchError, TextureRole, ThreadgroupSize,
    ADVECT_FEEDBACK_KERNEL_NAME, ADVECT_FEEDBACK_SHADER_SOURCE, FLOW_DISPLACE_KERNEL_NAME,
    FLOW_DISPLACE_SHADER_SOURCE,
};
pub use pipeline_placeholder::MetalPipelinePlan;
#[cfg(target_os = "macos")]
pub use runtime::{flow_displace_metal, flow_feedback_metal};
pub use texture_placeholder::MetalTexturePlan;
