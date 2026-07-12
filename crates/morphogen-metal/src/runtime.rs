use metal::{
    CommandQueue, CompileOptions, ComputePipelineState, Device, MTLCommandBufferStatus,
    MTLPixelFormat, MTLRegion, MTLResourceOptions, MTLSize, MTLStorageMode, MTLTextureType,
    MTLTextureUsage, Texture, TextureDescriptor,
};
use morphogen_render::{
    pyramidal_lucas_kanade_flow_with_refiner, ChannelShiftSettings, FieldParticleSettings,
    FlowFeedbackSettings, FlowField, FluidAdvectSettings, FluidAdvectTwoSourceSettings, GrainPool,
    GrainSelection, GranularMosaicSettings, ImageBufferF32, MatteField, PaletteQuantizeSettings,
    ParticleField, PixelSortSettings, PyramidalLucasKanadeEstimate, QuantizeMode, RenderError,
    RetroStaticSettings, RuttEtraSettings, ScanlineFilter, SortAxis, SortDirection, SortKey,
    StructureMode,
};

use crate::{
    FlowDisplaceDispatchPlan, GranularMosaicDispatchPlan, MetalDispatchError, RuttEtraDispatchPlan,
    ADVECT_FEEDBACK_KERNEL_NAME, ADVECT_FEEDBACK_SHADER_SOURCE, CHANNEL_SHIFT_KERNEL_NAME,
    CHANNEL_SHIFT_SHADER_SOURCE, COAGULATED_COMPOSITE_KERNEL_NAME,
    COAGULATED_COMPOSITE_SHADER_SOURCE, CONVOLUTION_BLEND_COLOR_KERNEL_NAME,
    CONVOLUTION_BLEND_COLOR_SHADER_SOURCE, CONVOLUTION_BLEND_KERNEL_NAME,
    CONVOLUTION_BLEND_SHADER_SOURCE, FIELD_PARTICLES_SPLAT_KERNEL_NAME,
    FIELD_PARTICLES_SPLAT_SHADER_SOURCE, FLOW_DISPLACE_KERNEL_NAME, FLOW_DISPLACE_SHADER_SOURCE,
    FLUID_ADVECT_KERNEL_NAME, FLUID_ADVECT_SHADER_SOURCE, FLUID_ADVECT_TWO_SOURCE_KERNEL_NAME,
    FLUID_ADVECT_TWO_SOURCE_SHADER_SOURCE, GRANULAR_MOSAIC_KERNEL_NAME,
    GRANULAR_MOSAIC_POOL_KERNEL_NAME, GRANULAR_MOSAIC_POOL_SHADER_SOURCE,
    GRANULAR_MOSAIC_SHADER_SOURCE, LUCAS_KANADE_REFINE_KERNEL_NAME,
    LUCAS_KANADE_REFINE_SHADER_SOURCE, MATTE_BLEND_KERNEL_NAME, MATTE_BLEND_SHADER_SOURCE,
    PALETTE_QUANTIZE_KERNEL_NAME, PALETTE_QUANTIZE_SHADER_SOURCE, PIXEL_SORT_KERNEL_NAME,
    PIXEL_SORT_SHADER_SOURCE, RETRO_STATIC_KERNEL_NAME, RETRO_STATIC_SHADER_SOURCE,
    RUTT_ETRA_KERNEL_NAME, RUTT_ETRA_SHADER_SOURCE, RUTT_ETRA_TWO_SOURCE_KERNEL_NAME,
    RUTT_ETRA_TWO_SOURCE_SHADER_SOURCE, VIDEO_VOCODER_MATCH_KERNEL_NAME,
    VIDEO_VOCODER_SHADER_SOURCE,
};

#[repr(C)]
struct FlowDisplaceParams {
    amount: f32,
    width: u32,
    height: u32,
}

#[repr(C)]
struct AdvectFeedbackParams {
    carrier_amount: f32,
    feedback_amount: f32,
    feedback_mix: f32,
    decay: f32,
    structure_mix: f32,
    width: u32,
    height: u32,
}

#[repr(C)]
struct GranularMosaicParams {
    rearrangement: f32,
    width: u32,
    height: u32,
    grain_size: u32,
    selection_columns: u32,
}

#[repr(C)]
struct GranularMosaicPoolParams {
    rearrangement: f32,
    width: u32,
    height: u32,
    grain_size: u32,
    selection_columns: u32,
}

#[repr(C)]
struct VideoVocoderParams {
    amount: f32,
    width: u32,
    height: u32,
}

#[repr(C)]
struct ConvolutionBlendParams {
    amount: f32,
    width: u32,
    height: u32,
    kernel_size: u32,
}

#[repr(C)]
struct FluidAdvectParams {
    advect: f32,
    turbulence_scale: f32,
    detail: f32,
    reinject: f32,
    time: f32,
    warp: f32,
    reinject_blotch: f32,
    diffuse: f32,
    blotch_scale: f32,
    width: u32,
    height: u32,
    seed_lo: u32,
    seed_hi: u32,
}

#[repr(C)]
struct FluidAdvectTwoSourceParams {
    advect: f32,
    reinject: f32,
    width: u32,
    height: u32,
}

#[repr(C)]
struct FieldParticlesSplatParams {
    width: u32,
    height: u32,
    particle_count: u32,
    particle_size: u32,
}

#[repr(C)]
struct LucasKanadeRefineParams {
    width: u32,
    height: u32,
    radius: i32,
}

#[repr(C)]
struct PixelSortMetalParams {
    width: u32,
    height: u32,
    axis: u32,
    key: u32,
    direction: u32,
    threshold_low: f32,
    threshold_high: f32,
    max_span: u32,
}

#[repr(C)]
struct RetroStaticMetalParams {
    width: u32,
    height: u32,
    real_bpp: u32,
    assumed_bpp: u32,
    filter: u32,
    strength: f32,
}

#[repr(C)]
struct ChannelShiftMetalParams {
    width: u32,
    height: u32,
    shift_r_x: f32,
    shift_r_y: f32,
    shift_g_x: f32,
    shift_g_y: f32,
    shift_b_x: f32,
    shift_b_y: f32,
}

#[repr(C)]
struct PaletteQuantizeMetalParams {
    width: u32,
    height: u32,
    mode: u32,   // 0 = posterize, 1 = neon palette
    levels: u32, // posterize only; 0 when mode != 0
}

#[repr(C)]
struct MatteBlendMetalParams {
    width: u32,
    height: u32,
}

#[repr(C)]
struct RuttEtraMetalParams {
    width: u32,
    height: u32,
    line_pitch: u32,
    displacement_depth: f32,
    line_thickness: u32,
    mono: u32,
}

#[repr(C)]
struct CoagulatedCompositeParams {
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
    patch_size: u32,
    seed_lo: u32,
    seed_hi: u32,
    edge_hardness: f32,
    edge_dither: f32,
    block_jitter: f32,
}

pub fn flow_displace_metal(
    carrier: &ImageBufferF32,
    flow: &FlowField,
    amount: f32,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if carrier.width != flow.width || carrier.height != flow.height {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "carrier is {}x{}, flow is {}x{}",
            carrier.width, carrier.height, flow.width, flow.height
        )));
    }

    let plan = FlowDisplaceDispatchPlan::new(carrier.width, carrier.height, amount)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(FLOW_DISPLACE_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(FLOW_DISPLACE_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let carrier_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let flow_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RG32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite | MTLTextureUsage::ShaderRead,
    );

    upload_rgba_f32_texture(&carrier_texture, carrier)?;
    upload_rg_f32_texture(&flow_texture, flow)?;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&carrier_texture));
    encoder.set_texture(1, Some(&flow_texture));
    encoder.set_texture(2, Some(&output_texture));

    let params = FlowDisplaceParams {
        amount: plan.amount,
        width: plan.width,
        height: plan.height,
    };
    encoder.set_bytes(
        0,
        std::mem::size_of::<FlowDisplaceParams>() as u64,
        (&params as *const FlowDisplaceParams).cast(),
    );
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

/// Video vocoder, histogram-specification (match) mode. Applies a precomputed
/// `tone` LUT (from `morphogen_render::luma_specification_tone_map`) to the
/// carrier on the GPU. The CPU reference `apply_tone_map_cpu` evaluates identical
/// math; the CLI gates this output against it per frame before export.
pub fn video_vocoder_match_metal(
    carrier: &ImageBufferF32,
    tone: &[f32],
    amount: f32,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if tone.is_empty() {
        return Err(MetalDispatchError::IncompatibleInputs(
            "tone map must be non-empty".to_string(),
        ));
    }

    let plan = FlowDisplaceDispatchPlan::new(carrier.width, carrier.height, amount)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(VIDEO_VOCODER_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(VIDEO_VOCODER_MATCH_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let carrier_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&carrier_texture, carrier)?;

    let tone_byte_len = std::mem::size_of_val(tone);
    let tone_buffer = device.new_buffer_with_data(
        tone.as_ptr().cast(),
        tone_byte_len as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&carrier_texture));
    encoder.set_texture(1, Some(&output_texture));
    encoder.set_buffer(0, Some(&tone_buffer), 0);

    let params = VideoVocoderParams {
        amount: plan.amount,
        width: plan.width,
        height: plan.height,
    };
    encoder.set_bytes(
        1,
        std::mem::size_of::<VideoVocoderParams>() as u64,
        (&params as *const VideoVocoderParams).cast(),
    );
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

/// Convolutional AV blend, image-kernel mode. Convolves the carrier with a
/// precomputed normalized `weights` kernel (`kernel_size × kernel_size`, from
/// `morphogen_render::analyze_convolution_kernel_cpu`) and blends by `amount`.
/// The CPU reference `convolution_blend_cpu` evaluates identical math; the CLI
/// gates this output against it per frame before export.
pub fn convolution_blend_metal(
    carrier: &ImageBufferF32,
    weights: &[f32],
    kernel_size: u32,
    amount: f32,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if kernel_size == 0 || kernel_size % 2 == 0 {
        return Err(MetalDispatchError::InvalidConvolutionSettings(
            "kernel_size must be odd and greater than zero".to_string(),
        ));
    }
    if weights.len() != (kernel_size * kernel_size) as usize {
        return Err(MetalDispatchError::InvalidConvolutionSettings(format!(
            "weights length {} does not match kernel_size {}",
            weights.len(),
            kernel_size
        )));
    }

    let plan = FlowDisplaceDispatchPlan::new(carrier.width, carrier.height, amount)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(CONVOLUTION_BLEND_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(CONVOLUTION_BLEND_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let carrier_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&carrier_texture, carrier)?;

    let weights_byte_len = std::mem::size_of_val(weights);
    let weights_buffer = device.new_buffer_with_data(
        weights.as_ptr().cast(),
        weights_byte_len as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&carrier_texture));
    encoder.set_texture(1, Some(&output_texture));
    encoder.set_buffer(0, Some(&weights_buffer), 0);

    let params = ConvolutionBlendParams {
        amount: plan.amount,
        width: plan.width,
        height: plan.height,
        kernel_size,
    };
    encoder.set_bytes(
        1,
        std::mem::size_of::<ConvolutionBlendParams>() as u64,
        (&params as *const ConvolutionBlendParams).cast(),
    );
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

/// Convolutional AV blend, per-channel **colour** mode. Convolves each carrier
/// channel (R/G/B) with its own normalized `weights_{r,g,b}` kernel
/// (`kernel_size × kernel_size`, from
/// `morphogen_render::analyze_convolution_kernels_color_cpu`) and blends by
/// `amount`. The CPU reference `convolution_blend_color_cpu` evaluates identical
/// math; the CLI gates this output against it per frame before export.
pub fn convolution_blend_color_metal(
    carrier: &ImageBufferF32,
    weights_r: &[f32],
    weights_g: &[f32],
    weights_b: &[f32],
    kernel_size: u32,
    amount: f32,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if kernel_size == 0 || kernel_size % 2 == 0 {
        return Err(MetalDispatchError::InvalidConvolutionSettings(
            "kernel_size must be odd and greater than zero".to_string(),
        ));
    }
    let expected = (kernel_size * kernel_size) as usize;
    for weights in [weights_r, weights_g, weights_b] {
        if weights.len() != expected {
            return Err(MetalDispatchError::InvalidConvolutionSettings(format!(
                "weights length {} does not match kernel_size {}",
                weights.len(),
                kernel_size
            )));
        }
    }

    let plan = FlowDisplaceDispatchPlan::new(carrier.width, carrier.height, amount)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(CONVOLUTION_BLEND_COLOR_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(CONVOLUTION_BLEND_COLOR_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let carrier_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&carrier_texture, carrier)?;

    let make_buffer = |weights: &[f32]| {
        device.new_buffer_with_data(
            weights.as_ptr().cast(),
            std::mem::size_of_val(weights) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    };
    let buffer_r = make_buffer(weights_r);
    let buffer_g = make_buffer(weights_g);
    let buffer_b = make_buffer(weights_b);

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&carrier_texture));
    encoder.set_texture(1, Some(&output_texture));
    encoder.set_buffer(0, Some(&buffer_r), 0);
    encoder.set_buffer(1, Some(&buffer_g), 0);
    encoder.set_buffer(2, Some(&buffer_b), 0);

    let params = ConvolutionBlendParams {
        amount: plan.amount,
        width: plan.width,
        height: plan.height,
        kernel_size,
    };
    encoder.set_bytes(
        3,
        std::mem::size_of::<ConvolutionBlendParams>() as u64,
        (&params as *const ConvolutionBlendParams).cast(),
    );
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

pub fn flow_feedback_metal(
    carrier: &ImageBufferF32,
    previous_output: Option<&ImageBufferF32>,
    flow: &FlowField,
    settings: FlowFeedbackSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    settings
        .validate()
        .map_err(|error| MetalDispatchError::InvalidFeedbackSettings(error.to_string()))?;
    if settings.structure_mode == StructureMode::Multiscale {
        return Err(MetalDispatchError::InvalidFeedbackSettings(
            "multiscale structure mode is CPU-only; use --backend cpu".to_string(),
        ));
    }
    if carrier.width != flow.width || carrier.height != flow.height {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "carrier is {}x{}, flow is {}x{}",
            carrier.width, carrier.height, flow.width, flow.height
        )));
    }
    let Some(previous_output) = previous_output else {
        return flow_displace_metal(carrier, flow, settings.carrier_amount);
    };
    if previous_output.width != carrier.width || previous_output.height != carrier.height {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "previous output is {}x{}, carrier is {}x{}",
            previous_output.width, previous_output.height, carrier.width, carrier.height
        )));
    }

    let plan =
        FlowDisplaceDispatchPlan::new(carrier.width, carrier.height, settings.carrier_amount)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(ADVECT_FEEDBACK_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(ADVECT_FEEDBACK_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let carrier_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let previous_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let flow_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RG32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&carrier_texture, carrier)?;
    upload_rgba_f32_texture(&previous_texture, previous_output)?;
    upload_rg_f32_texture(&flow_texture, flow)?;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&carrier_texture));
    encoder.set_texture(1, Some(&previous_texture));
    encoder.set_texture(2, Some(&flow_texture));
    encoder.set_texture(3, Some(&output_texture));
    let params = AdvectFeedbackParams {
        carrier_amount: settings.carrier_amount,
        feedback_amount: settings.feedback_amount,
        feedback_mix: settings.feedback_mix,
        decay: settings.decay,
        structure_mix: settings.structure_mix,
        width: plan.width,
        height: plan.height,
    };
    encoder.set_bytes(
        0,
        std::mem::size_of::<AdvectFeedbackParams>() as u64,
        (&params as *const AdvectFeedbackParams).cast(),
    );
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

