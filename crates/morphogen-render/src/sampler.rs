use crate::ImageBufferF32;

/// Samples float RGBA pixels with bilinear interpolation. Coordinates outside
/// the image are clamped to the nearest border pixel.
pub fn sample_bilinear_clamped(image: &ImageBufferF32, x: f32, y: f32) -> [f32; 4] {
    if image.width == 0 || image.height == 0 {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let max_x = (image.width - 1) as f32;
    let max_y = (image.height - 1) as f32;
    let clamped_x = x.clamp(0.0, max_x);
    let clamped_y = y.clamp(0.0, max_y);

    let x0 = clamped_x.floor() as u32;
    let y0 = clamped_y.floor() as u32;
    let x1 = (x0 + 1).min(image.width - 1);
    let y1 = (y0 + 1).min(image.height - 1);

    let tx = clamped_x - x0 as f32;
    let ty = clamped_y - y0 as f32;

    let c00 = image.pixel(x0, y0).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let c10 = image.pixel(x1, y0).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let c01 = image.pixel(x0, y1).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let c11 = image.pixel(x1, y1).unwrap_or([0.0, 0.0, 0.0, 0.0]);

    let top = mix_rgba(c00, c10, tx);
    let bottom = mix_rgba(c01, c11, tx);
    mix_rgba(top, bottom, ty)
}

fn mix_rgba(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}
