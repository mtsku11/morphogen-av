use morphogen_render::RuttEtraSettings;
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
pub const CONVOLUTION_BLEND_KERNEL_NAME: &str = "convolution_blend";
pub const CONVOLUTION_BLEND_SHADER_SOURCE: &str =
    include_str!("../shaders/convolution_blend.metal");
pub const CONVOLUTION_BLEND_COLOR_KERNEL_NAME: &str = "convolution_blend_color";
pub const CONVOLUTION_BLEND_COLOR_SHADER_SOURCE: &str =
    include_str!("../shaders/convolution_blend_color.metal");
pub const COAGULATED_COMPOSITE_KERNEL_NAME: &str = "coagulated_composite";
pub const COAGULATED_COMPOSITE_SHADER_SOURCE: &str =
    include_str!("../shaders/coagulated_composite.metal");
pub const FLUID_ADVECT_KERNEL_NAME: &str = "fluid_advect";
pub const FLUID_ADVECT_SHADER_SOURCE: &str = include_str!("../shaders/fluid_advect.metal");
pub const FLUID_ADVECT_TWO_SOURCE_KERNEL_NAME: &str = "fluid_advect_two_source";
pub const FLUID_ADVECT_TWO_SOURCE_SHADER_SOURCE: &str =
    include_str!("../shaders/fluid_advect_two_source.metal");
pub const FIELD_PARTICLES_SPLAT_KERNEL_NAME: &str = "field_particles_splat";
pub const FIELD_PARTICLES_SPLAT_SHADER_SOURCE: &str =
    include_str!("../shaders/field_particles_splat.metal");
pub const LUCAS_KANADE_REFINE_KERNEL_NAME: &str = "lucas_kanade_refine";
pub const LUCAS_KANADE_REFINE_SHADER_SOURCE: &str =
    include_str!("../shaders/lucas_kanade_refine.metal");
pub const PIXEL_SORT_KERNEL_NAME: &str = "pixel_sort";
pub const PIXEL_SORT_SHADER_SOURCE: &str = include_str!("../shaders/pixel_sort.metal");
pub const CHANNEL_SHIFT_KERNEL_NAME: &str = "channel_shift";
pub const CHANNEL_SHIFT_SHADER_SOURCE: &str = include_str!("../shaders/channel_shift.metal");
pub const PALETTE_QUANTIZE_KERNEL_NAME: &str = "palette_quantize";
pub const PALETTE_QUANTIZE_SHADER_SOURCE: &str = include_str!("../shaders/palette_quantize.metal");
pub const RETRO_STATIC_KERNEL_NAME: &str = "retro_static";
pub const RETRO_STATIC_SHADER_SOURCE: &str = include_str!("../shaders/retro_static.metal");
pub const RUTT_ETRA_KERNEL_NAME: &str = "rutt_etra_scanline";
pub const RUTT_ETRA_SHADER_SOURCE: &str = include_str!("../shaders/rutt_etra_scanline.metal");

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
    #[error("unsupported Metal operation: {0}")]
    UnsupportedOperation(String),
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
    #[error(
        "granular_mosaic_pool.metal does not contain the expected texture and buffer bindings"
    )]
    MissingGranularMosaicPoolBindingLayout,
    #[error("video_vocoder.metal does not contain the expected kernel entry point")]
    MissingVideoVocoderKernelEntryPoint,
    #[error("video_vocoder.metal does not contain the expected texture and buffer bindings")]
    MissingVideoVocoderBindingLayout,
    #[error("invalid convolution blend settings: {0}")]
    InvalidConvolutionSettings(String),
    #[error("convolution_blend.metal does not contain the expected kernel entry point")]
    MissingConvolutionBlendKernelEntryPoint,
    #[error("convolution_blend.metal does not contain the expected texture and buffer bindings")]
    MissingConvolutionBlendBindingLayout,
    #[error("invalid coagulation settings: {0}")]
    InvalidCoagulationSettings(String),
    #[error("coagulated_composite.metal does not contain the expected kernel entry point")]
    MissingCoagulatedCompositeKernelEntryPoint,
    #[error(
        "coagulated_composite.metal does not contain the expected texture and buffer bindings"
    )]
    MissingCoagulatedCompositeBindingLayout,
    #[error("invalid fluid advect settings: {0}")]
    InvalidFluidAdvectSettings(String),
    #[error("fluid_advect.metal does not contain the expected kernel entry point")]
    MissingFluidAdvectKernelEntryPoint,
    #[error("fluid_advect.metal does not contain the expected texture and buffer bindings")]
    MissingFluidAdvectBindingLayout,
    #[error("fluid_advect_two_source.metal does not contain the expected kernel entry point")]
    MissingFluidAdvectTwoSourceKernelEntryPoint,
    #[error(
        "fluid_advect_two_source.metal does not contain the expected texture and buffer bindings"
    )]
    MissingFluidAdvectTwoSourceBindingLayout,
    #[error("invalid field particle settings: {0}")]
    InvalidFieldParticlesSettings(String),
    #[error("field_particles_splat.metal does not contain the expected kernel entry point")]
    MissingFieldParticlesSplatKernelEntryPoint,
    #[error(
        "field_particles_splat.metal does not contain the expected texture and buffer bindings"
    )]
    MissingFieldParticlesSplatBindingLayout,
    #[error("lucas_kanade_refine.metal does not contain the expected kernel entry point")]
    MissingLucasKanadeRefineKernelEntryPoint,
    #[error("lucas_kanade_refine.metal does not contain the expected texture and buffer bindings")]
    MissingLucasKanadeRefineBindingLayout,
    #[error("optical-flow orchestration failed: {0}")]
    OpticalFlow(String),
    #[error("pixel_sort.metal does not contain the expected kernel entry point")]
    MissingPixelSortKernelEntryPoint,
    #[error("pixel_sort.metal does not contain the expected texture and buffer bindings")]
    MissingPixelSortBindingLayout,
    #[error("invalid pixel sort settings: {0}")]
    InvalidPixelSortSettings(String),
    #[error("channel_shift.metal does not contain the expected kernel entry point")]
    MissingChannelShiftKernelEntryPoint,
    #[error("channel_shift.metal does not contain the expected texture and buffer bindings")]
    MissingChannelShiftBindingLayout,
    #[error("palette_quantize.metal does not contain the expected kernel entry point")]
    MissingPaletteQuantizeKernelEntryPoint,
    #[error("palette_quantize.metal does not contain the expected texture and buffer bindings")]
    MissingPaletteQuantizeBindingLayout,
    #[error("rutt_etra_scanline.metal does not contain the expected kernel entry point")]
    MissingRuttEtraKernelEntryPoint,
    #[error("rutt_etra_scanline.metal does not contain the expected texture and buffer bindings")]
    MissingRuttEtraBindingLayout,
    #[error("invalid rutt-etra settings: {0}")]
    InvalidRuttEtraSettings(String),
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

