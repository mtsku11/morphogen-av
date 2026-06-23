#![forbid(unsafe_code)]

pub mod device_placeholder;
pub mod flow_displace_dispatch;
pub mod pipeline_placeholder;
#[cfg(target_os = "macos")]
pub mod runtime;
pub mod texture_placeholder;

pub use device_placeholder::MetalDevicePlan;
pub use flow_displace_dispatch::{
    validate_advect_feedback_shader_source, validate_coagulated_composite_shader_source,
    validate_convolution_blend_color_shader_source,
    validate_convolution_blend_shader_source, validate_flow_displace_shader_source,
    validate_granular_mosaic_pool_shader_source, validate_granular_mosaic_shader_source,
    validate_video_vocoder_shader_source, FlowDisplaceDispatchPlan, GranularMosaicDispatchPlan,
    MetalDispatchError, TextureRole, ThreadgroupSize, ADVECT_FEEDBACK_KERNEL_NAME,
    ADVECT_FEEDBACK_SHADER_SOURCE, COAGULATED_COMPOSITE_KERNEL_NAME,
    COAGULATED_COMPOSITE_SHADER_SOURCE, CONVOLUTION_BLEND_COLOR_KERNEL_NAME,
    CONVOLUTION_BLEND_COLOR_SHADER_SOURCE, CONVOLUTION_BLEND_KERNEL_NAME,
    CONVOLUTION_BLEND_SHADER_SOURCE, FLOW_DISPLACE_KERNEL_NAME, FLOW_DISPLACE_SHADER_SOURCE,
    GRANULAR_MOSAIC_KERNEL_NAME, GRANULAR_MOSAIC_POOL_KERNEL_NAME,
    GRANULAR_MOSAIC_POOL_SHADER_SOURCE, GRANULAR_MOSAIC_SHADER_SOURCE,
    VIDEO_VOCODER_MATCH_KERNEL_NAME, VIDEO_VOCODER_SHADER_SOURCE,
};
pub use pipeline_placeholder::MetalPipelinePlan;
#[cfg(target_os = "macos")]
pub use runtime::{
    coagulated_composite_metal, convolution_blend_color_metal, convolution_blend_metal,
    flow_displace_metal, flow_feedback_metal, granular_mosaic_metal, granular_mosaic_pool_metal,
    video_vocoder_match_metal,
};
pub use texture_placeholder::MetalTexturePlan;
