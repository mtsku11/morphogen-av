#![forbid(unsafe_code)]

pub mod cpu_reference;
pub mod error;
pub mod feedback_state;
pub mod flow;
pub mod flow_cache;
pub mod image_buffer;
pub mod luminance_flow;
pub mod sampler;

pub use cpu_reference::{flow_displace_cpu, flow_feedback_frame_cpu, FlowFeedbackSettings};
pub use error::RenderError;
pub use feedback_state::{
    feedback_state_path, read_flow_feedback_state, write_flow_feedback_state,
    FlowFeedbackStateDescriptor, FLOW_FEEDBACK_STATE_VERSION,
};
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

    #[test]
    fn flow_feedback_frame_zero_uses_only_the_displaced_carrier() {
        let carrier = ImageBufferF32::new(
            3,
            1,
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [0.5, 0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
            ],
        )
        .expect("carrier");
        let flow = FlowField::new(3, 1, vec![[1.0, 0.0]; 3]).expect("flow");
        let settings = FlowFeedbackSettings {
            carrier_amount: 1.0,
            feedback_amount: 99.0,
            feedback_mix: 1.0,
            decay: 0.0,
            iterations: 1,
        };

        let feedback = flow_feedback_frame_cpu(&carrier, None, &flow, settings).expect("frame");
        let displaced = flow_displace_cpu(&carrier, &flow, 1.0).expect("displace");

        assert_eq!(feedback, displaced);
    }

    #[test]
    fn flow_feedback_blends_advected_previous_float_output() {
        let first_carrier =
            ImageBufferF32::new(1, 1, vec![[0.2, 0.0, 0.0, 1.0]]).expect("first carrier");
        let second_carrier =
            ImageBufferF32::new(1, 1, vec![[0.8, 0.0, 0.0, 1.0]]).expect("second carrier");
        let flow = FlowField::new(1, 1, vec![[0.0, 0.0]]).expect("flow");
        let settings = FlowFeedbackSettings {
            carrier_amount: 0.0,
            feedback_amount: 0.0,
            feedback_mix: 0.5,
            decay: 0.5,
            iterations: 1,
        };

        let frame_zero =
            flow_feedback_frame_cpu(&first_carrier, None, &flow, settings).expect("frame zero");
        let frame_one =
            flow_feedback_frame_cpu(&second_carrier, Some(&frame_zero), &flow, settings)
                .expect("frame one");

        assert_image_near(
            &frame_one,
            &ImageBufferF32::new(1, 1, vec![[0.45, 0.0, 0.0, 0.75]]).expect("expected"),
            0.000_001,
        );
    }

    #[test]
    fn feedback_state_can_resume_a_sequence_without_float_drift() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = feedback_state_path(temp_dir.path(), 0);
        let flow = FlowField::new(1, 1, vec![[0.0, 0.0]]).expect("flow");
        let settings = FlowFeedbackSettings {
            carrier_amount: 0.0,
            feedback_amount: 0.0,
            feedback_mix: 0.75,
            decay: 0.9,
            iterations: 1,
        };
        let carriers = [
            [0.2, 0.0, 0.0, 1.0],
            [0.6, 0.0, 0.0, 1.0],
            [0.9, 0.0, 0.0, 1.0],
        ];

        let mut uninterrupted = None;
        for pixel in carriers {
            let carrier = ImageBufferF32::new(1, 1, vec![pixel]).expect("carrier");
            uninterrupted = Some(
                flow_feedback_frame_cpu(&carrier, uninterrupted.as_ref(), &flow, settings)
                    .expect("uninterrupted frame"),
            );
        }

        let first = ImageBufferF32::new(1, 1, vec![carriers[0]]).expect("first carrier");
        let initial = flow_feedback_frame_cpu(&first, None, &flow, settings).expect("initial");
        write_flow_feedback_state(&path, &initial).expect("write state");
        let (_, mut resumed) = read_flow_feedback_state(&path).expect("read state");
        for pixel in carriers.into_iter().skip(1) {
            let carrier = ImageBufferF32::new(1, 1, vec![pixel]).expect("carrier");
            resumed = flow_feedback_frame_cpu(&carrier, Some(&resumed), &flow, settings)
                .expect("resumed frame");
        }

        assert_eq!(Some(resumed), uninterrupted);
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
