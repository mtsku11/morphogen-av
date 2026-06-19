use metal::{
    CompileOptions, Device, MTLCommandBufferStatus, MTLPixelFormat, MTLRegion, MTLSize,
    MTLStorageMode, MTLTextureType, MTLTextureUsage, Texture, TextureDescriptor,
};
use morphogen_render::{FlowField, ImageBufferF32};

use crate::{
    FlowDisplaceDispatchPlan, MetalDispatchError, FLOW_DISPLACE_KERNEL_NAME,
    FLOW_DISPLACE_SHADER_SOURCE,
};

#[repr(C)]
struct FlowDisplaceParams {
    amount: f32,
    width: u32,
    height: u32,
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

fn upload_rgba_f32_texture(
    texture: &Texture,
    image: &ImageBufferF32,
) -> Result<(), MetalDispatchError> {
    let bytes = rgba_f32_bytes(&image.pixels)?;
    replace_texture_bytes(texture, image.width, image.height, 16, &bytes)
}

fn upload_rg_f32_texture(texture: &Texture, flow: &FlowField) -> Result<(), MetalDispatchError> {
    let bytes = rg_f32_bytes(&flow.vectors)?;
    replace_texture_bytes(texture, flow.width, flow.height, 8, &bytes)
}

fn replace_texture_bytes(
    texture: &Texture,
    width: u32,
    height: u32,
    bytes_per_pixel: usize,
    bytes: &[u8],
) -> Result<(), MetalDispatchError> {
    let bytes_per_row = checked_row_bytes(width, bytes_per_pixel)?;
    let image_stride = checked_image_bytes(height, bytes_per_row)?;
    texture.replace_region_in_slice(
        MTLRegion::new_2d(0, 0, width as u64, height as u64),
        0,
        0,
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

#[cfg(test)]
mod tests {
    use morphogen_render::{flow_displace_cpu, FlowField, ImageBufferF32};

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