/// Faux-fluid dye advection on the GPU — the Metal port of
/// `morphogen_render::fluid_advect_frame_cpu`. Frame zero (`previous == None`) returns the
/// source verbatim. Otherwise the frame is advanced in `effective_substeps()` kernel
/// passes within one command buffer, ping-ponging between two dye textures exactly as the
/// CPU reference loops `advect_substep` (per-substep step, compound reinjection rate and
/// interpolated field time all come from the shared `FluidAdvectSettings` helpers).
/// Compiled with fast-math disabled so the float math and the splitmix64 integer hashing
/// match the CPU reference; the parity test gates this output against
/// `fluid_advect_frame_cpu` frame-by-frame.
pub fn fluid_advect_metal(
    source: &ImageBufferF32,
    previous: Option<&ImageBufferF32>,
    frame_index: u32,
    settings: FluidAdvectSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    settings
        .validate()
        .map_err(|error| MetalDispatchError::InvalidFluidAdvectSettings(error.to_string()))?;

    let Some(previous) = previous else {
        // Frame zero: the dye is seeded from the source frame verbatim.
        return Ok(source.clone());
    };

    if previous.width != source.width || previous.height != source.height {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "previous dye is {}x{}, source is {}x{}",
            previous.width, previous.height, source.width, source.height
        )));
    }

    // Mirror the CPU reference's byte-exact off case (see fluid_advect_frame_cpu).
    if settings.reinject >= 1.0 && settings.reinject_blotch == 0.0 {
        return Ok(source.clone());
    }

    let substeps = settings.effective_substeps();
    let step = settings.advect / substeps as f32;
    let reinject = settings.per_substep_reinject(substeps);

    let plan = FlowDisplaceDispatchPlan::new(source.width, source.height, settings.advect)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(FLUID_ADVECT_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(FLUID_ADVECT_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let source_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    // Ping-pong dye textures: each substep reads one and writes the other.
    let dye_textures = [
        new_texture(
            &device,
            plan.width,
            plan.height,
            MTLPixelFormat::RGBA32Float,
            MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite,
        ),
        new_texture(
            &device,
            plan.width,
            plan.height,
            MTLPixelFormat::RGBA32Float,
            MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite,
        ),
    ];
    upload_rgba_f32_texture(&source_texture, source)?;
    upload_rgba_f32_texture(&dye_textures[0], previous)?;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    for substep in 0..substeps {
        let read_index = (substep % 2) as usize;
        let encoder = command_buffer.new_compute_command_encoder();
        encoder.set_compute_pipeline_state(&pipeline);
        encoder.set_texture(0, Some(&source_texture));
        encoder.set_texture(1, Some(&dye_textures[read_index]));
        encoder.set_texture(2, Some(&dye_textures[1 - read_index]));
        let params = FluidAdvectParams {
            advect: step,
            turbulence_scale: settings.turbulence_scale,
            detail: settings.detail,
            reinject,
            time: settings.substep_time(frame_index, substep, substeps),
            warp: settings.warp,
            reinject_blotch: settings.reinject_blotch,
            diffuse: settings.diffuse,
            blotch_scale: settings.blotch_lattice_scale(),
            width: plan.width,
            height: plan.height,
            seed_lo: (settings.seed & 0xFFFF_FFFF) as u32,
            seed_hi: (settings.seed >> 32) as u32,
        };
        encoder.set_bytes(
            0,
            std::mem::size_of::<FluidAdvectParams>() as u64,
            (&params as *const FluidAdvectParams).cast(),
        );
        encoder.dispatch_thread_groups(
            MTLSize::new(
                plan.threadgroups_per_grid.width as u64,
                plan.threadgroups_per_grid.height as u64,
                plan.threadgroups_per_grid.depth as u64,
            ),
            MTLSize::new(
                plan.threads_per_threadgroup.width as u64,
                plan.threads_per_threadgroup.height as u64,
                plan.threads_per_threadgroup.depth as u64,
            ),
        );
        encoder.end_encoding();
    }
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    let final_index = (substeps % 2) as usize;
    read_rgba_f32_texture(&dye_textures[final_index], plan.width, plan.height)
}

/// Two-source faux-fluid advection on the GPU — the Metal port of
/// `morphogen_render::fluid_advect_two_source_frame_cpu`. Source A's flow advects the
/// `previous` dye (the parity-gated displace) and a fraction of the current B frame is
/// reinjected, in one pass. Frame zero (B verbatim) is handled by the caller, so `previous`
/// is always present here. Compiled with fast-math disabled so the float math matches the CPU
/// reference; the CLI gates this output against the CPU per frame.
pub fn fluid_advect_two_source_metal(
    carrier_b: &ImageBufferF32,
    previous: &ImageBufferF32,
    flow: &FlowField,
    settings: FluidAdvectTwoSourceSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    settings
        .validate()
        .map_err(|error| MetalDispatchError::InvalidFluidAdvectSettings(error.to_string()))?;

    if previous.width != carrier_b.width || previous.height != carrier_b.height {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "previous dye is {}x{}, carrier B is {}x{}",
            previous.width, previous.height, carrier_b.width, carrier_b.height
        )));
    }
    if flow.width != carrier_b.width || flow.height != carrier_b.height {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "Source A flow is {}x{}, carrier B is {}x{}",
            flow.width, flow.height, carrier_b.width, carrier_b.height
        )));
    }

    let plan = FlowDisplaceDispatchPlan::new(carrier_b.width, carrier_b.height, settings.advect)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(FLUID_ADVECT_TWO_SOURCE_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(FLUID_ADVECT_TWO_SOURCE_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let carrier_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let previous_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let flow_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RG32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&carrier_texture, carrier_b)?;
    upload_rgba_f32_texture(&previous_texture, previous)?;
    upload_rg_f32_texture(&flow_texture, flow)?;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&carrier_texture));
    encoder.set_texture(1, Some(&previous_texture));
    encoder.set_texture(2, Some(&flow_texture));
    encoder.set_texture(3, Some(&output_texture));
    let params = FluidAdvectTwoSourceParams {
        advect: settings.advect,
        reinject: settings.reinject,
        width: plan.width,
        height: plan.height,
    };
    encoder.set_bytes(
        0,
        std::mem::size_of::<FluidAdvectTwoSourceParams>() as u64,
        (&params as *const FluidAdvectTwoSourceParams).cast(),
    );
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

