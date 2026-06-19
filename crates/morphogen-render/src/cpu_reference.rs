use crate::{sample_bilinear_clamped, FlowField, ImageBufferF32, RenderError};

pub fn flow_displace_cpu(
    carrier: &ImageBufferF32,
    flow: &FlowField,
    amount: f32,
) -> Result<ImageBufferF32, RenderError> {
    if carrier.width != flow.width || carrier.height != flow.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "carrier is {}x{}, flow is {}x{}",
            carrier.width, carrier.height, flow.width, flow.height
        )));
    }

    ImageBufferF32::from_fn(carrier.width, carrier.height, |x, y| {
        let vector = flow.vector(x, y).unwrap_or([0.0, 0.0]);
        let sample_x = x as f32 + vector[0] * amount;
        let sample_y = y as f32 + vector[1] * amount;
        sample_bilinear_clamped(carrier, sample_x, sample_y)
    })
}
