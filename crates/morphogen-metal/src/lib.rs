#![forbid(unsafe_code)]

pub mod device_placeholder;
pub mod flow_displace_dispatch;
pub mod pipeline_placeholder;
#[cfg(target_os = "macos")]
pub mod runtime;
pub mod texture_placeholder;

pub use device_placeholder::MetalDevicePlan;
pub use flow_displace_dispatch::{
    validate_advect_feedback_shader_source, validate_channel_shift_shader_source,
    validate_coagulated_composite_shader_source, validate_convolution_blend_color_shader_source,
    validate_convolution_blend_shader_source, validate_field_particles_splat_shader_source,
    validate_flow_displace_shader_source, validate_fluid_advect_shader_source,
    validate_fluid_advect_two_source_shader_source, validate_granular_mosaic_pool_shader_source,
    validate_granular_mosaic_shader_source, validate_lucas_kanade_refine_shader_source,
    validate_palette_quantize_shader_source, validate_pixel_sort_shader_source,
    validate_rutt_etra_shader_source, validate_video_vocoder_shader_source,
    FlowDisplaceDispatchPlan, GranularMosaicDispatchPlan, MetalDispatchError, RuttEtraDispatchPlan,
    TextureRole, ThreadgroupSize, ADVECT_FEEDBACK_KERNEL_NAME, ADVECT_FEEDBACK_SHADER_SOURCE,
    CHANNEL_SHIFT_KERNEL_NAME, CHANNEL_SHIFT_SHADER_SOURCE, COAGULATED_COMPOSITE_KERNEL_NAME,
    COAGULATED_COMPOSITE_SHADER_SOURCE, CONVOLUTION_BLEND_COLOR_KERNEL_NAME,
    CONVOLUTION_BLEND_COLOR_SHADER_SOURCE, CONVOLUTION_BLEND_KERNEL_NAME,
    CONVOLUTION_BLEND_SHADER_SOURCE, FIELD_PARTICLES_SPLAT_KERNEL_NAME,
    FIELD_PARTICLES_SPLAT_SHADER_SOURCE, FLOW_DISPLACE_KERNEL_NAME, FLOW_DISPLACE_SHADER_SOURCE,
    FLUID_ADVECT_KERNEL_NAME, FLUID_ADVECT_SHADER_SOURCE, FLUID_ADVECT_TWO_SOURCE_KERNEL_NAME,
    FLUID_ADVECT_TWO_SOURCE_SHADER_SOURCE, GRANULAR_MOSAIC_KERNEL_NAME,
    GRANULAR_MOSAIC_POOL_KERNEL_NAME, GRANULAR_MOSAIC_POOL_SHADER_SOURCE,
    GRANULAR_MOSAIC_SHADER_SOURCE, LUCAS_KANADE_REFINE_KERNEL_NAME,
    LUCAS_KANADE_REFINE_SHADER_SOURCE, PALETTE_QUANTIZE_KERNEL_NAME,
    PALETTE_QUANTIZE_SHADER_SOURCE, PIXEL_SORT_KERNEL_NAME, PIXEL_SORT_SHADER_SOURCE,
    RETRO_STATIC_KERNEL_NAME, RETRO_STATIC_SHADER_SOURCE, RUTT_ETRA_KERNEL_NAME,
    RUTT_ETRA_SHADER_SOURCE, VIDEO_VOCODER_MATCH_KERNEL_NAME, VIDEO_VOCODER_SHADER_SOURCE,
};
pub use pipeline_placeholder::MetalPipelinePlan;
#[cfg(target_os = "macos")]
pub use runtime::{
    channel_shift_metal, coagulated_composite_metal, convolution_blend_color_metal,
    convolution_blend_metal, field_particles_splat_metal, flow_displace_metal, flow_feedback_metal,
    fluid_advect_metal, fluid_advect_two_source_metal, granular_mosaic_metal,
    granular_mosaic_pool_metal, palette_quantize_metal, pixel_sort_metal,
    pyramidal_lucas_kanade_flow_metal, retro_static_metal, rutt_etra_scanline_metal,
    video_vocoder_match_metal,
};
pub use texture_placeholder::MetalTexturePlan;