/// Discrete-carrier particle splat on the GPU — the Metal port of
/// `morphogen_render::render_field_particles`. The particle state is computed on the CPU; this
/// rasterizes it. Each output pixel gathers the last (highest-index) particle whose
/// `particle_size` square covers it (matching the CPU last-writer-wins scatter byte-for-byte,
/// since positions are the CPU floats uploaded verbatim). O(width·height·particles) — a
/// correctness-first kernel; a tiled scatter is the perf follow-up.
pub fn field_particles_splat_metal(
    field: &ParticleField,
    settings: FieldParticleSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    settings
        .validate()
        .map_err(|error| MetalDispatchError::InvalidFieldParticlesSettings(error.to_string()))?;

    let (width, height) = field.dimensions();
    let particle_count = field.particle_count() as u32;
    let splat = field.splat_buffer();

    let plan = FlowDisplaceDispatchPlan::new(width, height, 0.0)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(FIELD_PARTICLES_SPLAT_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(FIELD_PARTICLES_SPLAT_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );

    // A zero-length buffer is rejected by Metal; an empty field is just the black canvas, and a
    // one-float stub keeps the binding valid while `particle_count == 0` skips the loop.
    let buffer_data: &[f32] = if splat.is_empty() { &[0.0] } else { &splat };
    let particle_buffer = device.new_buffer_with_data(
        buffer_data.as_ptr().cast(),
        std::mem::size_of_val(buffer_data) as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&output_texture));
    encoder.set_buffer(0, Some(&particle_buffer), 0);
    let params = FieldParticlesSplatParams {
        width: plan.width,
        height: plan.height,
        particle_count,
        particle_size: settings.particle_size,
    };
    encoder.set_bytes(
        1,
        std::mem::size_of::<FieldParticlesSplatParams>() as u64,
        (&params as *const FieldParticlesSplatParams).cast(),
    );
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

/// Pyramidal Lucas-Kanade optical flow with the dense per-level refinement run on the
/// GPU. The pyramid build, upsample, forward/backward consistency filter and output
/// resample stay on the CPU (shared with `pyramidal_lucas_kanade_flow_cpu`), so the only
/// GPU parity surface is the `lucas_kanade_refine` kernel. The device, library and
/// pipeline are built once and reused across every level/iteration/direction.
///
/// The result is not byte-identical to the CPU reference (GPU float rounding differs), so
/// the caller must gate it — the CLI validates frame 0 against the CPU within tolerance,
/// then trusts the GPU for the remaining frames of a render.
pub fn pyramidal_lucas_kanade_flow_metal(
    previous: &ImageBufferF32,
    current: &ImageBufferF32,
    width: u32,
    height: u32,
    window_radius: i32,
) -> Result<PyramidalLucasKanadeEstimate, MetalDispatchError> {
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(LUCAS_KANADE_REFINE_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(LUCAS_KANADE_REFINE_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;
    let command_queue = device.new_command_queue();

    pyramidal_lucas_kanade_flow_with_refiner(
        previous,
        current,
        width,
        height,
        window_radius,
        |level_previous: &[f32],
         level_current: &[f32],
         level_width: u32,
         level_height: u32,
         flow: &mut [[f32; 2]],
         radius: i32,
         iterations: usize| {
            refine_level_metal(
                &device,
                &pipeline,
                &command_queue,
                level_previous,
                level_current,
                level_width,
                level_height,
                flow,
                radius,
                iterations,
            )
            .map_err(|error| RenderError::InvalidFlowField(error.to_string()))
        },
    )
    .map_err(|error| MetalDispatchError::OpticalFlow(error.to_string()))
}

/// GPU implementation of a single pyramid-level refinement (all warp iterations). Mirrors
/// `morphogen_render::refine_level_cpu`: the luminance levels are uploaded as R32Float
/// textures, the flow is double-buffered between RG32Float textures (one iteration per
/// command buffer so writes are visible to the next read), and the final flow and
/// per-pixel confidence are read back.
#[allow(clippy::too_many_arguments)]
fn refine_level_metal(
    device: &Device,
    pipeline: &ComputePipelineState,
    command_queue: &CommandQueue,
    previous: &[f32],
    current: &[f32],
    width: u32,
    height: u32,
    flow: &mut [[f32; 2]],
    radius: i32,
    iterations: usize,
) -> Result<Vec<f32>, MetalDispatchError> {
    if width == 0 || height == 0 {
        return Err(MetalDispatchError::EmptyDimensions);
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?;
    if previous.len() != expected || current.len() != expected || flow.len() != expected {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "lucas-kanade refine level expects {expected} samples per buffer"
        )));
    }

    let plan = FlowDisplaceDispatchPlan::new(width, height, 0.0)?;

    let previous_texture = new_texture(
        device,
        width,
        height,
        MTLPixelFormat::R32Float,
        MTLTextureUsage::ShaderRead,
    );
    let current_texture = new_texture(
        device,
        width,
        height,
        MTLPixelFormat::R32Float,
        MTLTextureUsage::ShaderRead,
    );
    upload_r_f32_texture(&previous_texture, previous, width, height)?;
    upload_r_f32_texture(&current_texture, current, width, height)?;

    let mut flow_read = new_texture(
        device,
        width,
        height,
        MTLPixelFormat::RG32Float,
        MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite,
    );
    let mut flow_write = new_texture(
        device,
        width,
        height,
        MTLPixelFormat::RG32Float,
        MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite,
    );
    upload_rg_f32_texture_slice(&flow_read, flow, width, height)?;

    let confidence_texture = new_texture(
        device,
        width,
        height,
        MTLPixelFormat::R32Float,
        MTLTextureUsage::ShaderWrite,
    );

    let params = LucasKanadeRefineParams {
        width,
        height,
        radius,
    };

    for _ in 0..iterations.max(1) {
        let command_buffer = command_queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();
        encoder.set_compute_pipeline_state(pipeline);
        encoder.set_texture(0, Some(&previous_texture));
        encoder.set_texture(1, Some(&current_texture));
        encoder.set_texture(2, Some(&flow_read));
        encoder.set_texture(3, Some(&flow_write));
        encoder.set_texture(4, Some(&confidence_texture));
        encoder.set_bytes(
            0,
            std::mem::size_of::<LucasKanadeRefineParams>() as u64,
            (&params as *const LucasKanadeRefineParams).cast(),
        );
        encoder.dispatch_thread_groups(
            MTLSize::new(
                plan.threadgroups_per_grid.width as u64,
                plan.threadgroups_per_grid.height as u64,
                plan.threadgroups_per_grid.depth as u64,
            ),
            MTLSize::new(
                plan.threads_per_threadgroup.width as u64,
                plan.threads_per_threadgroup.height as u64,
                plan.threads_per_threadgroup.depth as u64,
            ),
        );
        encoder.end_encoding();
        command_buffer.commit();
        command_buffer.wait_until_completed();

        let status = command_buffer.status();
        if status != MTLCommandBufferStatus::Completed {
            return Err(MetalDispatchError::CommandBufferFailed(format!(
                "{status:?}"
            )));
        }

        // The iteration just wrote `flow_write`; make it the read source for the next.
        std::mem::swap(&mut flow_read, &mut flow_write);
    }

    let refined = read_rg_f32_texture(&flow_read, width, height)?;
    flow.copy_from_slice(&refined);
    read_r_f32_texture(&confidence_texture, width, height)
}

/// Descriptor-coagulated flow blend — composite stage on the GPU. Given Source A,
/// Source B, and the CPU-built `cols × rows` ownership field, evaluates the same
/// per-pixel block-jitter + bilinear field sample + dithered hard/soft edge blend +
/// A/B lerp as `morphogen_render::composite_with_field`. Compiled with fast-math
/// disabled so the float math (and the hard-edge threshold) matches the CPU
/// reference bit-for-bit; the CLI gates this output against it per frame.
#[allow(clippy::too_many_arguments)]
pub fn coagulated_composite_metal(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    field_weights: &[f32],
    cols: u32,
    rows: u32,
    patch_size: u32,
    edge_hardness: f32,
    edge_dither: f32,
    block_jitter: f32,
    seed: u64,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if source_a.width != source_b.width || source_a.height != source_b.height {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "source A is {}x{}, source B is {}x{}",
            source_a.width, source_a.height, source_b.width, source_b.height
        )));
    }
    if patch_size == 0 {
        return Err(MetalDispatchError::InvalidCoagulationSettings(
            "patch_size must be greater than zero".to_string(),
        ));
    }
    if field_weights.len() != (cols as usize) * (rows as usize) {
        return Err(MetalDispatchError::InvalidCoagulationSettings(format!(
            "field length {} does not match {}x{} cells",
            field_weights.len(),
            cols,
            rows
        )));
    }

    let plan = FlowDisplaceDispatchPlan::new(source_a.width, source_a.height, 0.0)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(COAGULATED_COMPOSITE_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(COAGULATED_COMPOSITE_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let source_a_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let source_b_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&source_a_texture, source_a)?;
    upload_rgba_f32_texture(&source_b_texture, source_b)?;

    let weights_byte_len = std::mem::size_of_val(field_weights);
    let weights_buffer = device.new_buffer_with_data(
        field_weights.as_ptr().cast(),
        weights_byte_len as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&source_a_texture));
    encoder.set_texture(1, Some(&source_b_texture));
    encoder.set_texture(2, Some(&output_texture));
    encoder.set_buffer(0, Some(&weights_buffer), 0);

    let params = CoagulatedCompositeParams {
        width: plan.width,
        height: plan.height,
        cols,
        rows,
        patch_size,
        seed_lo: seed as u32,
        seed_hi: (seed >> 32) as u32,
        edge_hardness,
        edge_dither,
        block_jitter,
    };
    encoder.set_bytes(
        1,
        std::mem::size_of::<CoagulatedCompositeParams>() as u64,
        (&params as *const CoagulatedCompositeParams).cast(),
    );
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

pub fn granular_mosaic_metal(
    carrier: &ImageBufferF32,
    selection: &GrainSelection,
    settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    settings
        .validate()
        .map_err(|error| MetalDispatchError::InvalidGranularMosaicSettings(error.to_string()))?;
    let plan = GranularMosaicDispatchPlan::new(
        carrier.width,
        carrier.height,
        settings.grain_size,
        settings.rearrangement,
    )?;
    validate_grain_selection(carrier, selection, settings.grain_size)?;
    let selection_byte_len = selection
        .indices
        .len()
        .checked_mul(std::mem::size_of::<u32>())
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?;
    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(GRANULAR_MOSAIC_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(GRANULAR_MOSAIC_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;
    let carrier_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&carrier_texture, carrier)?;
    let selection_buffer = device.new_buffer_with_data(
        selection.indices.as_ptr().cast(),
        selection_byte_len as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&carrier_texture));
    encoder.set_texture(1, Some(&output_texture));
    let params = GranularMosaicParams {
        rearrangement: plan.rearrangement,
        width: plan.width,
        height: plan.height,
        grain_size: plan.grain_size,
        selection_columns: selection.columns,
    };
    encoder.set_bytes(
        0,
        std::mem::size_of::<GranularMosaicParams>() as u64,
        (&params as *const GranularMosaicParams).cast(),
    );
    encoder.set_buffer(1, Some(&selection_buffer), 0);
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }
    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

/// Render a temporal-grain-pool mosaic (granular step 6b) on the GPU, gated by
/// the caller against [`morphogen_render::granular_mosaic_with_pool_selection_cpu`].
/// The whole-clip pool is uploaded as a 2D texture array (one slice per pool
/// frame); a flat grain-metadata buffer resolves each global pool index to its
/// `(frame_index, origin_x, origin_y)`. Sampling is integer-nearest clamped to
/// match the CPU reference, and `rearrangement` value-blends the current carrier
/// pixel with the selected grain's pixel.
pub fn granular_mosaic_pool_metal(
    pool_frames: &[ImageBufferF32],
    pool: &GrainPool,
    carrier: &ImageBufferF32,
    selection: &GrainSelection,
    settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    settings
        .validate()
        .map_err(|error| MetalDispatchError::InvalidGranularMosaicSettings(error.to_string()))?;
    let plan = GranularMosaicDispatchPlan::new(
        carrier.width,
        carrier.height,
        settings.grain_size,
        settings.rearrangement,
    )?;
    validate_pool_render_inputs(pool_frames, pool, carrier, selection, settings.grain_size)?;

    // Flatten the pool descriptors into a `[frame_index, origin_x, origin_y]`
    // triple per global grain index, matching the shader's `grainMeta` layout.
    let mut grain_meta = Vec::with_capacity(
        pool.grains
            .len()
            .checked_mul(3)
            .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?,
    );
    for grain in &pool.grains {
        grain_meta.push(grain.frame_index);
        grain_meta.push(grain.origin_x);
        grain_meta.push(grain.origin_y);
    }
    let grain_meta_byte_len = grain_meta
        .len()
        .checked_mul(std::mem::size_of::<u32>())
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?;
    let selection_byte_len = selection
        .indices
        .len()
        .checked_mul(std::mem::size_of::<u32>())
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?;

    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(GRANULAR_MOSAIC_POOL_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(GRANULAR_MOSAIC_POOL_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let carrier_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        plan.width,
        plan.height,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&carrier_texture, carrier)?;

    let pool_texture = new_array_texture(
        &device,
        pool.frame_width,
        pool.frame_height,
        pool_frames.len() as u32,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    for (slice, frame) in pool_frames.iter().enumerate() {
        upload_rgba_f32_texture_slice(&pool_texture, frame, slice as u32)?;
    }

    let selection_buffer = device.new_buffer_with_data(
        selection.indices.as_ptr().cast(),
        selection_byte_len as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let grain_meta_buffer = device.new_buffer_with_data(
        grain_meta.as_ptr().cast(),
        grain_meta_byte_len as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&carrier_texture));
    encoder.set_texture(1, Some(&output_texture));
    encoder.set_texture(2, Some(&pool_texture));
    let params = GranularMosaicPoolParams {
        rearrangement: plan.rearrangement,
        width: plan.width,
        height: plan.height,
        grain_size: plan.grain_size,
        selection_columns: selection.columns,
    };
    encoder.set_bytes(
        0,
        std::mem::size_of::<GranularMosaicPoolParams>() as u64,
        (&params as *const GranularMosaicPoolParams).cast(),
    );
    encoder.set_buffer(1, Some(&selection_buffer), 0);
    encoder.set_buffer(2, Some(&grain_meta_buffer), 0);
    encoder.dispatch_thread_groups(
        MTLSize::new(
            plan.threadgroups_per_grid.width as u64,
            plan.threadgroups_per_grid.height as u64,
            plan.threadgroups_per_grid.depth as u64,
        ),
        MTLSize::new(
            plan.threads_per_threadgroup.width as u64,
            plan.threads_per_threadgroup.height as u64,
            plan.threads_per_threadgroup.depth as u64,
        ),
    );
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }
    read_rgba_f32_texture(&output_texture, plan.width, plan.height)
}

fn validate_pool_render_inputs(
    pool_frames: &[ImageBufferF32],
    pool: &GrainPool,
    carrier: &ImageBufferF32,
    selection: &GrainSelection,
    grain_size: u32,
) -> Result<(), MetalDispatchError> {
    if pool_frames.is_empty() {
        return Err(MetalDispatchError::InvalidGranularMosaicSettings(
            "grain pool render requires at least one pool frame".to_string(),
        ));
    }
    if carrier.width != pool.frame_width || carrier.height != pool.frame_height {
        return Err(MetalDispatchError::InvalidGranularMosaicSettings(
            "carrier dimensions do not match the grain pool".to_string(),
        ));
    }
    if pool_frames
        .iter()
        .any(|frame| frame.width != pool.frame_width || frame.height != pool.frame_height)
    {
        return Err(MetalDispatchError::InvalidGranularMosaicSettings(
            "every pool frame must share the grain pool dimensions".to_string(),
        ));
    }

    let columns = div_ceil(carrier.width, grain_size);
    let rows = div_ceil(carrier.height, grain_size);
    let expected_count = (columns as usize)
        .checked_mul(rows as usize)
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?;
    if selection.columns != columns
        || selection.rows != rows
        || selection.indices.len() != expected_count
        || selection
            .indices
            .iter()
            .any(|index| *index as usize >= pool.grains.len())
    {
        return Err(MetalDispatchError::InvalidGranularMosaicSettings(
            "grain selection does not match the carrier grid or references a missing pool grain"
                .to_string(),
        ));
    }
    if pool
        .grains
        .iter()
        .any(|grain| grain.frame_index as usize >= pool_frames.len())
    {
        return Err(MetalDispatchError::InvalidGranularMosaicSettings(
            "grain pool references a frame outside the supplied pool frames".to_string(),
        ));
    }
    Ok(())
}

fn validate_grain_selection(
    carrier: &ImageBufferF32,
    selection: &GrainSelection,
    grain_size: u32,
) -> Result<(), MetalDispatchError> {
    let columns = div_ceil(carrier.width, grain_size);
    let rows = div_ceil(carrier.height, grain_size);
    let expected_count = (columns as usize)
        .checked_mul(rows as usize)
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?;
    if selection.columns != columns
        || selection.rows != rows
        || selection.indices.len() != expected_count
        || selection
            .indices
            .iter()
            .any(|index| *index as usize >= expected_count)
    {
        return Err(MetalDispatchError::InvalidGranularMosaicSettings(
            "grain selection does not match the carrier grain grid".to_string(),
        ));
    }
    Ok(())
}

fn new_texture(
    device: &Device,
    width: u32,
    height: u32,
    pixel_format: MTLPixelFormat,
    usage: MTLTextureUsage,
) -> Texture {
    let descriptor = TextureDescriptor::new();
    descriptor.set_texture_type(MTLTextureType::D2);
    descriptor.set_pixel_format(pixel_format);
    descriptor.set_width(width as u64);
    descriptor.set_height(height as u64);
    descriptor.set_storage_mode(MTLStorageMode::Shared);
    descriptor.set_usage(usage);
    device.new_texture(&descriptor)
}

fn new_array_texture(
    device: &Device,
    width: u32,
    height: u32,
    array_length: u32,
    pixel_format: MTLPixelFormat,
    usage: MTLTextureUsage,
) -> Texture {
    let descriptor = TextureDescriptor::new();
    descriptor.set_texture_type(MTLTextureType::D2Array);
    descriptor.set_pixel_format(pixel_format);
    descriptor.set_width(width as u64);
    descriptor.set_height(height as u64);
    descriptor.set_array_length(array_length as u64);
    descriptor.set_storage_mode(MTLStorageMode::Shared);
    descriptor.set_usage(usage);
    device.new_texture(&descriptor)
}

fn upload_rgba_f32_texture(
    texture: &Texture,
    image: &ImageBufferF32,
) -> Result<(), MetalDispatchError> {
    let bytes = rgba_f32_bytes(&image.pixels)?;
    replace_texture_bytes(texture, image.width, image.height, 16, 0, &bytes)
}

fn upload_rgba_f32_texture_slice(
    texture: &Texture,
    image: &ImageBufferF32,
    slice: u32,
) -> Result<(), MetalDispatchError> {
    let bytes = rgba_f32_bytes(&image.pixels)?;
    replace_texture_bytes(texture, image.width, image.height, 16, slice, &bytes)
}

fn upload_rg_f32_texture(texture: &Texture, flow: &FlowField) -> Result<(), MetalDispatchError> {
    let bytes = rg_f32_bytes(&flow.vectors)?;
    replace_texture_bytes(texture, flow.width, flow.height, 8, 0, &bytes)
}

fn upload_rg_f32_texture_slice(
    texture: &Texture,
    vectors: &[[f32; 2]],
    width: u32,
    height: u32,
) -> Result<(), MetalDispatchError> {
    let bytes = rg_f32_bytes(vectors)?;
    replace_texture_bytes(texture, width, height, 8, 0, &bytes)
}

fn upload_r_f32_texture(
    texture: &Texture,
    values: &[f32],
    width: u32,
    height: u32,
) -> Result<(), MetalDispatchError> {
    let byte_len = values
        .len()
        .checked_mul(4)
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?;
    let mut bytes = Vec::with_capacity(byte_len);
    for value in values {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }
    replace_texture_bytes(texture, width, height, 4, 0, &bytes)
}

fn read_rg_f32_texture(
    texture: &Texture,
    width: u32,
    height: u32,
) -> Result<Vec<[f32; 2]>, MetalDispatchError> {
    let bytes_per_row = checked_row_bytes(width, 8)?;
    let image_stride = checked_image_bytes(height, bytes_per_row)?;
    let mut bytes = vec![0; image_stride];
    texture.get_bytes_in_slice(
        bytes.as_mut_ptr().cast(),
        bytes_per_row as u64,
        image_stride as u64,
        MTLRegion::new_2d(0, 0, width as u64, height as u64),
        0,
        0,
    );

    let mut vectors = Vec::with_capacity(
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?,
    );
    for chunk in bytes.chunks_exact(8) {
        vectors.push([
            f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            f32::from_ne_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
        ]);
    }
    Ok(vectors)
}

fn read_r_f32_texture(
    texture: &Texture,
    width: u32,
    height: u32,
) -> Result<Vec<f32>, MetalDispatchError> {
    let bytes_per_row = checked_row_bytes(width, 4)?;
    let image_stride = checked_image_bytes(height, bytes_per_row)?;
    let mut bytes = vec![0; image_stride];
    texture.get_bytes_in_slice(
        bytes.as_mut_ptr().cast(),
        bytes_per_row as u64,
        image_stride as u64,
        MTLRegion::new_2d(0, 0, width as u64, height as u64),
        0,
        0,
    );

    let mut values = Vec::with_capacity(
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?,
    );
    for chunk in bytes.chunks_exact(4) {
        values.push(f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(values)
}

fn replace_texture_bytes(
    texture: &Texture,
    width: u32,
    height: u32,
    bytes_per_pixel: usize,
    slice: u32,
    bytes: &[u8],
) -> Result<(), MetalDispatchError> {
    let bytes_per_row = checked_row_bytes(width, bytes_per_pixel)?;
    let image_stride = checked_image_bytes(height, bytes_per_row)?;
    texture.replace_region_in_slice(
        MTLRegion::new_2d(0, 0, width as u64, height as u64),
        0,
        slice as u64,
        bytes.as_ptr().cast(),
        bytes_per_row as u64,
        image_stride as u64,
    );
    Ok(())
}

fn read_rgba_f32_texture(
    texture: &Texture,
    width: u32,
    height: u32,
) -> Result<ImageBufferF32, MetalDispatchError> {
    let bytes_per_row = checked_row_bytes(width, 16)?;
    let image_stride = checked_image_bytes(height, bytes_per_row)?;
    let mut bytes = vec![0; image_stride];
    texture.get_bytes_in_slice(
        bytes.as_mut_ptr().cast(),
        bytes_per_row as u64,
        image_stride as u64,
        MTLRegion::new_2d(0, 0, width as u64, height as u64),
        0,
        0,
    );

    let mut pixels = Vec::with_capacity(
        (width as usize)
            .checked_mul(height as usize)
            .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?,
    );
    for chunk in bytes.chunks_exact(16) {
        pixels.push([
            f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            f32::from_ne_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
            f32::from_ne_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]),
            f32::from_ne_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]),
        ]);
    }

    ImageBufferF32::new(width, height, pixels)
        .map_err(|error| MetalDispatchError::CommandBufferFailed(error.to_string()))
}