pub fn validate_convolution_blend_shader_source() -> Result<(), MetalDispatchError> {
    if !CONVOLUTION_BLEND_SHADER_SOURCE.contains("kernel void convolution_blend") {
        return Err(MetalDispatchError::MissingConvolutionBlendKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::read> carrier [[texture(0)]]",
        "texture2d<float, access::write> output [[texture(1)]]",
        "constant float *weights [[buffer(0)]]",
        "constant ConvolutionBlendParams &params [[buffer(1)]]",
    ] {
        if !CONVOLUTION_BLEND_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingConvolutionBlendBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_coagulated_composite_shader_source() -> Result<(), MetalDispatchError> {
    if !COAGULATED_COMPOSITE_SHADER_SOURCE.contains("kernel void coagulated_composite") {
        return Err(MetalDispatchError::MissingCoagulatedCompositeKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::read> source_a [[texture(0)]]",
        "texture2d<float, access::read> source_b [[texture(1)]]",
        "texture2d<float, access::write> output [[texture(2)]]",
        "constant float *weights [[buffer(0)]]",
        "constant CoagulatedCompositeParams &params [[buffer(1)]]",
    ] {
        if !COAGULATED_COMPOSITE_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingCoagulatedCompositeBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_convolution_blend_color_shader_source() -> Result<(), MetalDispatchError> {
    if !CONVOLUTION_BLEND_COLOR_SHADER_SOURCE.contains("kernel void convolution_blend_color") {
        return Err(MetalDispatchError::MissingConvolutionBlendKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::read> carrier [[texture(0)]]",
        "texture2d<float, access::write> output [[texture(1)]]",
        "constant float *weights_r [[buffer(0)]]",
        "constant float *weights_g [[buffer(1)]]",
        "constant float *weights_b [[buffer(2)]]",
        "constant ConvolutionBlendParams &params [[buffer(3)]]",
    ] {
        if !CONVOLUTION_BLEND_COLOR_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingConvolutionBlendBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_fluid_advect_shader_source() -> Result<(), MetalDispatchError> {
    if !FLUID_ADVECT_SHADER_SOURCE.contains("kernel void fluid_advect") {
        return Err(MetalDispatchError::MissingFluidAdvectKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::sample> source [[texture(0)]]",
        "texture2d<float, access::sample> previous [[texture(1)]]",
        "texture2d<float, access::write> output [[texture(2)]]",
        "constant FluidAdvectParams& params [[buffer(0)]]",
    ] {
        if !FLUID_ADVECT_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingFluidAdvectBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_fluid_advect_two_source_shader_source() -> Result<(), MetalDispatchError> {
    if !FLUID_ADVECT_TWO_SOURCE_SHADER_SOURCE.contains("kernel void fluid_advect_two_source") {
        return Err(MetalDispatchError::MissingFluidAdvectTwoSourceKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::sample> carrierB [[texture(0)]]",
        "texture2d<float, access::sample> previous [[texture(1)]]",
        "texture2d<float, access::read> flow [[texture(2)]]",
        "texture2d<float, access::write> output [[texture(3)]]",
        "constant FluidAdvectTwoSourceParams& params [[buffer(0)]]",
    ] {
        if !FLUID_ADVECT_TWO_SOURCE_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingFluidAdvectTwoSourceBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_field_particles_splat_shader_source() -> Result<(), MetalDispatchError> {
    if !FIELD_PARTICLES_SPLAT_SHADER_SOURCE.contains("kernel void field_particles_splat") {
        return Err(MetalDispatchError::MissingFieldParticlesSplatKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::write> output [[texture(0)]]",
        "device const float* particles [[buffer(0)]]",
        "constant FieldParticlesSplatParams& params [[buffer(1)]]",
    ] {
        if !FIELD_PARTICLES_SPLAT_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingFieldParticlesSplatBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_lucas_kanade_refine_shader_source() -> Result<(), MetalDispatchError> {
    if !LUCAS_KANADE_REFINE_SHADER_SOURCE.contains("kernel void lucas_kanade_refine") {
        return Err(MetalDispatchError::MissingLucasKanadeRefineKernelEntryPoint);
    }

    for expected in [
        "texture2d<float, access::read> previous [[texture(0)]]",
        "texture2d<float, access::read> current [[texture(1)]]",
        "texture2d<float, access::read> flowIn [[texture(2)]]",
        "texture2d<float, access::write> flowOut [[texture(3)]]",
        "texture2d<float, access::write> confidence [[texture(4)]]",
        "constant LucasKanadeRefineParams& params [[buffer(0)]]",
    ] {
        if !LUCAS_KANADE_REFINE_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingLucasKanadeRefineBindingLayout);
        }
    }

    Ok(())
}

pub fn validate_pixel_sort_shader_source() -> Result<(), MetalDispatchError> {
    if !PIXEL_SORT_SHADER_SOURCE.contains("kernel void pixel_sort") {
        return Err(MetalDispatchError::MissingPixelSortKernelEntryPoint);
    }
    for expected in [
        "texture2d<float, access::read>  source [[texture(0)]]",
        "texture2d<float, access::write> output [[texture(1)]]",
        "constant PixelSortParams&       params [[buffer(0)]]",
    ] {
        if !PIXEL_SORT_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingPixelSortBindingLayout);
        }
    }
    Ok(())
}

pub fn validate_channel_shift_shader_source() -> Result<(), MetalDispatchError> {
    if !CHANNEL_SHIFT_SHADER_SOURCE.contains("kernel void channel_shift") {
        return Err(MetalDispatchError::MissingChannelShiftKernelEntryPoint);
    }
    for expected in [
        "texture2d<float, access::sample> source_b [[texture(0)]]",
        "texture2d<float, access::write>  output   [[texture(1)]]",
        "constant ChannelShiftParams&     params   [[buffer(0)]]",
    ] {
        if !CHANNEL_SHIFT_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingChannelShiftBindingLayout);
        }
    }
    Ok(())
}

pub fn validate_palette_quantize_shader_source() -> Result<(), MetalDispatchError> {
    if !PALETTE_QUANTIZE_SHADER_SOURCE.contains("kernel void palette_quantize") {
        return Err(MetalDispatchError::MissingPaletteQuantizeKernelEntryPoint);
    }
    for expected in [
        "texture2d<float, access::read>  source_b [[texture(0)]]",
        "texture2d<float, access::write> output   [[texture(1)]]",
        "constant PaletteQuantizeParams& params   [[buffer(0)]]",
    ] {
        if !PALETTE_QUANTIZE_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingPaletteQuantizeBindingLayout);
        }
    }
    Ok(())
}

pub fn validate_rutt_etra_shader_source() -> Result<(), MetalDispatchError> {
    if !RUTT_ETRA_SHADER_SOURCE.contains("kernel void rutt_etra_scanline") {
        return Err(MetalDispatchError::MissingRuttEtraKernelEntryPoint);
    }
    for expected in [
        "texture2d<float, access::read>  source_b [[texture(0)]]",
        "texture2d<float, access::write> output   [[texture(1)]]",
        "constant RuttEtraParams&        params   [[buffer(0)]]",
    ] {
        if !RUTT_ETRA_SHADER_SOURCE.contains(expected) {
            return Err(MetalDispatchError::MissingRuttEtraBindingLayout);
        }
    }
    Ok(())
}

/// Dispatch plan for the Rutt-Etra gather kernel. Stateless and single-source
/// (analogous to `palette_quantize`): each output pixel gathers its colour by
/// scanning scanlines in reverse order without any cross-thread coordination.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuttEtraDispatchPlan {
    pub width: u32,
    pub height: u32,
    pub line_pitch: u32,
    pub displacement_depth: f32,
    pub line_thickness: u32,
    pub mono: bool,
    pub threads_per_threadgroup: ThreadgroupSize,
    pub threadgroups_per_grid: ThreadgroupSize,
}

impl RuttEtraDispatchPlan {
    pub fn new(
        settings: &RuttEtraSettings,
        width: u32,
        height: u32,
    ) -> Result<Self, MetalDispatchError> {
        if width == 0 || height == 0 {
            return Err(MetalDispatchError::EmptyDimensions);
        }
        if !settings.displacement_depth.is_finite() {
            return Err(MetalDispatchError::NonFiniteAmount);
        }
        if settings.line_pitch == 0 {
            return Err(MetalDispatchError::InvalidRuttEtraSettings(
                "line_pitch must be >= 1".to_string(),
            ));
        }
        if settings.line_thickness == 0 {
            return Err(MetalDispatchError::InvalidRuttEtraSettings(
                "line_thickness must be >= 1".to_string(),
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
            line_pitch: settings.line_pitch,
            displacement_depth: settings.displacement_depth,
            line_thickness: settings.line_thickness,
            mono: settings.mono,
            threads_per_threadgroup,
            threadgroups_per_grid,
        })
    }

    pub fn kernel_name(&self) -> &'static str {
        RUTT_ETRA_KERNEL_NAME
    }
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

    #[test]
    fn convolution_blend_shader_has_expected_bindings() {
        validate_convolution_blend_shader_source().expect("convolution blend shader preflight");
    }

    #[test]
    fn convolution_blend_color_shader_has_expected_bindings() {
        validate_convolution_blend_color_shader_source()
            .expect("colour convolution blend shader preflight");
    }

    #[test]
    fn fluid_advect_shader_has_expected_bindings() {
        validate_fluid_advect_shader_source().expect("fluid advect shader preflight");
    }

    #[test]
    fn fluid_advect_two_source_shader_has_expected_bindings() {
        validate_fluid_advect_two_source_shader_source()
            .expect("fluid advect two-source shader preflight");
    }

    #[test]
    fn field_particles_splat_shader_has_expected_bindings() {
        validate_field_particles_splat_shader_source()
            .expect("field particles splat shader preflight");
    }

    #[test]
    fn lucas_kanade_refine_shader_has_expected_bindings() {
        validate_lucas_kanade_refine_shader_source().expect("lucas kanade refine shader preflight");
    }

    #[test]
    fn channel_shift_shader_has_expected_bindings() {
        validate_channel_shift_shader_source().expect("channel shift shader preflight");
    }

    #[test]
    fn palette_quantize_shader_has_expected_bindings() {
        validate_palette_quantize_shader_source().expect("palette quantize shader preflight");
    }

    #[test]
    fn rutt_etra_shader_has_expected_bindings() {
        validate_rutt_etra_shader_source().expect("rutt_etra_scanline shader preflight");
    }

    #[test]
    fn rutt_etra_dispatch_plan_rejects_zero_line_pitch() {
        let settings = RuttEtraSettings {
            line_pitch: 0,
            ..RuttEtraSettings::default()
        };
        assert!(RuttEtraDispatchPlan::new(&settings, 16, 16).is_err());
    }

    #[test]
    fn rutt_etra_dispatch_plan_rejects_zero_line_thickness() {
        let settings = RuttEtraSettings {
            line_thickness: 0,
            ..RuttEtraSettings::default()
        };
        assert!(RuttEtraDispatchPlan::new(&settings, 16, 16).is_err());
    }
}
