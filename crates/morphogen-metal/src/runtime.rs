use metal::{
    CompileOptions, Device, MTLCommandBufferStatus, MTLPixelFormat, MTLRegion, MTLResourceOptions,
    MTLSize, MTLStorageMode, MTLTextureType, MTLTextureUsage, Texture, TextureDescriptor,
};
use morphogen_render::{
    FlowFeedbackSettings, FlowField, GrainPool, GrainSelection, GranularMosaicSettings,
    ImageBufferF32, StructureMode,
};

use crate::{
    FlowDisplaceDispatchPlan, GranularMosaicDispatchPlan, MetalDispatchError,
    ADVECT_FEEDBACK_KERNEL_NAME, ADVECT_FEEDBACK_SHADER_SOURCE, FLOW_DISPLACE_KERNEL_NAME,
    FLOW_DISPLACE_SHADER_SOURCE, GRANULAR_MOSAIC_KERNEL_NAME, GRANULAR_MOSAIC_POOL_KERNEL_NAME,
    GRANULAR_MOSAIC_POOL_SHADER_SOURCE, GRANULAR_MOSAIC_SHADER_SOURCE,
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

#[cfg(test)]
mod tests {
    use morphogen_render::{
        analyze_grain_pool_cpu, flow_displace_cpu, flow_feedback_frame_cpu,
        granular_mosaic_with_pool_selection_cpu, granular_mosaic_with_selection_cpu,
        select_grains_from_pool_cpu, FlowFeedbackSettings, FlowField, GrainSelection,
        GranularMosaicSettings, ImageBufferF32, StructureMode,
    };

    use super::*;

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
            morphogen_render::PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("selection");

        let cpu =
            granular_mosaic_with_pool_selection_cpu(&pool_frames, &pool, carrier, &selection, settings)
                .expect("cpu render");
        let gpu = match granular_mosaic_pool_metal(&pool_frames, &pool, carrier, &selection, settings)
        {
            Ok(image) => image,
            Err(MetalDispatchError::DeviceUnavailable) => {
                eprintln!("skipping Metal pool parity assertion because no Metal device is available");
                return;
            }
            Err(error) => panic!("metal pool render failed: {error}"),
        };

        assert_image_near(&gpu, &cpu, 1.0 / 255.0);
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