fn rgba_f32_bytes(pixels: &[[f32; 4]]) -> Result<Vec<u8>, MetalDispatchError> {
    let byte_len = pixels
        .len()
        .checked_mul(16)
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?;
    let mut bytes = Vec::with_capacity(byte_len);
    for pixel in pixels {
        for channel in pixel {
            bytes.extend_from_slice(&channel.to_ne_bytes());
        }
    }
    Ok(bytes)
}

fn rg_f32_bytes(vectors: &[[f32; 2]]) -> Result<Vec<u8>, MetalDispatchError> {
    let byte_len = vectors
        .len()
        .checked_mul(8)
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)?;
    let mut bytes = Vec::with_capacity(byte_len);
    for vector in vectors {
        for channel in vector {
            bytes.extend_from_slice(&channel.to_ne_bytes());
        }
    }
    Ok(bytes)
}

fn checked_row_bytes(width: u32, bytes_per_pixel: usize) -> Result<usize, MetalDispatchError> {
    (width as usize)
        .checked_mul(bytes_per_pixel)
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)
}

fn checked_image_bytes(height: u32, bytes_per_row: usize) -> Result<usize, MetalDispatchError> {
    (height as usize)
        .checked_mul(bytes_per_row)
        .ok_or(MetalDispatchError::TextureByteLengthTooLarge)
}

fn div_ceil(value: u32, divisor: u32) -> u32 {
    value / divisor + u32::from(value % divisor != 0)
}

/// Pixel-sort effect on the GPU. Mirrors `render_pixel_sort_frame` exactly.
/// One threadgroup per line (row or column); all threads cooperate on load/store;
/// thread 0 runs the stable insertion sort. The CPU parity gate in the CLI driver
/// runs every frame before writing output (standard stateless per-frame gate).
pub fn pixel_sort_metal(
    source: &ImageBufferF32,
    settings: &PixelSortSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if source.width == 0 || source.height == 0 {
        return Err(MetalDispatchError::EmptyDimensions);
    }

    let w = source.width;
    let h = source.height;

    let axis_u32: u32 = match settings.axis {
        SortAxis::Row => 0,
        SortAxis::Col => 1,
    };
    let key_u32: u32 = match settings.key {
        SortKey::Luma => 0,
        SortKey::Hue => 1,
        SortKey::Sat => 2,
        SortKey::Red => 3,
        SortKey::Green => 4,
        SortKey::Blue => 5,
    };
    let dir_u32: u32 = match settings.direction {
        SortDirection::Asc => 0,
        SortDirection::Desc => 1,
    };

    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(PIXEL_SORT_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(PIXEL_SORT_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let source_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&source_texture, source)?;

    let params = PixelSortMetalParams {
        width: w,
        height: h,
        axis: axis_u32,
        key: key_u32,
        direction: dir_u32,
        threshold_low: settings.threshold_low,
        threshold_high: settings.threshold_high,
        max_span: settings.max_span,
    };

    // One threadgroup per line; threads share load/store. Thread count = min(line_len, 64).
    let (threadgroups, threads_per_tg) = match settings.axis {
        SortAxis::Row => (
            MTLSize::new(1, h as u64, 1),
            MTLSize::new(64_u64.min(w as u64), 1, 1),
        ),
        SortAxis::Col => (
            MTLSize::new(w as u64, 1, 1),
            MTLSize::new(64_u64.min(h as u64), 1, 1),
        ),
    };

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&source_texture));
    encoder.set_texture(1, Some(&output_texture));
    encoder.set_bytes(
        0,
        std::mem::size_of::<PixelSortMetalParams>() as u64,
        (&params as *const PixelSortMetalParams).cast(),
    );
    encoder.dispatch_thread_groups(threadgroups, threads_per_tg);
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, w, h)
}

fn scanline_filter_id(filter: ScanlineFilter) -> u32 {
    match filter {
        ScanlineFilter::None => 0,
        ScanlineFilter::Sub => 1,
        ScanlineFilter::Up => 2,
        ScanlineFilter::Average => 3,
        ScanlineFilter::Paeth => 4,
    }
}

pub fn retro_static_metal(
    source: &ImageBufferF32,
    settings: &RetroStaticSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if source.width == 0 || source.height == 0 {
        return Err(MetalDispatchError::EmptyDimensions);
    }

    let w = source.width;
    let h = source.height;

    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(RETRO_STATIC_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(RETRO_STATIC_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let source_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&source_texture, source)?;

    let params = RetroStaticMetalParams {
        width: w,
        height: h,
        real_bpp: settings.real_bpp,
        assumed_bpp: settings.assumed_bpp,
        filter: scanline_filter_id(settings.filter),
        strength: settings.strength,
    };

    let tg_w = 16_u64.min(w as u64);
    let tg_h = 16_u64.min(h as u64);
    let grid_w = div_ceil(w, tg_w as u32) as u64;
    let grid_h = div_ceil(h, tg_h as u32) as u64;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&source_texture));
    encoder.set_texture(1, Some(&output_texture));
    encoder.set_bytes(
        0,
        std::mem::size_of::<RetroStaticMetalParams>() as u64,
        (&params as *const RetroStaticMetalParams).cast(),
    );
    encoder.dispatch_thread_groups(MTLSize::new(grid_w, grid_h, 1), MTLSize::new(tg_w, tg_h, 1));
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, w, h)
}

pub fn channel_shift_metal(
    source_b: &ImageBufferF32,
    settings: &ChannelShiftSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if source_b.width == 0 || source_b.height == 0 {
        return Err(MetalDispatchError::EmptyDimensions);
    }

    let w = source_b.width;
    let h = source_b.height;

    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(CHANNEL_SHIFT_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(CHANNEL_SHIFT_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let source_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&source_texture, source_b)?;

    let params = ChannelShiftMetalParams {
        width: w,
        height: h,
        shift_r_x: settings.shift_r_x,
        shift_r_y: settings.shift_r_y,
        shift_g_x: settings.shift_g_x,
        shift_g_y: settings.shift_g_y,
        shift_b_x: settings.shift_b_x,
        shift_b_y: settings.shift_b_y,
    };

    let tg_w = 16_u64.min(w as u64);
    let tg_h = 16_u64.min(h as u64);
    let grid_w = div_ceil(w, tg_w as u32) as u64;
    let grid_h = div_ceil(h, tg_h as u32) as u64;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&source_texture));
    encoder.set_texture(1, Some(&output_texture));
    encoder.set_bytes(
        0,
        std::mem::size_of::<ChannelShiftMetalParams>() as u64,
        (&params as *const ChannelShiftMetalParams).cast(),
    );
    encoder.dispatch_thread_groups(MTLSize::new(grid_w, grid_h, 1), MTLSize::new(tg_w, tg_h, 1));
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, w, h)
}

/// Rutt-Etra scanline gather kernel on the GPU — the Metal port of
/// `morphogen_render::render_rutt_etra_frame`. Each output pixel gathers its
/// colour by scanning scanlines in reverse order (bottom→top) and stopping at
/// the first covering scanline, which is equivalent to the CPU's top→bottom
/// last-writer-wins scatter — the result is byte-identical to the CPU path.
pub fn rutt_etra_scanline_metal(
    source_b: &ImageBufferF32,
    settings: &RuttEtraSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if source_b.width == 0 || source_b.height == 0 {
        return Err(MetalDispatchError::EmptyDimensions);
    }

    let plan = RuttEtraDispatchPlan::new(settings, source_b.width, source_b.height)?;
    let w = plan.width;
    let h = plan.height;

    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(RUTT_ETRA_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(RUTT_ETRA_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let source_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&source_texture, source_b)?;

    let params = RuttEtraMetalParams {
        width: w,
        height: h,
        line_pitch: plan.line_pitch,
        displacement_depth: plan.displacement_depth,
        line_thickness: plan.line_thickness,
        mono: u32::from(plan.mono),
    };

    let tg_w = plan.threads_per_threadgroup.width as u64;
    let tg_h = plan.threads_per_threadgroup.height as u64;
    let grid_w = plan.threadgroups_per_grid.width as u64;
    let grid_h = plan.threadgroups_per_grid.height as u64;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&source_texture));
    encoder.set_texture(1, Some(&output_texture));
    encoder.set_bytes(
        0,
        std::mem::size_of::<RuttEtraMetalParams>() as u64,
        (&params as *const RuttEtraMetalParams).cast(),
    );
    encoder.dispatch_thread_groups(MTLSize::new(grid_w, grid_h, 1), MTLSize::new(tg_w, tg_h, 1));
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, w, h)
}

