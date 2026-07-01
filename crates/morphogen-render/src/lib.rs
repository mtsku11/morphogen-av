#![forbid(unsafe_code)]

pub mod audio_route;
pub mod block_collage;
pub mod cascade_collage;
pub mod channel_shift;
pub mod palette_quantize;
pub mod pixel_sort;
pub mod cascade_trails;
pub mod coagulate;
pub mod conv_blend;
pub mod cpu_reference;
pub mod datamosh;
pub mod disperse;
pub mod error;
pub mod feedback_state;
pub mod field_particles;
pub mod flow;
pub mod flow_cache;
pub mod fluid_advect;
pub mod fluid_mosaic;
pub mod grain_cache;
pub mod granular_mosaic;
pub mod image_buffer;
pub mod luminance_flow;
pub mod optical_flow;
pub mod retro_static;
pub mod sampler;
pub mod video_vocoder;
pub mod vortex_field;

pub use audio_route::{
    uniform_displacement_field, RmsDisplacementEnvelope, RMS_DISPLACEMENT_ROUTE_ALGORITHM,
};
pub use block_collage::{
    render_block_collage_frame, BlockCollageSettings, BLOCK_COLLAGE_ALGORITHM,
};
pub use cascade_collage::{
    render_cascade_collage_frame, BlendMode, CascadeCollageSettings, CascadeShape, Notch,
    ScribbleEdge, CASCADE_COLLAGE_ALGORITHM,
};
pub use channel_shift::{
    compute_per_row_shifts, render_channel_shift_frame, ChannelShiftSettings,
    CHANNEL_SHIFT_ALGORITHM, CHANNEL_SHIFT_FLOW_ALGORITHM,
};
pub use palette_quantize::{
    render_palette_quantize_frame, PaletteQuantizeSettings, QuantizeMode, NEON_PALETTE,
    PALETTE_QUANTIZE_ALGORITHM,
};
pub use pixel_sort::{
    compute_a_edge_mask, compute_a_flow_mask, compute_a_luma_mask, render_pixel_sort_frame,
    MaskSource, PixelSortSettings, SortAxis, SortDirection, SortKey, PIXEL_SORT_ALGORITHM,
    PIXEL_SORT_CROSS_SYNTH_ALGORITHM,
};
pub use cascade_trails::{
    advance_cascade_trails, assign_temporal_patches, initialize_cascade_trails,
    render_cascade_trails, CascadeFieldType, CascadeTrailSettings, CascadeTrailState,
    CASCADE_TRAIL_ALGORITHM,
};
pub use coagulate::{
    advance_coagulation_field, advect_coagulation_field, apply_history_smear, average_cell_flows,
    coagulated_blend_frame_cpu, coagulated_blend_temporal_frame_cpu, coagulation_field,
    composite_with_field, downsample_flow_to_cells, synthesize_turbulence_flow, CoagulationField,
    CoagulationFlowSource, CoagulationSettings, COAGULATED_BLEND_ALGORITHM,
};
pub use conv_blend::{
    analyze_convolution_kernel_cpu, analyze_convolution_kernels_color_cpu,
    convolution_blend_color_cpu, convolution_blend_color_from_modulator_cpu, convolution_blend_cpu,
    convolution_blend_from_modulator_cpu, ConvolutionBlendSettings, ConvolutionKernel,
    CONVOLUTION_BLEND_ALGORITHM, CONVOLUTION_BLEND_COLOR_ALGORITHM,
};
pub use cpu_reference::{
    flow_displace_cpu, flow_feedback_frame_cpu, flow_temporal_supersample_cpu,
    FlowFeedbackSettings, StructureMode,
};
pub use datamosh::{
    block_motion_refreshes, datamosh_algorithm, datamosh_block_frame_cpu,
    datamosh_block_refresh_composite, datamosh_bloom_frame_cpu, datamosh_codec_engrave_frame_cpu,
    datamosh_refresh_frame_cpu, datamosh_residual_flow, datamosh_residual_frame_cpu,
    datamosh_scanline_smear_frame_cpu, is_datamosh_keyframe, quantize_flow_to_blocks,
    remix_block_vectors, reset_residual_in_refreshed_blocks, zero_flow, CodecEngraveSettings,
    ScanlineSmearSettings, VectorRemixMode, DATAMOSH_BLOCK_ALGORITHM,
    DATAMOSH_BLOCK_REFRESH_ALGORITHM, DATAMOSH_BLOCK_RESIDUAL_ALGORITHM, DATAMOSH_BLOOM_ALGORITHM,
    DATAMOSH_CODEC_ENGRAVE_ALGORITHM, DATAMOSH_SCANLINE_SMEAR_ALGORITHM,
    DATAMOSH_VECTOR_REMIX_ALGORITHM,
};
pub use disperse::{
    advance_dispersion_field, disperse_composite_cpu, DispersionField, DispersionSettings,
    DISPERSION_BLEND_ALGORITHM,
};
pub use error::RenderError;
pub use feedback_state::{
    feedback_state_path, read_flow_feedback_state, write_flow_feedback_state,
    FlowFeedbackStateDescriptor, FLOW_FEEDBACK_STATE_VERSION,
};
pub use field_particles::{
    advance_field_particles, initialize_field_particles, refresh_field_particle_colors,
    render_field_particles, FieldParticleSettings, ParticleField, FIELD_PARTICLES_ALGORITHM,
};
pub use flow::FlowField;
pub use flow_cache::{
    read_flow_cache, write_flow_cache, write_flow_cache_with_source_fingerprint, FlowCacheFrame,
    FlowCacheManifest, FLOW_VECTOR_CONVENTION,
};
pub use fluid_advect::{
    fluid_advect_frame_cpu, fluid_advect_two_source_frame_cpu, FluidAdvectSettings,
    FluidAdvectTwoSourceSettings, FLUID_ADVECT_ALGORITHM, FLUID_ADVECT_TWO_SOURCE_ALGORITHM,
};
pub use fluid_mosaic::{
    advance_fluid_mosaic, initialize_fluid_mosaic, refresh_fluid_mosaic_colors,
    render_fluid_mosaic, resort_fluid_mosaic_colors, FluidMosaicSettings, FluidMosaicState,
    TileOrigin, TilePatch, FLUID_MOSAIC_ALGORITHM,
};
pub use grain_cache::{
    read_grain_color_descriptor_cache, read_grain_descriptor_cache,
    read_grain_pool_descriptor_cache, read_grain_selection_cache,
    write_grain_color_descriptor_cache, write_grain_descriptor_cache,
    write_grain_pool_descriptor_cache, write_grain_selection_cache,
    GranularMosaicColorDescriptorCache, GranularMosaicDescriptorCache,
    GranularMosaicPoolDescriptorCache, GranularMosaicSelectionCache,
    GRAIN_COLOR_DESCRIPTOR_CACHE_FILE_NAME, GRAIN_DESCRIPTOR_CACHE_FILE_NAME,
    GRAIN_POOL_DESCRIPTOR_CACHE_FILE_NAME, GRAIN_SELECTION_CACHE_FILE_NAME,
};
pub use granular_mosaic::{
    analyze_grain_colors_cpu, analyze_grain_pool_cpu, analyze_grains_cpu, granular_mosaic_cpu,
    granular_mosaic_with_pool_selection_cpu, granular_mosaic_with_selection_cpu, select_grains_cpu,
    select_grains_from_pool_cpu, select_grains_multimodal_cpu, AntiRepeat, GrainColorDescriptor,
    GrainDescriptor, GrainPool, GrainSelection, GranularMosaicSettings, PoolSelectionWindow,
    PooledGrainDescriptor, TemporalCoherence, GRANULAR_MOSAIC_ALGORITHM,
    MULTIMODAL_GRAIN_ALGORITHM, POOLED_GRAIN_ALGORITHM,
};
pub use image_buffer::ImageBufferF32;
pub use luminance_flow::luminance_gradient_flow_cpu;
pub use optical_flow::{
    lucas_kanade_flow_cpu, pyramidal_lucas_kanade_flow_cpu,
    pyramidal_lucas_kanade_flow_with_refiner, refine_level_cpu, FlowConfidenceMap,
    LucasKanadeLevelRefiner, PyramidalLucasKanadeEstimate, LUCAS_KANADE_WINDOW_RADIUS,
    PYRAMIDAL_LUCAS_KANADE_MAX_LEVELS, PYRAMIDAL_LUCAS_KANADE_WARP_ITERATIONS,
};
pub use retro_static::{
    render_retro_static_frame, RetroStaticSettings, ScanlineFilter, RETRO_STATIC_ALGORITHM,
};
pub use sampler::sample_bilinear_clamped;
pub use video_vocoder::{
    analyze_luma_band_envelope_cpu, apply_tone_map_cpu, histogram_specification_cpu,
    luma_specification_tone_map, video_vocoder_cpu, video_vocoder_from_modulator_cpu,
    LumaBandEnvelope, VideoVocoderSettings, TONE_MAP_LEVELS, VIDEO_VOCODER_ALGORITHM,
};
pub use vortex_field::steady_vortex_velocity;

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
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
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
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
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
    fn temporal_supersampling_blurs_along_the_current_flow_without_mutating_input() {
        let image = ImageBufferF32::new(
            3,
            1,
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        )
        .expect("image");
        let flow = FlowField::new(3, 1, vec![[1.0, 0.0]; 3]).expect("flow");

        let integrated =
            flow_temporal_supersample_cpu(&image, &flow, 1.0, 2).expect("temporal integration");

        assert_eq!(image.pixel(1, 0), Some([1.0, 0.0, 0.0, 1.0]));
        let center = integrated.pixel(1, 0).expect("center pixel");
        assert!((center[0] - 0.75).abs() < 0.000_001);
        assert_eq!(center[3], 1.0);
    }

    #[test]
    fn one_temporal_sample_returns_the_exact_float_image() {
        let image = ImageBufferF32::new(1, 1, vec![[0.123_456, 0.5, 0.75, 1.0]]).expect("image");
        let flow = FlowField::new(1, 1, vec![[0.0, 0.0]]).expect("flow");

        assert_eq!(
            flow_temporal_supersample_cpu(&image, &flow, 4.0, 1).expect("one sample"),
            image
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
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
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

    fn checkerboard(size: u32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(size, size, |x, y| {
            let value = if (x + y) % 2 == 0 { 0.9 } else { 0.1 };
            [value, value, value, 1.0]
        })
        .expect("checkerboard")
    }

    /// Sum of absolute neighbor differences on the luma channel; a proxy for how
    /// much high-frequency detail (structure) a frame still carries. Trends to
    /// zero as a frame washes out to flat fog.
    fn total_variation(image: &ImageBufferF32) -> f32 {
        let mut total = 0.0;
        for y in 0..image.height {
            for x in 0..image.width {
                let here = image.pixel(x, y).expect("pixel")[0];
                if x + 1 < image.width {
                    total += (image.pixel(x + 1, y).expect("pixel")[0] - here).abs();
                }
                if y + 1 < image.height {
                    total += (image.pixel(x, y + 1).expect("pixel")[0] - here).abs();
                }
            }
        }
        total
    }

    #[test]
    fn structure_mix_adds_nothing_on_a_flat_carrier() {
        // A flat carrier has no high-frequency band, so structure-preserving
        // re-injection must be a no-op regardless of the mix amount.
        let carrier = ImageBufferF32::new(4, 4, vec![[0.5, 0.5, 0.5, 1.0]; 16]).expect("carrier");
        let previous = ImageBufferF32::new(4, 4, vec![[0.2, 0.2, 0.2, 1.0]; 16]).expect("previous");
        let flow = FlowField::new(4, 4, vec![[0.0, 0.0]; 16]).expect("flow");
        let base = FlowFeedbackSettings {
            carrier_amount: 1.0,
            feedback_amount: 1.0,
            feedback_mix: 0.7,
            decay: 0.9,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
        };

        let without = flow_feedback_frame_cpu(&carrier, Some(&previous), &flow, base).expect("a");
        let with = flow_feedback_frame_cpu(
            &carrier,
            Some(&previous),
            &flow,
            FlowFeedbackSettings {
                structure_mix: 0.8,
                ..base
            },
        )
        .expect("b");

        assert_image_near(&with, &without, 0.000_001);
    }

    #[test]
    fn structure_mix_leaves_frame_zero_unchanged() {
        // Frame zero (no history) is the displaced carrier by contract;
        // structure re-injection only applies to frames that have history.
        let carrier = checkerboard(8);
        let flow = FlowField::new(8, 8, vec![[0.0, 0.0]; 64]).expect("flow");
        let base = FlowFeedbackSettings {
            carrier_amount: 1.0,
            feedback_amount: 1.0,
            feedback_mix: 0.7,
            decay: 0.9,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
        };

        let without = flow_feedback_frame_cpu(&carrier, None, &flow, base).expect("a");
        let with = flow_feedback_frame_cpu(
            &carrier,
            None,
            &flow,
            FlowFeedbackSettings {
                structure_mix: 0.9,
                ..base
            },
        )
        .expect("b");

        assert_image_near(&with, &without, 0.0);
    }

    #[test]
    fn structure_mix_reinjects_high_frequency_detail() {
        // Against a washed-out (flat-gray) history, structure re-injection must
        // restore the carrier's edges, raising the frame's total variation.
        let carrier = checkerboard(8);
        let washed = ImageBufferF32::new(8, 8, vec![[0.5, 0.5, 0.5, 1.0]; 64]).expect("washed");
        let flow = FlowField::new(8, 8, vec![[0.0, 0.0]; 64]).expect("flow");
        let base = FlowFeedbackSettings {
            carrier_amount: 0.0,
            feedback_amount: 0.0,
            feedback_mix: 0.97,
            decay: 1.0,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
        };

        let plain = flow_feedback_frame_cpu(&carrier, Some(&washed), &flow, base).expect("plain");
        let structured = flow_feedback_frame_cpu(
            &carrier,
            Some(&washed),
            &flow,
            FlowFeedbackSettings {
                structure_mix: 0.8,
                ..base
            },
        )
        .expect("structured");

        assert!(
            total_variation(&structured) > total_variation(&plain) * 2.0,
            "structure_mix should raise detail: plain={}, structured={}",
            total_variation(&plain),
            total_variation(&structured)
        );
    }

    #[test]
    fn structure_mix_resists_washout_across_a_sequence() {
        // Fractional flow blurs the advected history every frame; with high
        // feedback_mix the plain renderer collapses to flat fog. Structure
        // re-injection keeps regenerating detail so the frame stays sharp.
        let carrier = checkerboard(8);
        let flow = FlowField::new(8, 8, vec![[0.3, 0.0]; 64]).expect("flow");
        let base = FlowFeedbackSettings {
            carrier_amount: 0.0,
            feedback_amount: 1.0,
            feedback_mix: 0.99,
            decay: 1.0,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
        };
        let structured = FlowFeedbackSettings {
            structure_mix: 0.6,
            ..base
        };

        let mut plain_state: Option<ImageBufferF32> = None;
        let mut structured_state: Option<ImageBufferF32> = None;
        for _ in 0..12 {
            plain_state = Some(
                flow_feedback_frame_cpu(&carrier, plain_state.as_ref(), &flow, base)
                    .expect("plain"),
            );
            structured_state = Some(
                flow_feedback_frame_cpu(&carrier, structured_state.as_ref(), &flow, structured)
                    .expect("structured"),
            );
        }

        let plain_tv = total_variation(&plain_state.expect("plain final"));
        let structured_tv = total_variation(&structured_state.expect("structured final"));
        assert!(
            structured_tv > plain_tv * 3.0,
            "structured morph should resist washout: plain_tv={plain_tv}, structured_tv={structured_tv}"
        );
    }

    #[test]
    fn multiscale_structure_mode_leaves_the_zero_mix_path_untouched() {
        // structure_mix == 0 must short-circuit before any mode-specific work, so
        // single-scale and multiscale produce the identical (unmodified) frame.
        let carrier = checkerboard(8);
        let previous = ImageBufferF32::new(8, 8, vec![[0.3, 0.3, 0.3, 1.0]; 64]).expect("previous");
        let flow = FlowField::new(8, 8, vec![[0.2, 0.1]; 64]).expect("flow");
        let single = FlowFeedbackSettings {
            carrier_amount: 1.0,
            feedback_amount: 1.0,
            feedback_mix: 0.7,
            decay: 0.9,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
        };
        let multi = FlowFeedbackSettings {
            structure_mode: StructureMode::Multiscale,
            ..single
        };

        let from_single =
            flow_feedback_frame_cpu(&carrier, Some(&previous), &flow, single).expect("single");
        let from_multi =
            flow_feedback_frame_cpu(&carrier, Some(&previous), &flow, multi).expect("multi");

        assert_eq!(from_single, from_multi);
    }

    #[test]
    fn multiscale_structure_mode_resists_washout_across_a_sequence() {
        // Same washout stress as the single-scale test: the multiscale path must
        // also keep regenerating detail instead of collapsing to flat fog.
        let carrier = checkerboard(16);
        let flow = FlowField::new(16, 16, vec![[0.3, 0.0]; 256]).expect("flow");
        let plain = FlowFeedbackSettings {
            carrier_amount: 0.0,
            feedback_amount: 1.0,
            feedback_mix: 0.99,
            decay: 1.0,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
        };
        let multi = FlowFeedbackSettings {
            structure_mix: 0.8,
            structure_mode: StructureMode::Multiscale,
            ..plain
        };

        let mut plain_state: Option<ImageBufferF32> = None;
        let mut multi_state: Option<ImageBufferF32> = None;
        for _ in 0..12 {
            plain_state = Some(
                flow_feedback_frame_cpu(&carrier, plain_state.as_ref(), &flow, plain)
                    .expect("plain"),
            );
            multi_state = Some(
                flow_feedback_frame_cpu(&carrier, multi_state.as_ref(), &flow, multi)
                    .expect("multi"),
            );
        }

        let plain_tv = total_variation(&plain_state.expect("plain final"));
        let multi_tv = total_variation(&multi_state.expect("multi final"));
        assert!(
            multi_tv > plain_tv * 3.0,
            "multiscale morph should resist washout: plain_tv={plain_tv}, multi_tv={multi_tv}"
        );
    }

    #[test]
    fn multiscale_mask_concentrates_detail_on_morphed_structure() {
        // The structure mask is taken from the morphed (advected) history. Where
        // that history is flat, re-injection is held near the floor; where it has
        // an edge, re-injection runs at full strength. So a history with a single
        // strong edge must re-seed more carrier detail near that edge than far
        // from it. carrier_amount/feedback_amount are zero so neither input moves
        // and the only difference between regions is the mask response.
        let carrier = checkerboard(16);
        // Previous frame: left half dark, right half bright -> one vertical edge
        // down the middle column.
        let previous = ImageBufferF32::from_fn(16, 16, |x, _| {
            let value = if x < 8 { 0.1 } else { 0.9 };
            [value, value, value, 1.0]
        })
        .expect("previous");
        let flow = FlowField::new(16, 16, vec![[0.0, 0.0]; 256]).expect("flow");
        let plain = FlowFeedbackSettings {
            carrier_amount: 0.0,
            feedback_amount: 0.0,
            feedback_mix: 0.97,
            decay: 1.0,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
        };
        let multi = FlowFeedbackSettings {
            structure_mix: 0.8,
            structure_mode: StructureMode::Multiscale,
            ..plain
        };

        let plain_frame =
            flow_feedback_frame_cpu(&carrier, Some(&previous), &flow, plain).expect("plain");
        let multi_frame =
            flow_feedback_frame_cpu(&carrier, Some(&previous), &flow, multi).expect("multi");

        // Re-injected detail = multiscale frame minus the plain (un-re-injected)
        // frame. Measure its magnitude in a column near the morphed edge vs. a
        // column in the flat far region.
        let injected_energy = |column: u32| {
            let mut total = 0.0;
            for y in 0..16 {
                for channel in 0..3 {
                    let injected = multi_frame.pixel(column, y).expect("multi")[channel]
                        - plain_frame.pixel(column, y).expect("plain")[channel];
                    total += injected.abs();
                }
            }
            total
        };

        let near_edge = injected_energy(7);
        let far_from_edge = injected_energy(1);
        assert!(
            near_edge > far_from_edge,
            "mask should bias re-injection toward the morphed edge: near={near_edge}, far={far_from_edge}"
        );
    }
}
