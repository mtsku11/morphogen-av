#![forbid(unsafe_code)]

pub mod cpu_reference;
pub mod error;
pub mod flow;
pub mod flow_cache;
pub mod image_buffer;
pub mod luminance_flow;
pub mod sampler;

pub use cpu_reference::flow_displace_cpu;
pub use error::RenderError;
pub use flow::FlowField;
pub use flow_cache::{read_flow_cache, write_flow_cache, FlowCacheFrame, FlowCacheManifest};
pub use image_buffer::ImageBufferF32;
pub use luminance_flow::luminance_gradient_flow_cpu;
pub use sampler::sample_bilinear_clamped;

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn bilinear_sampling_averages_four_pixels() {
        let image = ImageBufferF32::new(
            2,
            2,
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [1.0, 1.0, 0.0, 1.0],
            ],
        )
        .expect("valid image");

        let sampled = sample_bilinear_clamped(&image, 0.5, 0.5);

        assert!((sampled[0] - 0.5).abs() < 0.000_001);
        assert!((sampled[1] - 0.5).abs() < 0.000_001);
        assert_eq!(sampled[3], 1.0);
    }

    #[test]
    fn bilinear_sampling_clamps_at_borders() {
        let image = ImageBufferF32::new(2, 1, vec![[0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0]])
            .expect("valid image");

        let sampled = sample_bilinear_clamped(&image, 10.0, 0.0);

        assert_eq!(sampled, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn flow_displacement_moves_carrier_sampling_coordinates() {
        let carrier = ImageBufferF32::new(
            3,
            1,
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
            ],
        )
        .expect("valid carrier");
        let flow =
            FlowField::new(3, 1, vec![[1.0, 0.0], [1.0, 0.0], [1.0, 0.0]]).expect("valid flow");

        let displaced = flow_displace_cpu(&carrier, &flow, 1.0).expect("displace");

        assert_eq!(displaced.pixel(0, 0), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(displaced.pixel(2, 0), Some([0.0, 1.0, 0.0, 1.0]));
    }

    #[test]
    fn flow_displacement_matches_checked_in_golden_fixture() {
        let fixture: FlowDisplaceGoldenFixture = serde_json::from_str(include_str!(
            "../../../tests/fixtures/render/flow_displace_cpu_golden.json"
        ))
        .expect("parse golden fixture");

        assert!(!fixture.description.is_empty());
        let rendered = flow_displace_cpu(&fixture.carrier, &fixture.flow, fixture.amount)
            .expect("render golden fixture");

        assert_image_near(&rendered, &fixture.expected, 0.000_001);
    }

    #[derive(Deserialize)]
    struct FlowDisplaceGoldenFixture {
        description: String,
        carrier: ImageBufferF32,
        flow: FlowField,
        amount: f32,
        expected: ImageBufferF32,
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