/// Two-source Rutt-Etra: Source A's luma drives the displacement, Source B
/// supplies the drawn colour. A and B must share dimensions (the caller — the
/// CPU-gated render path — guarantees this; enforced here defensively).
pub fn rutt_etra_two_source_metal(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    settings: &RuttEtraSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if source_b.width == 0 || source_b.height == 0 {
        return Err(MetalDispatchError::EmptyDimensions);
    }
    if source_a.width != source_b.width || source_a.height != source_b.height {
        return Err(MetalDispatchError::InvalidRuttEtraSettings(format!(
            "Source A is {}x{}, Source B is {}x{}; two-source requires equal dimensions",
            source_a.width, source_a.height, source_b.width, source_b.height
        )));
    }

    let plan = RuttEtraDispatchPlan::new(settings, source_b.width, source_b.height)?;
    let w = plan.width;
    let h = plan.height;

    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(RUTT_ETRA_TWO_SOURCE_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(RUTT_ETRA_TWO_SOURCE_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let source_a_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let source_b_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&source_a_texture, source_a)?;
    upload_rgba_f32_texture(&source_b_texture, source_b)?;

    let params = RuttEtraMetalParams {
        width: w,
        height: h,
        line_pitch: plan.line_pitch,
        displacement_depth: plan.displacement_depth,
        line_thickness: plan.line_thickness,
        mono: u32::from(plan.mono),
    };

    let tg_w = plan.threads_per_threadgroup.width as u64;
    let tg_h = plan.threads_per_threadgroup.height as u64;
    let grid_w = plan.threadgroups_per_grid.width as u64;
    let grid_h = plan.threadgroups_per_grid.height as u64;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&source_a_texture));
    encoder.set_texture(1, Some(&source_b_texture));
    encoder.set_texture(2, Some(&output_texture));
    encoder.set_bytes(
        0,
        std::mem::size_of::<RuttEtraMetalParams>() as u64,
        (&params as *const RuttEtraMetalParams).cast(),
    );
    encoder.dispatch_thread_groups(MTLSize::new(grid_w, grid_h, 1), MTLSize::new(tg_w, tg_h, 1));
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, w, h)
}

pub fn palette_quantize_metal(
    source_b: &ImageBufferF32,
    settings: &PaletteQuantizeSettings,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if source_b.width == 0 || source_b.height == 0 {
        return Err(MetalDispatchError::EmptyDimensions);
    }

    let (mode, levels) = match settings.mode {
        QuantizeMode::Posterize => (0u32, settings.levels),
        QuantizeMode::Palette => (1u32, 0u32),
        QuantizeMode::Kmeans => {
            return Err(MetalDispatchError::UnsupportedOperation(
                "kmeans palette mode is not yet supported on Metal".to_string(),
            ))
        }
    };

    let w = source_b.width;
    let h = source_b.height;

    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(PALETTE_QUANTIZE_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(PALETTE_QUANTIZE_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let source_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&source_texture, source_b)?;

    let params = PaletteQuantizeMetalParams {
        width: w,
        height: h,
        mode,
        levels,
    };

    let tg_w = 16_u64.min(w as u64);
    let tg_h = 16_u64.min(h as u64);
    let grid_w = div_ceil(w, tg_w as u32) as u64;
    let grid_h = div_ceil(h, tg_h as u32) as u64;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&source_texture));
    encoder.set_texture(1, Some(&output_texture));
    encoder.set_bytes(
        0,
        std::mem::size_of::<PaletteQuantizeMetalParams>() as u64,
        (&params as *const PaletteQuantizeMetalParams).cast(),
    );
    encoder.dispatch_thread_groups(MTLSize::new(grid_w, grid_h, 1), MTLSize::new(tg_w, tg_h, 1));
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, w, h)
}

/// Blend `effected` toward `original` per-pixel using `matte` on the GPU:
/// `out = m*effected + (1-m)*original`, alpha from `effected` — the exact CPU
/// blend in `morphogen_render::apply_matte`, ported as a trivial gather kernel
/// (Tier 5.4 S2). The matte field itself stays CPU-computed
/// (`compute_matte_field`); only the blend runs here.
pub fn matte_blend_metal(
    effected: &ImageBufferF32,
    original: &ImageBufferF32,
    matte: &MatteField,
) -> Result<ImageBufferF32, MetalDispatchError> {
    if effected.width == 0 || effected.height == 0 {
        return Err(MetalDispatchError::EmptyDimensions);
    }
    if effected.width != original.width || effected.height != original.height {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "effected is {}x{}, original is {}x{}",
            effected.width, effected.height, original.width, original.height
        )));
    }
    if matte.width != effected.width || matte.height != effected.height {
        return Err(MetalDispatchError::IncompatibleInputs(format!(
            "matte field is {}x{}, carrier is {}x{}",
            matte.width, matte.height, effected.width, effected.height
        )));
    }

    let w = effected.width;
    let h = effected.height;

    let device = Device::system_default().ok_or(MetalDispatchError::DeviceUnavailable)?;
    let compile_options = CompileOptions::new();
    compile_options.set_fast_math_enabled(false);
    let library = device
        .new_library_with_source(MATTE_BLEND_SHADER_SOURCE, &compile_options)
        .map_err(MetalDispatchError::ShaderCompilation)?;
    let function = library
        .get_function(MATTE_BLEND_KERNEL_NAME, None)
        .map_err(MetalDispatchError::FunctionLookup)?;
    let pipeline = device
        .new_compute_pipeline_state_with_function(&function)
        .map_err(MetalDispatchError::PipelineCreation)?;

    let effected_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let original_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderRead,
    );
    let matte_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::R32Float,
        MTLTextureUsage::ShaderRead,
    );
    let output_texture = new_texture(
        &device,
        w,
        h,
        MTLPixelFormat::RGBA32Float,
        MTLTextureUsage::ShaderWrite,
    );
    upload_rgba_f32_texture(&effected_texture, effected)?;
    upload_rgba_f32_texture(&original_texture, original)?;
    upload_r_f32_texture(&matte_texture, &matte.values, w, h)?;

    let params = MatteBlendMetalParams {
        width: w,
        height: h,
    };

    let tg_w = 16_u64.min(w as u64);
    let tg_h = 16_u64.min(h as u64);
    let grid_w = div_ceil(w, tg_w as u32) as u64;
    let grid_h = div_ceil(h, tg_h as u32) as u64;

    let command_queue = device.new_command_queue();
    let command_buffer = command_queue.new_command_buffer();
    let encoder = command_buffer.new_compute_command_encoder();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_texture(0, Some(&effected_texture));
    encoder.set_texture(1, Some(&original_texture));
    encoder.set_texture(2, Some(&matte_texture));
    encoder.set_texture(3, Some(&output_texture));
    encoder.set_bytes(
        0,
        std::mem::size_of::<MatteBlendMetalParams>() as u64,
        (&params as *const MatteBlendMetalParams).cast(),
    );
    encoder.dispatch_thread_groups(MTLSize::new(grid_w, grid_h, 1), MTLSize::new(tg_w, tg_h, 1));
    encoder.end_encoding();
    command_buffer.commit();
    command_buffer.wait_until_completed();

    let status = command_buffer.status();
    if status != MTLCommandBufferStatus::Completed {
        return Err(MetalDispatchError::CommandBufferFailed(format!(
            "{status:?}"
        )));
    }

    read_rgba_f32_texture(&output_texture, w, h)
}

#[cfg(test)]
mod tests {
    use morphogen_render::{
        advance_field_particles, analyze_convolution_kernel_cpu,
        analyze_convolution_kernels_color_cpu, analyze_grain_pool_cpu, apply_tone_map_cpu,
        coagulation_field, composite_with_field, convolution_blend_color_cpu,
        convolution_blend_cpu, flow_displace_cpu, flow_feedback_frame_cpu, fluid_advect_frame_cpu,
        fluid_advect_two_source_frame_cpu, granular_mosaic_with_pool_selection_cpu,
        granular_mosaic_with_selection_cpu, initialize_field_particles,
        luma_specification_tone_map, render_field_particles, select_grains_from_pool_cpu,
        CoagulationSettings, FieldParticleSettings, FlowFeedbackSettings, FlowField,
        GrainSelection, GranularMosaicSettings, ImageBufferF32, StructureMode,
    };

    use super::*;

    fn coagulation_fixture(seed: u64) -> (ImageBufferF32, ImageBufferF32, CoagulationSettings) {
        // Structured, contrasting A/B so the ownership field has real spatial edges
        // (a flat fixture cannot exercise the bilinear sample or the threshold).
        let a = ImageBufferF32::from_fn(24, 18, |x, y| {
            let v = ((x * 7 + y * 3) % 11) as f32 / 11.0;
            [v, 1.0 - v, 0.5 * v, 1.0]
        })
        .expect("source a");
        let b = ImageBufferF32::from_fn(24, 18, |x, y| {
            let v = ((x * 2 + y * 5) % 13) as f32 / 13.0;
            [0.2 * v, 0.4, 1.0 - v, 1.0]
        })
        .expect("source b");
        let settings = CoagulationSettings {
            patch_size: 5,
            coagulation_strength: 1.4,
            texture_weight: 0.6,
            randomness: 0.5,
            bias: 0.3,
            coherence_passes: 2,
            coherence_strength: 0.5,
            edge_hardness: 0.85,
            edge_dither: 0.4,
            block_jitter: 0.6,
            seed,
            ..CoagulationSettings::default()
        };
        (a, b, settings)
    }

    #[test]
    fn metal_coagulated_composite_matches_cpu_reference() {
        // Exercises every composite lever (block jitter, dithered hard threshold,
        // bilinear field sample). With fast-math disabled the GPU math — including
        // the threshold decision — must match the CPU reference within tolerance.
        let (a, b, settings) = coagulation_fixture(7);
        let field = coagulation_field(&a, &b, settings).expect("field");
        let cpu = composite_with_field(&a, &b, &field, settings).expect("cpu composite");
        let gpu = match coagulated_composite_metal(
            &a,
            &b,
            &field.weights,
            field.cols,
            field.rows,
            field.patch_size,
            settings.edge_hardness,
            settings.edge_dither,
            settings.block_jitter,
            settings.seed,
        ) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal coagulated-composite parity: no Metal device");
                return;
            }
            Err(error) => panic!("metal render failed: {error}"),
        };
        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_coagulated_composite_rejects_mismatched_field_length() {
        let a = ImageBufferF32::new(4, 4, vec![[0.5, 0.5, 0.5, 1.0]; 16]).expect("a");
        let b = ImageBufferF32::new(4, 4, vec![[0.2, 0.2, 0.2, 1.0]; 16]).expect("b");
        let error = coagulated_composite_metal(&a, &b, &[0.0; 3], 2, 2, 2, 0.0, 0.0, 0.0, 0)
            .expect_err("wrong field length must be rejected");
        assert!(
            matches!(error, MetalDispatchError::InvalidCoagulationSettings(_)),
            "expected InvalidCoagulationSettings, got {error:?}"
        );
    }

    #[test]
    fn metal_flow_displacement_matches_cpu_reference_on_tiny_fixture() {
        let carrier = ImageBufferF32::new(
            3,
            2,
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [0.5, 0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.5, 1.0, 0.0, 1.0],
                [1.0, 1.0, 0.0, 1.0],
            ],
        )
        .expect("valid carrier");
        let flow = FlowField::new(
            3,
            2,
            vec![
                [0.5, 0.0],
                [0.25, 0.0],
                [1.0, 0.0],
                [0.0, -0.5],
                [-0.25, -0.25],
                [-1.0, 0.0],
            ],
        )
        .expect("valid flow");

        let cpu = flow_displace_cpu(&carrier, &flow, 1.0).expect("cpu render");
        let gpu = match flow_displace_metal(&carrier, &flow, 1.0) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal parity assertion because no Metal device is available");
                return;
            }
            Err(error) => panic!("metal render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 0.000_01);
    }

    fn lk_textured_frame(width: u32, height: u32, shift_x: f32, shift_y: f32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |x, y| {
            let fx = x as f32 - shift_x;
            let fy = y as f32 - shift_y;
            let value = 0.5
                + 0.2 * (0.31 * fx).sin()
                + 0.2 * (0.37 * fy).sin()
                + 0.1 * (0.23 * (fx + fy)).sin();
            [value, value, value, 1.0]
        })
        .expect("valid frame")
    }

    #[test]
    fn metal_pyramidal_lucas_kanade_matches_cpu_reference() {
        let radius = morphogen_render::LUCAS_KANADE_WINDOW_RADIUS;
        let previous = lk_textured_frame(64, 48, 0.0, 0.0);
        let current = lk_textured_frame(64, 48, 6.0, 4.0);

        let cpu =
            morphogen_render::pyramidal_lucas_kanade_flow_cpu(&previous, &current, 64, 48, radius)
                .expect("cpu flow");
        let gpu = match pyramidal_lucas_kanade_flow_metal(&previous, &current, 64, 48, radius) {
            Ok(estimate) => estimate,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal LK parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal flow failed: {error}"),
        };

        let mut max_flow_diff = 0.0_f32;
        for (g, c) in gpu.flow.vectors.iter().zip(cpu.flow.vectors.iter()) {
            max_flow_diff = max_flow_diff
                .max((g[0] - c[0]).abs())
                .max((g[1] - c[1]).abs());
        }
        let mut max_conf_diff = 0.0_f32;
        for (g, c) in gpu
            .forward_confidence
            .values
            .iter()
            .zip(cpu.forward_confidence.values.iter())
        {
            max_conf_diff = max_conf_diff.max((g - c).abs());
        }

        // GPU float rounding differs from the CPU reference, so this is a within-tolerance
        // check (not byte parity). Flow is in pixel-displacement units; the project pixel
        // epsilon is 1/255 ~= 0.0039, and the displacement agreement is far tighter than a
        // pixel. The actual measured max difference prints below for ratcheting.
        eprintln!("LK metal parity: max_flow_diff={max_flow_diff}, max_conf_diff={max_conf_diff}");
        assert!(
            max_flow_diff < 1.0 / 255.0,
            "flow diverged by {max_flow_diff} (> 1/255)"
        );
        assert!(
            max_conf_diff < 1.0 / 255.0,
            "confidence diverged by {max_conf_diff} (> 1/255)"
        );
    }

    #[test]
    fn metal_flow_feedback_matches_cpu_reference_on_tiny_fixture() {
        let carrier = ImageBufferF32::new(
            3,
            1,
            vec![
                [0.1, 0.0, 0.0, 1.0],
                [0.5, 0.0, 0.0, 1.0],
                [0.9, 0.0, 0.0, 1.0],
            ],
        )
        .expect("carrier");
        let previous = ImageBufferF32::new(
            3,
            1,
            vec![
                [0.2, 0.0, 0.0, 1.0],
                [0.4, 0.0, 0.0, 1.0],
                [0.8, 0.0, 0.0, 1.0],
            ],
        )
        .expect("previous");
        let flow = FlowField::new(3, 1, vec![[0.5, 0.0]; 3]).expect("flow");
        let settings = FlowFeedbackSettings {
            carrier_amount: 0.5,
            feedback_amount: 0.75,
            feedback_mix: 0.6,
            decay: 0.9,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
        };

        let cpu = flow_feedback_frame_cpu(&carrier, Some(&previous), &flow, settings)
            .expect("cpu render");
        let gpu = match flow_feedback_metal(&carrier, Some(&previous), &flow, settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal feedback parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal feedback render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 0.000_01);
    }

    #[test]
    fn metal_flow_feedback_structure_mix_matches_cpu_reference() {
        // A 2D textured fixture so the structure-preserving blur exercises both
        // axes, with fractional flow so the base term goes through the hardware
        // linear sampler. Parity is asserted at the project's 1/255 tolerance.
        let carrier = ImageBufferF32::from_fn(4, 4, |x, y| {
            let value = if (x + y) % 2 == 0 { 0.85 } else { 0.15 };
            [value, value * 0.5, 1.0 - value, 1.0]
        })
        .expect("carrier");
        let previous = ImageBufferF32::new(4, 4, vec![[0.4, 0.4, 0.4, 1.0]; 16]).expect("previous");
        let flow = FlowField::new(4, 4, vec![[0.35, -0.2]; 16]).expect("flow");
        let settings = FlowFeedbackSettings {
            carrier_amount: 1.0,
            feedback_amount: 1.5,
            feedback_mix: 0.8,
            decay: 0.95,
            iterations: 1,
            structure_mix: 0.7,
            structure_mode: StructureMode::SingleScale,
        };

        let cpu = flow_feedback_frame_cpu(&carrier, Some(&previous), &flow, settings)
            .expect("cpu render");
        let gpu = match flow_feedback_metal(&carrier, Some(&previous), &flow, settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal feedback structure parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal feedback render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_video_vocoder_match_matches_cpu_reference() {
        // A textured, full-range carrier and a brighter modulator so the tone map
        // is non-trivial; parity asserted at the project's 1/255 tolerance.
        let carrier = ImageBufferF32::from_fn(6, 5, |x, y| {
            let base = (x as f32) / 5.0;
            let tint = if (x + y) % 2 == 0 { 0.1 } else { -0.1 };
            let v = (base + tint).clamp(0.0, 1.0);
            [v, v * 0.7, 1.0 - v, 1.0]
        })
        .expect("carrier");
        let modulator = ImageBufferF32::from_fn(8, 8, |x, _| {
            let v = 0.55 + 0.45 * (x as f32 / 7.0); // bright, skewed high
            [v, v, v, 1.0]
        })
        .expect("modulator");

        let tone = luma_specification_tone_map(&modulator, &carrier);
        let cpu = apply_tone_map_cpu(&carrier, &tone, 1.0).expect("cpu render");
        let gpu = match video_vocoder_match_metal(&carrier, &tone, 1.0) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal video vocoder parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal video vocoder render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_convolution_blend_matches_cpu_reference() {
        // A high-frequency carrier (so a blur actually changes pixels) and a
        // structured modulator (so the kernel is non-uniform); parity at 1/255.
        let carrier = ImageBufferF32::from_fn(7, 6, |x, y| {
            let v = if (x + y) % 2 == 0 { 0.85 } else { 0.15 };
            [v, v * 0.6, 1.0 - v, 1.0]
        })
        .expect("carrier");
        let modulator = ImageBufferF32::from_fn(9, 9, |x, y| {
            let v = ((x + 2 * y) as f32 / 24.0).clamp(0.0, 1.0);
            [v, v, v, 1.0]
        })
        .expect("modulator");

        let kernel = analyze_convolution_kernel_cpu(&modulator, 3).expect("kernel");
        let cpu = convolution_blend_cpu(&carrier, &kernel, 1.0).expect("cpu render");
        let gpu = match convolution_blend_metal(&carrier, &kernel.weights, kernel.size, 1.0) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal convolution blend parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal convolution blend render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_convolution_blend_matches_cpu_reference_large_kernel() {
        // The Metal kernel has no K cap: it loops over `kernel_size` with a
        // dynamically-sized weights buffer, so a large K stays byte-parity with
        // the CPU reference exactly like a small one. Guards the "large-K spatial
        // kernel" claim against a hidden cap or buffer-size regression.
        let carrier = ImageBufferF32::from_fn(20, 18, |x, y| {
            let v = if (x + y) % 2 == 0 { 0.85 } else { 0.15 };
            [v, v * 0.6, 1.0 - v, 1.0]
        })
        .expect("carrier");
        let modulator = ImageBufferF32::from_fn(33, 33, |x, y| {
            let v = ((x + 2 * y) as f32 / 96.0).clamp(0.0, 1.0);
            [v, v, v, 1.0]
        })
        .expect("modulator");

        let kernel = analyze_convolution_kernel_cpu(&modulator, 11).expect("kernel");
        let cpu = convolution_blend_cpu(&carrier, &kernel, 1.0).expect("cpu render");
        let gpu = match convolution_blend_metal(&carrier, &kernel.weights, kernel.size, 1.0) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal large-kernel parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal convolution blend render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_convolution_blend_color_matches_cpu_reference() {
        // A carrier whose channels differ and a modulator whose R/G/B channels
        // carry different structure, so the three per-channel kernels are all
        // distinct; parity at 1/255 against the CPU colour reference.
        let carrier = ImageBufferF32::from_fn(8, 7, |x, y| {
            let v = if (x + y) % 2 == 0 { 0.85 } else { 0.15 };
            [v, v * 0.6, 1.0 - v, 1.0]
        })
        .expect("carrier");
        let modulator = ImageBufferF32::from_fn(9, 9, |x, y| {
            [
                (x as f32 / 8.0).clamp(0.0, 1.0),
                (y as f32 / 8.0).clamp(0.0, 1.0),
                ((x + y) as f32 / 16.0).clamp(0.0, 1.0),
                1.0,
            ]
        })
        .expect("modulator");

        let kernels = analyze_convolution_kernels_color_cpu(&modulator, 3).expect("kernels");
        let cpu = convolution_blend_color_cpu(&carrier, &kernels, 1.0).expect("cpu render");
        let gpu = match convolution_blend_color_metal(
            &carrier,
            &kernels[0].weights,
            &kernels[1].weights,
            &kernels[2].weights,
            kernels[0].size,
            1.0,
        ) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal colour convolution parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal colour convolution blend render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_convolution_blend_color_rejects_even_kernel_size() {
        let carrier = ImageBufferF32::new(2, 2, vec![[0.5, 0.5, 0.5, 1.0]; 4]).expect("carrier");
        let error =
            convolution_blend_color_metal(&carrier, &[0.25; 4], &[0.25; 4], &[0.25; 4], 2, 1.0)
                .expect_err("even kernel size must be rejected");
        assert!(
            matches!(error, MetalDispatchError::InvalidConvolutionSettings(_)),
            "expected InvalidConvolutionSettings, got {error:?}"
        );
    }

    #[test]
    fn metal_convolution_blend_rejects_even_kernel_size() {
        let carrier = ImageBufferF32::new(2, 2, vec![[0.5, 0.5, 0.5, 1.0]; 4]).expect("carrier");
        let error = convolution_blend_metal(&carrier, &[0.25; 4], 2, 1.0)
            .expect_err("even kernel size must be rejected");
        assert!(
            matches!(error, MetalDispatchError::InvalidConvolutionSettings(_)),
            "expected InvalidConvolutionSettings, got {error:?}"
        );
    }

    #[test]
    fn metal_feedback_rejects_multiscale_structure_mode() {
        // The Metal shader only implements single-scale structure re-injection,
        // so multiscale must be rejected before dispatch rather than silently
        // rendering the wrong (single-scale) result. This guard needs no GPU.
        let carrier = ImageBufferF32::new(2, 2, vec![[0.5, 0.5, 0.5, 1.0]; 4]).expect("carrier");
        let previous = ImageBufferF32::new(2, 2, vec![[0.2, 0.2, 0.2, 1.0]; 4]).expect("previous");
        let flow = FlowField::new(2, 2, vec![[0.1, 0.0]; 4]).expect("flow");
        let settings = FlowFeedbackSettings {
            carrier_amount: 1.0,
            feedback_amount: 1.0,
            feedback_mix: 0.7,
            decay: 0.9,
            iterations: 1,
            structure_mix: 0.6,
            structure_mode: StructureMode::Multiscale,
        };

        let error = flow_feedback_metal(&carrier, Some(&previous), &flow, settings)
            .expect_err("multiscale must be rejected on the Metal backend");
        assert!(
            matches!(error, MetalDispatchError::InvalidFeedbackSettings(_)),
            "expected InvalidFeedbackSettings, got {error:?}"
        );
    }

    #[test]
    fn metal_granular_mosaic_matches_cpu_reference_on_tiny_fixture() {
        let carrier = ImageBufferF32::new(
            4,
            2,
            vec![
                [0.1, 0.0, 0.0, 1.0],
                [0.3, 0.0, 0.0, 1.0],
                [0.6, 0.0, 0.0, 1.0],
                [0.9, 0.0, 0.0, 1.0],
                [0.0, 0.1, 0.0, 1.0],
                [0.0, 0.3, 0.0, 1.0],
                [0.0, 0.6, 0.0, 1.0],
                [0.0, 0.9, 0.0, 1.0],
            ],
        )
        .expect("carrier");
        let selection = GrainSelection {
            columns: 2,
            rows: 1,
            indices: vec![1, 0],
        };
        let settings = GranularMosaicSettings {
            grain_size: 2,
            rearrangement: 0.65,
            variation: 0.0,
            seed: 0,
        };

        let cpu =
            granular_mosaic_with_selection_cpu(&carrier, &selection, settings).expect("cpu render");
        let gpu = match granular_mosaic_metal(&carrier, &selection, settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal parity assertion because no Metal device is available");
                return;
            }
            Err(error) => panic!("metal render failed: {error}"),
        };

        // A fractional rearrangement weight makes the carrier sample land between
        // texels, where Metal's hardware linear sampler quantizes the
        // interpolation weight to 8-bit fixed point. Hold parity to the project's
        // Metal/CPU tolerance (1/255), the same bound the CLI render path gates on.
        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_granular_mosaic_supports_selection_tables_over_four_kib() {
        // `set_bytes` is limited to data smaller than 4 KiB. A 33x33 grain
        // grid produces 1,089 u32 selection entries (4,356 bytes), so this
        // validates the dedicated MTLBuffer binding used by real HD renders.
        const SIDE: u32 = 33;
        let carrier = ImageBufferF32::from_fn(SIDE, SIDE, |x, y| {
            let value = (x + y * SIDE) as f32 / (SIDE * SIDE) as f32;
            [value, 1.0 - value, value * 0.5, 1.0]
        })
        .expect("carrier");
        let selection = GrainSelection {
            columns: SIDE,
            rows: SIDE,
            indices: (0..SIDE * SIDE).collect(),
        };
        assert!(selection.indices.len() * std::mem::size_of::<u32>() > 4 * 1024);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 0.65,
            variation: 0.0,
            seed: 0,
        };

        let cpu =
            granular_mosaic_with_selection_cpu(&carrier, &selection, settings).expect("cpu render");
        let gpu = match granular_mosaic_metal(&carrier, &selection, settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal parity assertion because no Metal device is available");
                return;
            }
            Err(error) => panic!("metal render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_granular_mosaic_pool_matches_cpu_reference_on_multi_frame_fixture() {
        // Two distinct pool frames so the cross-frame selection actually pulls
        // grains from different source frames, exercising the texture array.
        let frame_a = ImageBufferF32::from_fn(4, 4, |x, y| {
            let v = (x + y * 4) as f32 / 16.0;
            [v, 0.0, 0.0, 1.0]
        })
        .expect("frame a");
        let frame_b = ImageBufferF32::from_fn(4, 4, |x, y| {
            let v = (x + y * 4) as f32 / 16.0;
            [0.0, v, 1.0 - v, 1.0]
        })
        .expect("frame b");
        let pool_frames = vec![frame_a.clone(), frame_b.clone()];
        // Frame A is bright top-left, frame B bottom-right: distinct audio
        // descriptors give the joint-AV matcher something to separate on.
        let frame_audio = vec![vec![0.1_f32], vec![0.9_f32]];
        let grain_size = 2;
        let pool = analyze_grain_pool_cpu(&pool_frames, &frame_audio, grain_size).expect("pool");

        let modulator = ImageBufferF32::from_fn(4, 4, |x, y| {
            let v = (x + y) as f32 / 6.0;
            [v, v, v, 1.0]
        })
        .expect("modulator");
        let settings = GranularMosaicSettings {
            grain_size,
            rearrangement: 0.6,
            variation: 0.0,
            seed: 7,
        };
        let carrier = &pool_frames[0];
        let query_audio = vec![0.3_f32];
        let audio_weight = 1.0;
        let selection = select_grains_from_pool_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &query_audio,
            &pool,
            settings,
            audio_weight,
            0.0,
            morphogen_render::PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("selection");

        let cpu = granular_mosaic_with_pool_selection_cpu(
            &pool_frames,
            &pool,
            carrier,
            &selection,
            settings,
        )
        .expect("cpu render");
        let gpu =
            match granular_mosaic_pool_metal(&pool_frames, &pool, carrier, &selection, settings) {
                Ok(image) => image,
                Err(MetalDispatchError::DeviceUnavailable) => {
                    eprintln!(
                        "skipping Metal pool parity assertion because no Metal device is available"
                    );
                    return;
                }
                Err(error) => panic!("metal pool render failed: {error}"),
            };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_fluid_advect_matches_cpu_reference() {
        // A 2D-textured source and a distinct previous dye buffer so the curl-noise
        // velocity actually relocates content and the source reinjection blends a real
        // difference. A non-zero frame index drives the drifting detail octave. The
        // velocity field reproduces the CPU splitmix64 lattice noise (integer hashing is
        // exact; the float math runs with fast-math disabled), so parity holds far below
        // the project's 1/255 bound — to ~2e-6, the same order as the other manual-bilinear
        // kernels. Asserted at 1e-5 (the flow_feedback precedent), well under 1/255.
        let source = ImageBufferF32::from_fn(24, 20, |x, y| {
            let v = if (x + y) % 2 == 0 { 0.8 } else { 0.2 };
            [v, v * 0.6, 1.0 - v, 1.0]
        })
        .expect("source");
        let previous = ImageBufferF32::from_fn(24, 20, |x, y| {
            [
                (x as f32 * 0.04).fract(),
                (y as f32 * 0.05).fract(),
                0.3,
                1.0,
            ]
        })
        .expect("previous");
        let settings = FluidAdvectSettings {
            advect: 9.0,
            reinject: 0.2,
            ..FluidAdvectSettings::default()
        };

        let cpu =
            fluid_advect_frame_cpu(&source, Some(&previous), 4, settings).expect("cpu render");
        let gpu = match fluid_advect_metal(&source, Some(&previous), 4, settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal fluid advect parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal fluid advect render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 0.000_01);
    }

    #[test]
    fn metal_fluid_advect_new_knobs_match_cpu_reference() {
        // Every v3 knob engaged at once: explicit substep count (ping-pong passes),
        // domain warp (sin in the streamfunction), blotch-gated reinjection (pow 5.5 in
        // the mask) and diffusion (the 9-tap blur). sin/pow accumulate a few more ULPs
        // than the pure lattice math and the passes compound, so this asserts at 1e-4 —
        // still 40x under the project's 1/255 gate.
        let source = ImageBufferF32::from_fn(24, 20, |x, y| {
            let v = if (x + y) % 2 == 0 { 0.8 } else { 0.2 };
            [v, v * 0.6, 1.0 - v, 1.0]
        })
        .expect("source");
        let previous = ImageBufferF32::from_fn(24, 20, |x, y| {
            [
                (x as f32 * 0.04).fract(),
                (y as f32 * 0.05).fract(),
                0.3,
                1.0,
            ]
        })
        .expect("previous");
        let settings = FluidAdvectSettings {
            advect: 9.0,
            reinject: 0.3,
            substeps: 5,
            reinject_blotch: 0.8,
            warp: 1.5,
            diffuse: 0.25,
            detail: 0.4,
            turbulence_scale: 0.03,
            ..FluidAdvectSettings::default()
        };

        let cpu =
            fluid_advect_frame_cpu(&source, Some(&previous), 6, settings).expect("cpu render");
        let gpu = match fluid_advect_metal(&source, Some(&previous), 6, settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal fluid advect new-knob parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal fluid advect render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 0.000_1);
    }

    #[test]
    fn metal_fluid_advect_frame_zero_is_source() {
        let source =
            ImageBufferF32::from_fn(8, 8, |x, _| [x as f32 / 7.0, 0.1, 0.2, 1.0]).expect("source");
        let gpu = fluid_advect_metal(&source, None, 0, FluidAdvectSettings::default())
            .expect("frame zero");
        assert_eq!(gpu.pixels, source.pixels);
    }

    #[test]
    fn metal_fluid_advect_two_source_matches_cpu_reference() {
        // A textured carrier B, a distinct previous dye, and a non-uniform flow field so the
        // displace lands on fractional positions (the manual bilinear) and the reinject blends
        // a real difference. Parity at the project's 1/255 bound (the manual-bilinear class).
        let carrier_b = ImageBufferF32::from_fn(20, 16, |x, y| {
            let v = if (x + y) % 2 == 0 { 0.75 } else { 0.25 };
            [v, 1.0 - v, v * 0.5, 1.0]
        })
        .expect("carrier");
        let previous = ImageBufferF32::from_fn(20, 16, |x, y| {
            [
                (x as f32 * 0.04).fract(),
                (y as f32 * 0.05).fract(),
                0.3,
                1.0,
            ]
        })
        .expect("previous");
        let flow = FlowField::from_fn(20, 16, |x, y| {
            [(x as f32).sin() * 1.5, (y as f32).cos() * 1.2]
        })
        .expect("flow");
        let settings = FluidAdvectTwoSourceSettings {
            advect: 1.5,
            reinject: 0.2,
        };

        let cpu = fluid_advect_two_source_frame_cpu(&carrier_b, Some(&previous), &flow, settings)
            .expect("cpu render");
        let gpu = match fluid_advect_two_source_metal(&carrier_b, &previous, &flow, settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal two-source fluid advect parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal two-source fluid advect render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_field_particles_splat_matches_cpu_reference() {
        // Build a particle field from a textured source and advance several frames so the
        // particles sit at non-trivial, overlapping float positions, then compare the GPU
        // gather splat with the CPU scatter. Positions are the CPU floats uploaded verbatim, so
        // the rasterization is byte-identical (asserted at a tight bound).
        let source = ImageBufferF32::from_fn(40, 32, |x, y| {
            let u = x as f32 / 39.0;
            let v = y as f32 / 31.0;
            [u, v, 1.0 - u, 1.0]
        })
        .expect("source");
        let settings = FieldParticleSettings {
            spacing: 5,
            particle_size: 5,
            advect: 2.5,
            turbulence_scale: 0.03,
            ..FieldParticleSettings::default()
        };
        let mut field = initialize_field_particles(&source, settings).expect("init");
        for index in 1..=5 {
            advance_field_particles(&mut field, index, settings).expect("advance");
        }

        let cpu = render_field_particles(&field, settings).expect("cpu render");
        let gpu = match field_particles_splat_metal(&field, settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!(
                    "skipping Metal field particles splat parity assertion because no Metal device is available"
                );
                return;
            }
            Err(error) => panic!("metal field particles splat render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 0.000_001);
    }

    #[test]
    fn metal_pixel_sort_matches_cpu_reference_rgb_360wide() {
        use morphogen_render::{render_pixel_sort_frame, PixelSortSettings, SortAxis};

        // Full-color fixture matching the cello2 footage size (360×640) with varied channels.
        // Uses the same 8-bit/255 value range as PNG-decoded footage.
        let fixture = ImageBufferF32::from_fn(360, 640, |x, y| {
            let r = ((x * 7 + y * 3 + 13) % 256) as f32 / 255.0;
            let g = ((x * 11 + y * 17 + 7) % 256) as f32 / 255.0;
            let b = ((x * 5 + y * 23 + 29) % 256) as f32 / 255.0;
            [r, g, b, 1.0]
        })
        .expect("fixture");

        let settings = PixelSortSettings {
            axis: SortAxis::Row,
            threshold_low: 0.20,
            threshold_high: 0.85,
            ..Default::default()
        };

        let cpu = render_pixel_sort_frame(&fixture, &settings, &[]).expect("cpu render");
        let gpu = match pixel_sort_metal(&fixture, &settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal pixel sort parity (rgb 360-wide): no Metal device");
                return;
            }
            Err(error) => panic!("metal pixel sort (rgb 360-wide) failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 0.0);
    }

    #[test]
    fn metal_pixel_sort_real_cello_frame() {
        use morphogen_render::{render_pixel_sort_frame, PixelSortSettings, SortAxis};

        // Greyscale fixture matching cello2 footage: r=g=b, values from v/255 (same as
        // image 0.25.x to_rgba32f() on an 8-bit PNG — no gamma correction applied).
        let w = 360u32;
        let h = 640u32;
        let fixture = ImageBufferF32::from_fn(w, h, |x, y| {
            let v_u8 = ((x * 83 + y * 17 + 41) % 256) as u8;
            let v = v_u8 as f32 / 255.0;
            [v, v, v, 1.0]
        })
        .expect("fixture");

        let settings = PixelSortSettings {
            axis: SortAxis::Row,
            threshold_low: 0.20,
            threshold_high: 0.85,
            ..Default::default()
        };

        let cpu = render_pixel_sort_frame(&fixture, &settings, &[]).expect("cpu");
        let gpu = match pixel_sort_metal(&fixture, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping: no Metal device");
                return;
            }
            Err(e) => panic!("gpu failed: {e}"),
        };

        assert_image_near(&gpu, &cpu, 0.0);
    }

    #[test]
    fn metal_pixel_sort_matches_cpu_reference() {
        use morphogen_render::{render_pixel_sort_frame, PixelSortSettings, SortAxis};

        // Textured fixture: unique lumas per pixel so no ties, all distinct keys.
        // Row width=32 and height=16 → well within PS_MAX_LINE=1024.
        let fixture = ImageBufferF32::from_fn(32, 16, |x, y| {
            // Interleave x and y to ensure variation along both axes.
            let v = ((x * 7 + y * 13) % 31 + 1) as f32 / 32.0;
            [v, v, v, 1.0]
        })
        .expect("fixture");

        let settings = PixelSortSettings {
            axis: SortAxis::Row,
            threshold_low: 0.2,
            threshold_high: 0.8,
            ..Default::default()
        };

        let cpu = render_pixel_sort_frame(&fixture, &settings, &[]).expect("cpu render");
        let gpu = match pixel_sort_metal(&fixture, &settings) {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal pixel sort parity assertion: no Metal device");
                return;
            }
            Err(error) => panic!("metal pixel sort failed: {error}"),
        };

        // Pixel sort is a pure permutation: every output pixel is an unmodified input
        // pixel. CPU and GPU use the same IEEE 754 f32 sort key. Parity must be exact.
        assert_image_near(&gpu, &cpu, 0.0);
    }

    #[test]
    fn metal_pixel_sort_col_axis_matches_cpu() {
        use morphogen_render::{render_pixel_sort_frame, PixelSortSettings, SortAxis};

        // Col axis: one threadgroup per column, sorts vertically. Use a wider-than-tall
        // fixture so many col threadgroups are dispatched (the row tests focus on rows).
        let fixture = ImageBufferF32::from_fn(64, 32, |x, y| {
            let r = ((x * 11 + y * 7 + 5) % 256) as f32 / 255.0;
            let g = ((x * 17 + y * 3 + 11) % 256) as f32 / 255.0;
            let b = ((x * 5 + y * 13 + 23) % 256) as f32 / 255.0;
            [r, g, b, 1.0]
        })
        .expect("fixture");

        let settings = PixelSortSettings {
            axis: SortAxis::Col,
            threshold_low: 0.15,
            threshold_high: 0.85,
            ..Default::default()
        };
        let cpu = render_pixel_sort_frame(&fixture, &settings, &[]).expect("cpu");
        let gpu = match pixel_sort_metal(&fixture, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal pixel sort col-axis parity: no Metal device");
                return;
            }
            Err(e) => panic!("metal pixel sort col-axis failed: {e}"),
        };
        assert_image_near(&gpu, &cpu, 0.0);
    }

    #[test]
    fn metal_pixel_sort_descending_matches_cpu() {
        use morphogen_render::{
            render_pixel_sort_frame, PixelSortSettings, SortAxis, SortDirection,
        };

        let fixture = ImageBufferF32::from_fn(32, 16, |x, y| {
            let v = ((x * 7 + y * 13 + 3) % 31 + 1) as f32 / 32.0;
            [v, v, v, 1.0]
        })
        .expect("fixture");

        let settings = PixelSortSettings {
            axis: SortAxis::Row,
            direction: SortDirection::Desc,
            threshold_low: 0.1,
            threshold_high: 0.9,
            ..Default::default()
        };
        let cpu = render_pixel_sort_frame(&fixture, &settings, &[]).expect("cpu");
        let gpu = match pixel_sort_metal(&fixture, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal pixel sort descending parity: no Metal device");
                return;
            }
            Err(e) => panic!("metal pixel sort descending failed: {e}"),
        };
        assert_image_near(&gpu, &cpu, 0.0);
    }

    #[test]
    fn metal_pixel_sort_max_span_matches_cpu() {
        use morphogen_render::{render_pixel_sort_frame, PixelSortSettings, SortAxis};

        let fixture = ImageBufferF32::from_fn(48, 16, |x, y| {
            let v = ((x * 11 + y * 7 + 17) % 256) as f32 / 255.0;
            [v, v, v, 1.0]
        })
        .expect("fixture");

        let settings = PixelSortSettings {
            axis: SortAxis::Row,
            threshold_low: 0.0,
            threshold_high: 1.0,
            max_span: 8, // chunk every 8 px
            ..Default::default()
        };
        let cpu = render_pixel_sort_frame(&fixture, &settings, &[]).expect("cpu");
        let gpu = match pixel_sort_metal(&fixture, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal pixel sort max_span parity: no Metal device");
                return;
            }
            Err(e) => panic!("metal pixel sort max_span failed: {e}"),
        };
        assert_image_near(&gpu, &cpu, 0.0);
    }

    #[test]
    fn metal_pixel_sort_hue_key_matches_cpu() {
        use morphogen_render::{render_pixel_sort_frame, PixelSortSettings, SortAxis, SortKey};

        // Full-color fixture with varied hue so the hue sort key produces a real permutation.
        let fixture = ImageBufferF32::from_fn(32, 16, |x, y| {
            let r = ((x * 13 + y * 5 + 7) % 256) as f32 / 255.0;
            let g = ((x * 7 + y * 17 + 13) % 256) as f32 / 255.0;
            let b = ((x * 3 + y * 11 + 31) % 256) as f32 / 255.0;
            [r, g, b, 1.0]
        })
        .expect("fixture");

        let settings = PixelSortSettings {
            axis: SortAxis::Row,
            key: SortKey::Hue,
            threshold_low: 0.0,
            threshold_high: 1.0,
            ..Default::default()
        };
        let cpu = render_pixel_sort_frame(&fixture, &settings, &[]).expect("cpu");
        let gpu = match pixel_sort_metal(&fixture, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal pixel sort hue key parity: no Metal device");
                return;
            }
            Err(e) => panic!("metal pixel sort hue key failed: {e}"),
        };
        assert_image_near(&gpu, &cpu, 0.0);
    }

    #[test]
    fn metal_channel_shift_passthrough_matches_cpu() {
        use morphogen_render::{render_channel_shift_frame, ChannelShiftSettings};

        let fixture = ImageBufferF32::from_fn(32, 24, |x, y| {
            let r = ((x * 7 + y * 3) % 17) as f32 / 17.0;
            let g = ((x * 3 + y * 11) % 19) as f32 / 19.0;
            let b = ((x * 5 + y * 7) % 23) as f32 / 23.0;
            [r, g, b, 1.0]
        })
        .expect("fixture");

        let settings = ChannelShiftSettings::default();

        let cpu = render_channel_shift_frame(&fixture, &settings, &[]).expect("cpu");
        let gpu = match channel_shift_metal(&fixture, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal channel shift passthrough: no Metal device");
                return;
            }
            Err(e) => panic!("channel shift metal (passthrough) failed: {e}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_channel_shift_offset_matches_cpu() {
        use morphogen_render::{render_channel_shift_frame, ChannelShiftSettings};

        // RGB fixture: distinct per-channel ramps so offset is detectable per-channel.
        let fixture = ImageBufferF32::from_fn(64, 48, |x, y| {
            let r = x as f32 / 63.0;
            let g = y as f32 / 47.0;
            let b = ((x + y) % 32) as f32 / 32.0;
            [r, g, b, 1.0]
        })
        .expect("fixture");

        let settings = ChannelShiftSettings {
            shift_r_x: 6.0,
            shift_b_x: -6.0,
            ..Default::default()
        };

        let cpu = render_channel_shift_frame(&fixture, &settings, &[]).expect("cpu");
        let gpu = match channel_shift_metal(&fixture, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal channel shift offset: no Metal device");
                return;
            }
            Err(e) => panic!("channel shift metal (offset) failed: {e}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_palette_quantize_posterize_matches_cpu() {
        use morphogen_render::{
            render_palette_quantize_frame, PaletteQuantizeSettings, QuantizeMode,
        };

        let fixture = ImageBufferF32::from_fn(32, 24, |x, y| {
            let r = ((x * 7 + y * 3) % 17) as f32 / 17.0;
            let g = ((x * 3 + y * 11) % 19) as f32 / 19.0;
            let b = ((x * 5 + y * 7) % 23) as f32 / 23.0;
            [r, g, b, 1.0]
        })
        .expect("fixture");

        let settings = PaletteQuantizeSettings {
            mode: QuantizeMode::Posterize,
            levels: 8,
        };
        let cpu = render_palette_quantize_frame(&fixture, &settings).expect("cpu");
        let gpu = match palette_quantize_metal(&fixture, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal palette quantize posterize: no Metal device");
                return;
            }
            Err(e) => panic!("palette_quantize_metal (posterize) failed: {e}"),
        };
        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_palette_quantize_palette_matches_cpu() {
        use morphogen_render::{
            render_palette_quantize_frame, PaletteQuantizeSettings, QuantizeMode,
        };

        // Use pixels clearly within one Voronoi region to avoid FP tie-break risk.
        // Each quadrant is assigned to a different palette entry.
        let fixture = ImageBufferF32::from_fn(32, 32, |x, y| {
            let (r, g, b) = match (x < 16, y < 16) {
                (true, true) => (0.95f32, 0.05, 0.95),  // near magenta (1,0,1)
                (false, true) => (0.90f32, 0.45, 0.05), // near orange  (1,0.5,0)
                (true, false) => (0.05f32, 0.72, 0.72), // near teal    (0,0.75,0.75)
                (false, false) => (0.05f32, 0.05, 0.05), // near black   (0,0,0)
            };
            [r, g, b, 1.0]
        })
        .expect("fixture");

        let settings = PaletteQuantizeSettings {
            mode: QuantizeMode::Palette,
            levels: 256,
        };
        let cpu = render_palette_quantize_frame(&fixture, &settings).expect("cpu");
        let gpu = match palette_quantize_metal(&fixture, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal palette quantize palette: no Metal device");
                return;
            }
            Err(e) => panic!("palette_quantize_metal (palette) failed: {e}"),
        };
        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
    }

    #[test]
    fn metal_matte_blend_matches_cpu_reference() {
        use morphogen_render::{apply_matte, compute_matte_field, MatteSource};

        // Textured, distinct effected/original frames so the blend is
        // non-trivial in every channel; matte-media frame is half-black/
        // half-white so the a-luma field varies spatially (the half-frame
        // readout shape, on a small fixture).
        let effected = ImageBufferF32::from_fn(16, 16, |x, y| {
            let v = 0.5 + 0.3 * (0.4 * x as f32).sin() + 0.2 * (0.3 * y as f32).cos();
            [v, v * 0.5, 1.0 - v, 1.0]
        })
        .expect("effected fixture");
        let original = ImageBufferF32::from_fn(16, 16, |x, y| {
            let v = 0.2 + 0.1 * x as f32 / 16.0 + 0.1 * y as f32 / 16.0;
            [1.0 - v, v, v * 0.7, 1.0]
        })
        .expect("original fixture");
        let matte_media = ImageBufferF32::from_fn(16, 16, |x, _y| {
            if x < 8 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        })
        .expect("matte media fixture");

        let field =
            compute_matte_field(None, &matte_media, MatteSource::ALuma, 1.0).expect("matte field");
        let cpu = apply_matte(&effected, &original, &field).expect("cpu blend");
        let gpu = match matte_blend_metal(&effected, &original, &field) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal matte blend: no Metal device");
                return;
            }
            Err(e) => panic!("matte_blend_metal failed: {e}"),
        };
        assert_image_near(&gpu, &cpu, 1.0 / 255.0);

        // Sanity: the two halves must actually differ (else the test would
        // pass trivially even with a broken blend).
        let left_gpu = gpu.pixels[8]; // (x=8,y=0) is inside the white half
        let right_gpu = gpu.pixels[0]; // (x=0,y=0) is inside the black half
        assert_ne!(
            left_gpu, right_gpu,
            "matte-driven halves must diverge in the blended output"
        );
    }

    #[test]
    fn metal_rutt_etra_scanline_matches_cpu_reference() {
        // Small synthetic gradient: luma ramps left→right so displacement varies
        // across columns. Pitch 4, depth 6.0, thickness 1, mono false — enough
        // to produce both displaced and black pixels, exercising the gather loop.
        use morphogen_render::{render_rutt_etra_frame, RuttEtraSettings};

        let source = ImageBufferF32::from_fn(32, 16, |x, _y| {
            let v = x as f32 / 31.0;
            [v, v, v, 1.0]
        })
        .expect("fixture");

        let settings = RuttEtraSettings {
            line_pitch: 4,
            displacement_depth: 6.0,
            line_thickness: 1,
            mono: false,
        };

        let cpu = render_rutt_etra_frame(&source, &settings).expect("cpu render");
        let gpu = match rutt_etra_scanline_metal(&source, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal rutt-etra parity: no Metal device");
                return;
            }
            Err(e) => panic!("rutt_etra_scanline_metal failed: {e}"),
        };

        // The gather kernel must be byte-identical to the CPU scatter.
        assert_eq!(
            gpu.pixels, cpu.pixels,
            "Metal rutt-etra output must be byte-identical to the CPU reference"
        );
    }

    #[test]
    fn metal_rutt_etra_two_source_matches_cpu_reference() {
        // A drives displacement (vertical luma ramp → per-row shift), B supplies
        // colour (horizontal RGB gradient). Distinct A and B exercise the split
        // luma/colour reads; the gather proof is unchanged from single-source.
        use morphogen_render::{render_rutt_etra_two_source_frame, RuttEtraSettings};

        let source_a = ImageBufferF32::from_fn(32, 16, |_x, y| {
            let v = y as f32 / 15.0;
            [v, v, v, 1.0]
        })
        .expect("A fixture");
        let source_b = ImageBufferF32::from_fn(32, 16, |x, _y| {
            let t = x as f32 / 31.0;
            [1.0 - t, (1.0 - (2.0 * t - 1.0).abs()).max(0.0), t, 1.0]
        })
        .expect("B fixture");

        let settings = RuttEtraSettings {
            line_pitch: 4,
            displacement_depth: 6.0,
            line_thickness: 2,
            mono: false,
        };

        let cpu = render_rutt_etra_two_source_frame(&source_a, &source_b, &settings)
            .expect("cpu two-source render");
        let gpu = match rutt_etra_two_source_metal(&source_a, &source_b, &settings) {
            Ok(img) => img,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal rutt-etra two-source parity: no Metal device");
                return;
            }
            Err(e) => panic!("rutt_etra_two_source_metal failed: {e}"),
        };

        assert_eq!(
            gpu.pixels, cpu.pixels,
            "Metal two-source output must be byte-identical to the CPU reference"
        );
    }

    fn assert_image_near(actual: &ImageBufferF32, expected: &ImageBufferF32, epsilon: f32) {
        assert_eq!(actual.width, expected.width);
        assert_eq!(actual.height, expected.height);
        assert_eq!(actual.pixels.len(), expected.pixels.len());

        for (index, (actual, expected)) in actual.pixels.iter().zip(&expected.pixels).enumerate() {
            for channel in 0..4 {
                let delta = (actual[channel] - expected[channel]).abs();
                assert!(
                    delta <= epsilon,
                    "pixel {index} channel {channel}: expected {}, got {}",
                    expected[channel],
                    actual[channel]
                );
            }
        }
    }
}
