use std::{fs, path::Path};

use assert_cmd::Command;
use image::{ImageBuffer, Rgba};
use morphogen_render::read_flow_cache;
use predicates::prelude::*;

#[test]
fn init_and_inspect_example_project() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let project_path = temp_dir.path().join("example.morphogen.json");
    let project_arg = project_path.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["init-example", project_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote example project"));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["inspect-project", project_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Two Source Flow Displace"));
}

#[test]
fn render_test_writes_png() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let output_path = temp_dir.path().join("render.png");
    let output_arg = output_path.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["render-test", output_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote CPU reference render"));

    assert!(output_path.exists());
}

#[test]
fn help_lists_metal_render_test_validation_command() {
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("metal-render-test"));
}

#[test]
fn render_two_source_writes_png_from_real_image_inputs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_path = temp_dir.path().join("modulator.png");
    let carrier_path = temp_dir.path().join("carrier.png");
    let output_path = temp_dir.path().join("two-source.png");
    let flow_cache_dir = temp_dir.path().join("flow-cache");
    let modulator_arg = modulator_path.to_string_lossy().to_string();
    let carrier_arg = carrier_path.to_string_lossy().to_string();
    let output_arg = output_path.to_string_lossy().to_string();
    let flow_cache_arg = flow_cache_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["render-test", modulator_arg.as_str()])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["render-test", carrier_arg.as_str()])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-two-source",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--amount",
            "12",
            "--flow-cache-dir",
            flow_cache_arg.as_str(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered two-source CPU displacement",
        ));

    assert!(output_path.exists());
    assert!(flow_cache_dir.join("manifest.json").exists());
    assert!(flow_cache_dir.join("frame_000000.flowf32").exists());
}

#[test]
fn render_granular_mosaic_writes_image_and_frame_sequence() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_path = temp_dir.path().join("modulator.png");
    let carrier_path = temp_dir.path().join("carrier.png");
    let output_path = temp_dir.path().join("mosaic.png");
    let grain_cache_dir = temp_dir.path().join("grain-cache");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let output_dir = temp_dir.path().join("mosaic-frames");

    let modulator = ImageBuffer::from_fn(4, 2, |x, _| {
        let value = x as u8 * 85;
        Rgba([value, value, value, u8::MAX])
    });
    let carrier = ImageBuffer::from_fn(4, 2, |x, y| {
        Rgba([x as u8 * 60, y as u8 * 120, (x + y) as u8 * 40, u8::MAX])
    });
    modulator.save(&modulator_path).expect("write modulator");
    carrier.save(&carrier_path).expect("write carrier");
    fs::create_dir_all(&modulator_dir).expect("create modulator frames");
    fs::create_dir_all(&carrier_dir).expect("create carrier frames");
    modulator
        .save(modulator_dir.join("frame_000001.png"))
        .expect("write sequence modulator");
    carrier
        .save(carrier_dir.join("frame_000001.png"))
        .expect("write sequence carrier");

    let modulator_arg = modulator_path.to_string_lossy().to_string();
    let carrier_arg = carrier_path.to_string_lossy().to_string();
    let output_arg = output_path.to_string_lossy().to_string();
    let grain_cache_arg = grain_cache_dir.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-granular-mosaic",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--grain-size",
            "1",
            "--rearrangement",
            "1",
            "--variation",
            "0",
            "--grain-cache-dir",
            grain_cache_arg.as_str(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("rendered granular mosaic"));
    assert!(output_path.exists());
    assert!(grain_cache_dir.join("grain_descriptors.json").exists());
    assert!(grain_cache_dir.join("grain_selection.json").exists());

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-granular-mosaic",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--grain-size",
            "1",
            "--rearrangement",
            "1",
            "--variation",
            "0",
            "--grain-cache-dir",
            grain_cache_arg.as_str(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("reused granular descriptor"));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-granular-mosaic",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--grain-size",
            "1",
            "--rearrangement",
            "1",
            "--variation",
            "0.5",
            "--grain-cache-dir",
            grain_cache_arg.as_str(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "reused granular descriptor cache and generated selection cache",
        ));

    let modulator_dir_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_dir_arg = carrier_dir.to_string_lossy().to_string();
    let output_dir_arg = output_dir.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-granular-mosaic-sequence",
            modulator_dir_arg.as_str(),
            carrier_dir_arg.as_str(),
            output_dir_arg.as_str(),
            "--grain-size",
            "2",
            "--seed",
            "42",
            "--grain-cache-dir",
            grain_cache_arg.as_str(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered granular mosaic sequence with 1 frame(s)",
        ));
    assert!(output_dir.join("frame_000000.png").exists());
    assert!(grain_cache_dir
        .join("frame_000000/grain_descriptors.json")
        .exists());
    assert!(grain_cache_dir
        .join("frame_000000/grain_selection.json")
        .exists());
}

#[test]
fn render_granular_mosaic_pool_sequence_writes_frames_and_pool_sidecar() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let output_dir = temp_dir.path().join("pool-frames");
    let grain_cache_dir = temp_dir.path().join("pool-cache");
    fs::create_dir_all(&modulator_dir).expect("create modulator frames");
    fs::create_dir_all(&carrier_dir).expect("create carrier frames");

    for index in 0..2u32 {
        let modulator = ImageBuffer::from_fn(4, 2, |x, _| {
            let value = ((x + index) as u8).wrapping_mul(60);
            Rgba([value, value, value, u8::MAX])
        });
        let carrier = ImageBuffer::from_fn(4, 2, |x, y| {
            Rgba([
                (x as u8).wrapping_mul(50).wrapping_add(index as u8 * 30),
                (y as u8).wrapping_mul(120),
                ((x + y) as u8).wrapping_mul(40),
                u8::MAX,
            ])
        });
        modulator
            .save(modulator_dir.join(format!("frame_{:06}.png", index + 1)))
            .expect("write modulator frame");
        carrier
            .save(carrier_dir.join(format!("frame_{:06}.png", index + 1)))
            .expect("write carrier frame");
    }

    // RMS caches for Source A (query) and Source B (pool grains).
    let modulator_wav = temp_dir.path().join("modulator.wav");
    let carrier_wav = temp_dir.path().join("carrier.wav");
    write_test_wav(&modulator_wav, &[0.0, 0.5, -0.5, 1.0, -1.0, 0.25, -0.25, 0.75]);
    write_test_wav(&carrier_wav, &[1.0, -1.0, 0.5, -0.5, 0.0, 0.8, -0.8, 0.2]);
    let modulator_rms = temp_dir.path().join("mod-rms.json");
    let carrier_rms = temp_dir.path().join("car-rms.json");
    for (wav, json) in [(&modulator_wav, &modulator_rms), (&carrier_wav, &carrier_rms)] {
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "cache-rms",
                wav.to_string_lossy().as_ref(),
                json.to_string_lossy().as_ref(),
                "--window-size",
                "2",
                "--hop-size",
                "2",
            ])
            .assert()
            .success();
    }

    let modulator_dir_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_dir_arg = carrier_dir.to_string_lossy().to_string();
    let output_dir_arg = output_dir.to_string_lossy().to_string();
    let grain_cache_arg = grain_cache_dir.to_string_lossy().to_string();
    let modulator_rms_arg = modulator_rms.to_string_lossy().to_string();
    let carrier_rms_arg = carrier_rms.to_string_lossy().to_string();
    let pool_args = [
        "render-granular-mosaic-pool-sequence",
        modulator_dir_arg.as_str(),
        carrier_dir_arg.as_str(),
        output_dir_arg.as_str(),
        "--grain-size",
        "2",
        "--audio-weight",
        "1.0",
        "--modulator-rms-cache",
        modulator_rms_arg.as_str(),
        "--carrier-rms-cache",
        carrier_rms_arg.as_str(),
        "--frame-rate",
        "2.0",
        "--grain-cache-dir",
        grain_cache_arg.as_str(),
    ];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(pool_args)
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote grain pool sidecar"))
        .stdout(predicate::str::contains("pooled_av_nearest_grain_cpu_v2"));

    assert!(output_dir.join("frame_000000.png").exists());
    assert!(output_dir.join("frame_000001.png").exists());
    let pool_sidecar = grain_cache_dir.join("grain_pool_descriptors.json");
    assert!(pool_sidecar.exists());
    let pool_json = fs::read_to_string(&pool_sidecar).expect("read pool sidecar");
    assert!(pool_json.contains("pooled_av_nearest_grain_cpu_v2"));

    // A second identical run reuses the persisted pool.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(pool_args)
        .assert()
        .success()
        .stdout(predicate::str::contains("reused grain pool sidecar"));
}

#[test]
fn render_frame_sequence_writes_pngs_and_per_frame_flow_caches() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let output_dir = temp_dir.path().join("output-frames");
    let flow_cache_dir = temp_dir.path().join("flow-cache");

    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let modulator_path = modulator_dir.join(frame_name);
        let carrier_path = carrier_dir.join(frame_name);
        let modulator_arg = modulator_path.to_string_lossy().to_string();
        let carrier_arg = carrier_path.to_string_lossy().to_string();

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", modulator_arg.as_str()])
            .assert()
            .success();

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", carrier_arg.as_str()])
            .assert()
            .success();
    }

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_dir.to_string_lossy().to_string();
    let flow_cache_arg = flow_cache_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-frame-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--amount",
            "8",
            "--flow-cache-dir",
            flow_cache_arg.as_str(),
            "--max-frames",
            "2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered frame sequence with 2 frame(s)",
        ));

    assert!(output_dir.join("frame_000000.png").exists());
    assert!(output_dir.join("frame_000001.png").exists());
    assert!(flow_cache_dir.join("frame_000000/manifest.json").exists());
    assert!(flow_cache_dir
        .join("frame_000000/frame_000000.flowf32")
        .exists());
    assert!(flow_cache_dir.join("frame_000001/manifest.json").exists());
    assert!(flow_cache_dir
        .join("frame_000001/frame_000000.flowf32")
        .exists());
}

#[test]
fn render_frame_sequence_can_modulate_amount_from_rms_wav() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let output_dir = temp_dir.path().join("output-frames");
    let wav_path = temp_dir.path().join("modulator.wav");

    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let modulator_path = modulator_dir.join(frame_name);
        let carrier_path = carrier_dir.join(frame_name);
        let modulator_arg = modulator_path.to_string_lossy().to_string();
        let carrier_arg = carrier_path.to_string_lossy().to_string();

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", modulator_arg.as_str()])
            .assert()
            .success();

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", carrier_arg.as_str()])
            .assert()
            .success();
    }
    write_test_wav(&wav_path, &[0.0, 0.0, 1.0, 1.0]);

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_dir.to_string_lossy().to_string();
    let wav_arg = wav_path.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-frame-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--amount",
            "8",
            "--rms-modulator-wav",
            wav_arg.as_str(),
            "--frame-rate",
            "2",
            "--rms-window-size",
            "2",
            "--rms-hop-size",
            "2",
            "--rms-amount-scale",
            "16",
            "--max-frames",
            "2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("applied RMS amount modulation"));

    let first_frame = fs::read(output_dir.join("frame_000000.png")).expect("read first frame");
    let second_frame = fs::read(output_dir.join("frame_000001.png")).expect("read second frame");

    assert_ne!(first_frame, second_frame);
}

#[test]
fn render_feedback_sequence_checkpoints_and_resumes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let resumed_output_dir = temp_dir.path().join("resumed-output");
    let uninterrupted_output_dir = temp_dir.path().join("uninterrupted-output");
    let reset_output_dir = temp_dir.path().join("reset-output");

    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        let modulator_arg = modulator_dir.join(frame_name).to_string_lossy().to_string();
        let carrier_arg = carrier_dir.join(frame_name).to_string_lossy().to_string();

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", modulator_arg.as_str()])
            .assert()
            .success();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", carrier_arg.as_str()])
            .assert()
            .success();
    }

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let resumed_arg = resumed_output_dir.to_string_lossy().to_string();
    let uninterrupted_arg = uninterrupted_output_dir.to_string_lossy().to_string();
    let reset_arg = reset_output_dir.to_string_lossy().to_string();
    let feedback_args = [
        "render-feedback-sequence",
        modulator_arg.as_str(),
        carrier_arg.as_str(),
        resumed_arg.as_str(),
        "--carrier-amount",
        "8",
        "--feedback-amount",
        "12",
        "--feedback-mix",
        "0.7",
        "--decay",
        "0.95",
        "--max-frames",
        "3",
    ];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(feedback_args)
        .arg("--stop-after-frame")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed flow-feedback sequence after frame 0",
        ));

    let partial_checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(resumed_output_dir.join("checkpoint.json"))
            .expect("read partial checkpoint"),
    )
    .expect("parse partial checkpoint");
    assert_eq!(partial_checkpoint["task"], "frame_sequence_flow_feedback");
    assert_eq!(partial_checkpoint["status"], "running");
    assert_eq!(partial_checkpoint["next_frame_index"], 1);
    assert!(resumed_output_dir
        .join("state/feedback_frame_000000.rgba32f")
        .exists());
    assert!(resumed_output_dir.join("frames/frame_000000.png").exists());
    assert!(!resumed_output_dir.join("manifest.json").exists());

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(feedback_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered flow-feedback sequence with 3 frame(s)",
        ));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            uninterrupted_arg.as_str(),
            "--carrier-amount",
            "8",
            "--feedback-amount",
            "12",
            "--feedback-mix",
            "0.7",
            "--decay",
            "0.95",
            "--max-frames",
            "3",
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            reset_arg.as_str(),
            "--carrier-amount",
            "8",
            "--feedback-amount",
            "12",
            "--feedback-mix",
            "0.7",
            "--decay",
            "0.95",
            "--max-frames",
            "3",
            "--reset-at-frame",
            "1",
        ])
        .assert()
        .success();

    let final_checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(resumed_output_dir.join("checkpoint.json"))
            .expect("read final checkpoint"),
    )
    .expect("parse final checkpoint");
    assert_eq!(final_checkpoint["status"], "complete");
    assert_eq!(final_checkpoint["next_frame_index"], 3);
    assert_eq!(final_checkpoint["contract"]["settings"]["iterations"], 1);
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(resumed_output_dir.join("manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "frame_sequence_flow_feedback");
    assert_eq!(manifest["frames"].as_array().expect("frames").len(), 3);
    assert_eq!(
        fs::read(resumed_output_dir.join("frames/frame_000002.png")).expect("resumed frame"),
        fs::read(uninterrupted_output_dir.join("frames/frame_000002.png"))
            .expect("uninterrupted frame")
    );
    assert_eq!(
        fs::read(reset_output_dir.join("frames/frame_000001.png")).expect("reset frame"),
        fs::read(reset_output_dir.join("frames/frame_000000.png")).expect("frame zero")
    );
    let reset_manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(reset_output_dir.join("manifest.json")).expect("read reset manifest"),
    )
    .expect("parse reset manifest");
    assert_eq!(reset_manifest["feedback_contract"]["reset_at_frame"], 1);
}

#[test]
fn feedback_flow_source_selects_recorded_algorithm() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");

    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        for dir in [&modulator_dir, &carrier_dir] {
            let frame_arg = dir.join(frame_name).to_string_lossy().to_string();
            Command::cargo_bin("morphogen")
                .expect("morphogen binary")
                .args(["render-test", frame_arg.as_str()])
                .assert()
                .success();
        }
    }

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();

    for (flow_source, expected_algorithm) in [
        (None, "pyramidal_lucas_kanade_cpu_v1"),
        (Some("luminance"), "luminance_gradient_cpu_v1"),
        (Some("optical-flow"), "pyramidal_lucas_kanade_cpu_v1"),
    ] {
        let output_dir = temp_dir
            .path()
            .join(format!("out-{}", flow_source.unwrap_or("default")));
        let output_arg = output_dir.to_string_lossy().to_string();
        let mut args = vec![
            "render-feedback-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--max-frames",
            "2",
        ];
        if let Some(flow_source) = flow_source {
            args.push("--flow-source");
            args.push(flow_source);
        }

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
            .success();

        let checkpoint: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(output_dir.join("checkpoint.json")).expect("read checkpoint"),
        )
        .expect("parse checkpoint");
        assert_eq!(
            checkpoint["contract"]["flow_algorithm"], expected_algorithm,
            "flow_source {flow_source:?} should record {expected_algorithm}"
        );
    }
}

#[test]
fn optical_flow_feedback_uses_validated_caches_and_zeroes_reset_frames() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let cache_dir = temp_dir.path().join("flow-cache");
    let first_output_dir = temp_dir.path().join("first-output");
    let cached_output_dir = temp_dir.path().join("cached-output");
    let reset_output_dir = temp_dir.path().join("reset-output");
    let stale_output_dir = temp_dir.path().join("stale-output");

    for (index, shift) in [0, 1, 2].into_iter().enumerate() {
        let frame_name = format!("frame_{index:06}.png");
        write_translated_texture(&modulator_dir.join(&frame_name), 24, 16, shift);
        write_horizontal_carrier(&carrier_dir.join(frame_name), 47, 16);
    }

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let cache_arg = cache_dir.to_string_lossy().to_string();
    let first_output_arg = first_output_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            first_output_arg.as_str(),
            "--flow-cache-dir",
            cache_arg.as_str(),
            "--carrier-amount",
            "1",
            "--feedback-amount",
            "0",
            "--feedback-mix",
            "0",
            "--decay",
            "1",
            "--max-frames",
            "3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "reused 0 and generated 2 temporal optical-flow cache frame(s)",
        ));

    let (manifest, flow) = read_flow_cache(cache_dir.join("frame_000001")).expect("flow cache");
    assert_eq!(manifest.algorithm, "pyramidal_lucas_kanade_cpu_v1");
    assert!(manifest.source_fingerprint.is_some());
    let vector = flow.vector(24, 8).expect("center flow vector");
    assert!(vector[0] < -1.2 && vector[0] > -2.8, "u was {}", vector[0]);

    let carrier = image::open(carrier_dir.join("frame_000001.png"))
        .expect("carrier image")
        .to_rgba8();
    let first_output = image::open(first_output_dir.join("frames/frame_000001.png"))
        .expect("first output image")
        .to_rgba8();
    assert!(
        first_output.get_pixel(24, 8)[0] < carrier.get_pixel(24, 8)[0],
        "backward flow should sample from the carrier's left side"
    );

    let cached_output_arg = cached_output_dir.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            cached_output_arg.as_str(),
            "--flow-cache-dir",
            cache_arg.as_str(),
            "--carrier-amount",
            "1",
            "--feedback-amount",
            "0",
            "--feedback-mix",
            "0",
            "--decay",
            "1",
            "--max-frames",
            "3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "reused 2 and generated 0 temporal optical-flow cache frame(s)",
        ));

    let reset_output_arg = reset_output_dir.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            reset_output_arg.as_str(),
            "--flow-cache-dir",
            cache_arg.as_str(),
            "--carrier-amount",
            "1",
            "--feedback-amount",
            "0",
            "--feedback-mix",
            "0",
            "--decay",
            "1",
            "--max-frames",
            "3",
            "--reset-at-frame",
            "1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "reused 1 and generated 0 temporal optical-flow cache frame(s)",
        ));

    let reset_output = image::open(reset_output_dir.join("frames/frame_000001.png"))
        .expect("reset output image")
        .to_rgba8();
    assert_eq!(reset_output, carrier);

    write_translated_texture(&modulator_dir.join("frame_000002.png"), 24, 16, 3);
    let stale_output_arg = stale_output_dir.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            stale_output_arg.as_str(),
            "--flow-cache-dir",
            cache_arg.as_str(),
            "--carrier-amount",
            "1",
            "--feedback-amount",
            "0",
            "--feedback-mix",
            "0",
            "--decay",
            "1",
            "--max-frames",
            "3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "reused 0 and generated 2 temporal optical-flow cache frame(s)",
        ));
}

fn write_translated_texture(path: &Path, width: u32, height: u32, shift_x: i32) {
    let parent = path.parent().expect("parent directory");
    fs::create_dir_all(parent).expect("create frame directory");
    let image = ImageBuffer::from_fn(width, height, |x, y| {
        let fx = x as f32 - shift_x as f32;
        let fy = y as f32;
        let value = 128.0
            + 45.0 * (0.55 * fx).sin()
            + 45.0 * (0.39 * fy).sin()
            + 25.0 * (0.31 * (fx + fy)).sin();
        let channel = value.round().clamp(0.0, 255.0) as u8;
        Rgba([channel, channel, channel, 255])
    });
    image.save(path).expect("save translated texture");
}

fn write_horizontal_carrier(path: &Path, width: u32, height: u32) {
    let parent = path.parent().expect("parent directory");
    fs::create_dir_all(parent).expect("create frame directory");
    let image = ImageBuffer::from_fn(width, height, |x, y| {
        let red = (x * 5).min(255) as u8;
        let green = (y * 11).min(255) as u8;
        Rgba([red, green, 0, 255])
    });
    image.save(path).expect("save carrier");
}

#[test]
fn cache_synthetic_flow_writes_manifest_and_frame() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let cache_dir = temp_dir.path().join("synthetic-flow-cache");
    let cache_arg = cache_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "cache-synthetic-flow",
            cache_arg.as_str(),
            "--width",
            "8",
            "--height",
            "4",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote synthetic flow cache"));

    assert!(cache_dir.join("manifest.json").exists());
    assert!(cache_dir.join("frame_000000.flowf32").exists());
}

#[test]
fn render_queue_commands_persist_jobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let queue_path = temp_dir.path().join("queue.json");
    let output_dir = temp_dir.path().join("queue-output");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_arg = output_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-init", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote empty render queue"));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-test",
            queue_arg.as_str(),
            "--project-path",
            "examples/projects/two_source_flow_displace.morphogen.json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("queued render job job-0001"));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-inspect", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains("render queue: 1 job(s)"))
        .stdout(predicate::str::contains("job-0001"));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-run-test",
            queue_arg.as_str(),
            output_arg.as_str(),
            "--stop-after-frame",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed queued test job job-0001",
        ));

    let job_output_dir = output_dir.join("job-0001");
    assert!(job_output_dir.join("checkpoint.json").exists());
    assert!(job_output_dir.join("frames/frame_000000.png").exists());
    assert!(!job_output_dir.join("audio/main.wav").exists());

    let checkpointed_queue_json =
        fs::read_to_string(&queue_path).expect("read checkpointed render queue");
    let checkpointed_queue: serde_json::Value =
        serde_json::from_str(&checkpointed_queue_json).expect("parse checkpointed render queue");
    assert_eq!(
        checkpointed_queue["jobs"][0]["output"]["frame_paths"],
        serde_json::json!(["frames/frame_000000.png"])
    );
    assert_eq!(
        checkpointed_queue["jobs"][0]["output"]["audio_stem_paths"],
        serde_json::json!([])
    );
    assert_eq!(
        checkpointed_queue["jobs"][0]["output"]["timing"]["frame_rate"],
        24.0
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-inspect", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "job-0001 task=test_render status=Running",
        ));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-test", queue_arg.as_str(), output_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered queued test job job-0001",
        ));

    assert!(job_output_dir.join("manifest.json").exists());
    assert!(job_output_dir.join("checkpoint.json").exists());
    assert!(job_output_dir.join("frames/frame_000000.png").exists());
    assert!(job_output_dir.join("audio/main.wav").exists());

    let manifest_json =
        fs::read_to_string(job_output_dir.join("manifest.json")).expect("read output manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_json).expect("parse output manifest");
    assert_eq!(manifest["timing"]["frame_rate"], 24.0);
    assert_eq!(manifest["timing"]["frame_count"], 1);
    assert_eq!(manifest["timing"]["sample_rate"], 48_000);
    assert_eq!(manifest["timing"]["audio_sample_count"], 48_000);

    let complete_queue_json = fs::read_to_string(&queue_path).expect("read completed render queue");
    let complete_queue: serde_json::Value =
        serde_json::from_str(&complete_queue_json).expect("parse completed render queue");
    assert_eq!(
        complete_queue["jobs"][0]["output"]["audio_stem_paths"],
        serde_json::json!(["audio/main.wav"])
    );
    assert_eq!(
        complete_queue["jobs"][0]["output"]["timing"]["frame_count"],
        1
    );
    assert_eq!(
        complete_queue["jobs"][0]["output"]["timing"]["audio_sample_count"],
        48_000
    );

    let reader =
        hound::WavReader::open(job_output_dir.join("audio/main.wav")).expect("open rendered stem");
    let spec = reader.spec();
    assert_eq!(spec.sample_format, hound::SampleFormat::Float);
    assert_eq!(spec.sample_rate, 48_000);
    assert_eq!(spec.channels, 2);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-inspect", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "job-0001 task=test_render status=Complete",
        ));
}

#[test]
fn frame_sequence_queue_job_persists_provenance_and_writes_bundle_output() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let queue_path = temp_dir.path().join("queue.json");
    let output_root = temp_dir.path().join("queue-output");

    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let modulator_arg = modulator_dir.join(frame_name).to_string_lossy().to_string();
        let carrier_arg = carrier_dir.join(frame_name).to_string_lossy().to_string();

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", modulator_arg.as_str()])
            .assert()
            .success();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", carrier_arg.as_str()])
            .assert()
            .success();
    }

    let queue_arg = queue_path.to_string_lossy().to_string();
    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_root.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-init", queue_arg.as_str()])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-frame-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--amount",
            "8",
            "--max-frames",
            "2",
            "--frame-rate",
            "12",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "queued frame-sequence render job job-0001",
        ));

    let queued_json = fs::read_to_string(&queue_path).expect("read queued render job");
    let queued: serde_json::Value = serde_json::from_str(&queued_json).expect("parse queue json");
    assert_eq!(
        queued["jobs"][0]["task"]["type"],
        "frame_sequence_flow_displace"
    );
    assert_eq!(
        queued["jobs"][0]["provenance"]["sources"][0]["role"],
        "modulator"
    );
    assert_eq!(
        queued["jobs"][0]["provenance"]["analysis_caches"][0]["kind"],
        "optical_flow"
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-frame-sequence", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered queued frame-sequence job job-0001",
        ));

    let bundle_dir = output_root.join("job-0001");
    assert!(bundle_dir.join("frames/frame_000000.png").exists());
    assert!(bundle_dir.join("frames/frame_000001.png").exists());
    assert!(bundle_dir
        .join("cache/flow/frame_000000/manifest.json")
        .exists());

    let manifest_json =
        fs::read_to_string(bundle_dir.join("manifest.json")).expect("read frame bundle manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_json).expect("parse frame bundle manifest");
    assert_eq!(manifest["task"], "frame_sequence_flow_displace");
    assert_eq!(manifest["timing"]["frame_rate"], 12.0);
    assert_eq!(manifest["timing"]["frame_count"], 2);
    assert_eq!(
        manifest["provenance"]["analysis_caches"][0]["producer"],
        "luminance_gradient_cpu_v1"
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-inspect", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "job-0001 task=frame_sequence_flow_displace status=Complete",
        ))
        .stdout(predicate::str::contains("sources=2 caches=1"));
}

#[test]
fn granular_mosaic_queue_job_persists_provenance_and_writes_bundle_output() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let queue_path = temp_dir.path().join("queue.json");
    let output_root = temp_dir.path().join("queue-output");
    let rms_cache = temp_dir.path().join("source-a-rms.json");
    let onset_cache = temp_dir.path().join("source-a-onsets.json");
    let stft_cache = temp_dir.path().join("source-a-stft.json");

    fs::write(
        &rms_cache,
        serde_json::json!({
            "cache_format": "rms_envelope_v1",
            "sample_rate": 8,
            "frame_size": 2,
            "hop_size": 1,
            "frames": [{
                "time_seconds": 0.0,
                "rms": 0.5,
                "spectral_centroid_hz": null
            }]
        })
        .to_string(),
    )
    .expect("write RMS cache");
    fs::write(
        &onset_cache,
        serde_json::json!({
            "cache_format": "onset_strength_v1",
            "source_cache_format": "stft_magnitude_v1",
            "sample_rate": 8,
            "hop_size": 1,
            "frames": [{ "index": 0, "time_seconds": 0.0, "strength": 1.0 }]
        })
        .to_string(),
    )
    .expect("write onset cache");
    fs::write(
        &stft_cache,
        serde_json::json!({
            "cache_format": "stft_magnitude_v1",
            "sample_rate": 8,
            "channels": 1,
            "channel_mix": "mean_channels",
            "fft_size": 8,
            "hop_size": 1,
            "window": "rectangular",
            "bin_count": 5,
            "frames": [{
                "index": 0,
                "time_seconds": 0.0,
                "magnitudes": [0.0, 0.0, 1.0, 0.0, 0.0]
            }]
        })
        .to_string(),
    )
    .expect("write STFT cache");

    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        for directory in [&modulator_dir, &carrier_dir] {
            let frame_arg = directory.join(frame_name).to_string_lossy().to_string();
            Command::cargo_bin("morphogen")
                .expect("morphogen binary")
                .args(["render-test", frame_arg.as_str()])
                .assert()
                .success();
        }
    }

    let queue_arg = queue_path.to_string_lossy().to_string();
    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_root.to_string_lossy().to_string();
    let rms_cache_arg = rms_cache.to_string_lossy().to_string();
    let onset_cache_arg = onset_cache.to_string_lossy().to_string();
    let stft_cache_arg = stft_cache.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-init", queue_arg.as_str()])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-granular-mosaic-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--grain-size",
            "24",
            "--variation",
            "0.4",
            "--seed",
            "42",
            "--rms-cache",
            rms_cache_arg.as_str(),
            "--onset-cache",
            onset_cache_arg.as_str(),
            "--stft-cache",
            stft_cache_arg.as_str(),
            "--rms-variation-scale",
            "0.6",
            "--onset-rearrangement-scale",
            "0.4",
            "--centroid-grain-size-scale",
            "8",
            "--max-frames",
            "2",
            "--frame-rate",
            "12",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "queued granular-mosaic render job job-0001",
        ));

    let queued: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queued granular job"))
            .expect("parse granular queue");
    assert_eq!(
        queued["jobs"][0]["task"]["type"],
        "frame_sequence_granular_mosaic"
    );
    assert_eq!(
        queued["jobs"][0]["provenance"]["analysis_caches"][0]["kind"],
        "grain_descriptors"
    );
    assert_eq!(
        queued["jobs"][0]["task"]["audio_modulation"]["rms_variation_scale"],
        0.6
    );
    assert_eq!(
        queued["jobs"][0]["provenance"]["analysis_caches"]
            .as_array()
            .map(Vec::len),
        Some(4)
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-granular-mosaic-sequence", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered queued granular-mosaic job job-0001",
        ));

    let bundle_dir = output_root.join("job-0001");
    assert!(bundle_dir.join("frames/frame_000000.png").exists());
    assert!(bundle_dir.join("frames/frame_000001.png").exists());
    assert!(bundle_dir
        .join("cache/grains/frame_000000/grain_descriptors.json")
        .exists());
    assert!(bundle_dir
        .join("cache/grains/frame_000000/grain_selection.json")
        .exists());

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(bundle_dir.join("manifest.json")).expect("read granular manifest"),
    )
    .expect("parse granular manifest");
    assert_eq!(manifest["task"], "frame_sequence_granular_mosaic");
    assert_eq!(manifest["timing"]["frame_rate"], 12.0);
    assert_eq!(manifest["timing"]["frame_count"], 2);
    assert_eq!(
        manifest["granular_mosaic"]["algorithm"],
        "luma_nearest_grain_cpu_v1"
    );
    assert_eq!(
        manifest["granular_mosaic"]["audio_modulation"]["centroid_grain_size_scale"],
        8.0
    );
    assert_eq!(
        manifest["provenance"]["analysis_caches"][0]["producer"],
        "luma_nearest_grain_cpu_v1"
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-inspect", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "job-0001 task=frame_sequence_granular_mosaic status=Complete",
        ));
}

#[test]
fn granular_mosaic_pool_queue_job_persists_provenance_and_writes_bundle_output() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let queue_path = temp_dir.path().join("queue.json");
    let output_root = temp_dir.path().join("queue-output");
    let modulator_rms = temp_dir.path().join("source-a-rms.json");
    let carrier_rms = temp_dir.path().join("source-b-rms.json");

    for (path, rms) in [(&modulator_rms, 0.5_f64), (&carrier_rms, 0.8)] {
        fs::write(
            path,
            serde_json::json!({
                "cache_format": "rms_envelope_v1",
                "sample_rate": 8,
                "frame_size": 2,
                "hop_size": 1,
                "frames": [{ "time_seconds": 0.0, "rms": rms, "spectral_centroid_hz": null }]
            })
            .to_string(),
        )
        .expect("write RMS cache");
    }

    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        for directory in [&modulator_dir, &carrier_dir] {
            let frame_arg = directory.join(frame_name).to_string_lossy().to_string();
            Command::cargo_bin("morphogen")
                .expect("morphogen binary")
                .args(["render-test", frame_arg.as_str()])
                .assert()
                .success();
        }
    }

    let queue_arg = queue_path.to_string_lossy().to_string();
    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_root.to_string_lossy().to_string();
    let modulator_rms_arg = modulator_rms.to_string_lossy().to_string();
    let carrier_rms_arg = carrier_rms.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-init", queue_arg.as_str()])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-granular-mosaic-pool-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--grain-size",
            "16",
            "--variation",
            "0",
            "--seed",
            "7",
            "--audio-weight",
            "1.0",
            "--texture-weight",
            "0.0625",
            "--modulator-rms-cache",
            modulator_rms_arg.as_str(),
            "--carrier-rms-cache",
            carrier_rms_arg.as_str(),
            "--pool-window",
            "2",
            "--anti-repeat-weight",
            "0.5",
            "--anti-repeat-cooldown",
            "3",
            "--coherence-weight",
            "0.25",
            "--coherence-reach",
            "5",
            "--spatial-coherence-weight",
            "0.125",
            "--max-frames",
            "2",
            "--frame-rate",
            "12",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "queued granular-mosaic pool render job job-0001",
        ));

    let queued: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queued pool job"))
            .expect("parse pool queue");
    assert_eq!(
        queued["jobs"][0]["task"]["type"],
        "frame_sequence_granular_mosaic_pool"
    );
    assert_eq!(queued["jobs"][0]["task"]["audio_weight"], 1.0);
    assert_eq!(queued["jobs"][0]["task"]["texture_weight"], 0.0625);
    // Pool-selection knobs added in the exposure sweep persist on the queued task.
    assert_eq!(queued["jobs"][0]["task"]["pool_window"], 2);
    assert_eq!(queued["jobs"][0]["task"]["anti_repeat_weight"], 0.5);
    assert_eq!(queued["jobs"][0]["task"]["anti_repeat_cooldown"], 3);
    assert_eq!(queued["jobs"][0]["task"]["coherence_weight"], 0.25);
    assert_eq!(queued["jobs"][0]["task"]["coherence_reach"], 5);
    assert_eq!(queued["jobs"][0]["task"]["spatial_coherence_weight"], 0.125);
    assert_eq!(
        queued["jobs"][0]["provenance"]["analysis_caches"][0]["kind"],
        "grain_descriptors"
    );
    assert_eq!(
        queued["jobs"][0]["provenance"]["analysis_caches"][0]["producer"],
        "pooled_av_nearest_grain_cpu_v2"
    );
    // grain pool descriptors + Source A RMS + Source B RMS.
    assert_eq!(
        queued["jobs"][0]["provenance"]["analysis_caches"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-granular-mosaic-pool-sequence", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered queued granular-mosaic pool job job-0001",
        ));

    let bundle_dir = output_root.join("job-0001");
    assert!(bundle_dir.join("frames/frame_000000.png").exists());
    assert!(bundle_dir.join("frames/frame_000001.png").exists());
    assert!(bundle_dir
        .join("cache/pool/grain_pool_descriptors.json")
        .exists());

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(bundle_dir.join("manifest.json")).expect("read pool manifest"),
    )
    .expect("parse pool manifest");
    assert_eq!(manifest["task"], "frame_sequence_granular_mosaic_pool");
    assert_eq!(manifest["timing"]["frame_count"], 2);
    assert_eq!(
        manifest["granular_mosaic_pool"]["algorithm"],
        "pooled_av_nearest_grain_cpu_v2"
    );
    assert_eq!(manifest["granular_mosaic_pool"]["audio_weight"], 1.0);
    assert_eq!(manifest["granular_mosaic_pool"]["texture_weight"], 0.0625);
    assert_eq!(manifest["granular_mosaic_pool"]["pool_window"], 2);
    assert_eq!(manifest["granular_mosaic_pool"]["anti_repeat_weight"], 0.5);
    assert_eq!(manifest["granular_mosaic_pool"]["anti_repeat_cooldown"], 3);
    assert_eq!(manifest["granular_mosaic_pool"]["coherence_weight"], 0.25);
    assert_eq!(manifest["granular_mosaic_pool"]["coherence_reach"], 5);
    assert_eq!(
        manifest["granular_mosaic_pool"]["spatial_coherence_weight"],
        0.125
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-inspect", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "job-0001 task=frame_sequence_granular_mosaic_pool status=Complete",
        ));
}

#[test]
fn video_vocoder_queue_job_persists_knobs_and_writes_bundle_output() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let queue_path = temp_dir.path().join("queue.json");
    let output_root = temp_dir.path().join("queue-output");

    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        for directory in [&modulator_dir, &carrier_dir] {
            let frame_arg = directory.join(frame_name).to_string_lossy().to_string();
            Command::cargo_bin("morphogen")
                .expect("morphogen binary")
                .args(["render-test", frame_arg.as_str()])
                .assert()
                .success();
        }
    }

    let queue_arg = queue_path.to_string_lossy().to_string();
    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-init", queue_arg.as_str()])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-video-vocoder-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--bands",
            "8",
            "--amount",
            "0.5",
            "--mode",
            "gain",
            "--max-frames",
            "2",
            "--frame-rate",
            "12",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "queued video-vocoder render job job-0001",
        ));

    let queued: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queued vocoder job"))
            .expect("parse vocoder queue");
    assert_eq!(
        queued["jobs"][0]["task"]["type"],
        "frame_sequence_video_vocoder"
    );
    assert_eq!(queued["jobs"][0]["task"]["bands"], 8);
    assert_eq!(queued["jobs"][0]["task"]["amount"], 0.5);
    assert_eq!(queued["jobs"][0]["task"]["mode"], "gain");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-video-vocoder-sequence", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered queued video-vocoder job job-0001",
        ));

    let bundle_dir = output_root.join("job-0001");
    assert!(bundle_dir.join("frames/frame_000000.png").exists());
    assert!(bundle_dir.join("frames/frame_000001.png").exists());

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(bundle_dir.join("manifest.json")).expect("read vocoder manifest"),
    )
    .expect("parse vocoder manifest");
    assert_eq!(manifest["task"], "frame_sequence_video_vocoder");
    assert_eq!(manifest["timing"]["frame_count"], 2);
    assert_eq!(
        manifest["video_vocoder"]["algorithm"],
        "luma_band_gain_vocoder_cpu_v1"
    );
    assert_eq!(manifest["video_vocoder"]["mode"], "gain");
    assert_eq!(manifest["video_vocoder"]["bands"], 8);
    assert_eq!(manifest["video_vocoder"]["amount"], 0.5);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-inspect", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "job-0001 task=frame_sequence_video_vocoder status=Complete",
        ));
}

#[test]
fn feedback_queue_job_persists_parameters_and_writes_resumable_bundle() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let queue_path = temp_dir.path().join("queue.json");
    let output_root = temp_dir.path().join("queue-output");

    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let modulator_arg = modulator_dir.join(frame_name).to_string_lossy().to_string();
        let carrier_arg = carrier_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", modulator_arg.as_str()])
            .assert()
            .success();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", carrier_arg.as_str()])
            .assert()
            .success();
    }

    let queue_arg = queue_path.to_string_lossy().to_string();
    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-init", queue_arg.as_str()])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-feedback-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--carrier-amount",
            "8",
            "--feedback-amount",
            "12",
            "--feedback-mix",
            "0.7",
            "--decay",
            "0.95",
            "--structure-mix",
            "0.6",
            "--max-frames",
            "2",
            "--output-bit-depth",
            "16",
            "--temporal-supersampling",
            "2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "queued flow-feedback render job job-0001",
        ));

    let queued: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queued feedback job"))
            .expect("parse queue json");
    assert_eq!(
        queued["jobs"][0]["task"]["type"],
        "frame_sequence_flow_feedback"
    );
    assert_eq!(queued["jobs"][0]["task"]["feedback_mix"], 0.7);
    assert_eq!(queued["jobs"][0]["task"]["structure_mix"], 0.6);
    assert_eq!(queued["jobs"][0]["settings"]["temporal_supersampling"], 2);
    assert_eq!(
        queued["jobs"][0]["settings"]["export_format"]["bit_depth"],
        16
    );
    assert_eq!(
        queued["jobs"][0]["provenance"]["analysis_caches"][0]["producer"],
        "pyramidal_lucas_kanade_cpu_v1"
    );
    let mut legacy_provenance_queue = queued;
    legacy_provenance_queue["jobs"][0]["provenance"]["analysis_caches"][0]["producer"] =
        serde_json::Value::String("lucas_kanade_cpu_v1".to_string());
    fs::write(
        &queue_path,
        serde_json::to_string_pretty(&legacy_provenance_queue).expect("serialize legacy queue"),
    )
    .expect("write legacy queue");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-feedback-sequence", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered queued flow-feedback job job-0001",
        ));

    let bundle_dir = output_root.join("job-0001");
    assert!(bundle_dir.join("frames/frame_000000.png").exists());
    assert!(bundle_dir.join("frames/frame_000001.png").exists());
    assert!(bundle_dir
        .join("state/feedback_frame_000001.rgba32f")
        .exists());
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(bundle_dir.join("manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "frame_sequence_flow_feedback");
    assert_eq!(manifest["export"]["format"], "png");
    assert_eq!(manifest["export"]["bit_depth"], 16);
    assert_eq!(manifest["export"]["temporal_supersampling"], 2);
    assert_eq!(manifest["feedback_contract"]["output_bit_depth"], 16);
    assert_eq!(manifest["feedback_contract"]["temporal_supersampling"], 2);
    let output_color = image::ImageReader::open(bundle_dir.join("frames/frame_000001.png"))
        .expect("open 16-bit output")
        .decode()
        .expect("decode 16-bit output")
        .color();
    assert_eq!(output_color, image::ColorType::Rgba16);
    let decay = manifest["feedback_contract"]["settings"]["decay"]
        .as_f64()
        .expect("feedback decay");
    assert!((decay - 0.95).abs() < 0.000_001);
    assert_eq!(
        manifest["provenance"]["analysis_caches"][0]["producer"],
        "pyramidal_lucas_kanade_cpu_v1"
    );

    let completed_queue: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read completed queue"))
            .expect("parse completed queue");
    assert_eq!(
        completed_queue["jobs"][0]["provenance"]["analysis_caches"][0]["producer"],
        "pyramidal_lucas_kanade_cpu_v1"
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-inspect", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "job-0001 task=frame_sequence_flow_feedback status=Complete",
        ));
}

#[test]
fn extraction_commands_are_available_without_running_ffmpeg() {
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["extract-frames", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--fps"));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["extract-audio", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--sample-rate"))
        .stdout(predicate::str::contains("--max-duration-seconds"));
}

#[test]
fn extraction_commands_validate_numeric_arguments_before_ffmpeg() {
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["extract-frames", "input.mov", "frames", "--fps", "0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "fps must be a positive finite number",
        ));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "extract-audio",
            "input.mov",
            "out.wav",
            "--sample-rate",
            "0",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "sample-rate must be greater than zero",
        ));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "extract-audio",
            "input.mov",
            "out.wav",
            "--max-duration-seconds",
            "0",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "max-duration-seconds must be a positive finite number",
        ));
}

#[test]
fn export_audio_stem_writes_float_wav_with_gain() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let input_wav = temp_dir.path().join("input.wav");
    let output_wav = temp_dir.path().join("output.wav");
    write_test_wav(&input_wav, &[0.25, -0.5]);
    let input_arg = input_wav.to_string_lossy().to_string();
    let output_arg = output_wav.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "export-audio-stem",
            input_arg.as_str(),
            output_arg.as_str(),
            "--gain",
            "2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("exported WAV stem"));

    let mut reader = hound::WavReader::open(output_wav).expect("open output wav");
    let spec = reader.spec();
    let samples: Vec<f32> = reader
        .samples::<f32>()
        .map(|sample| sample.expect("read sample"))
        .collect();

    assert_eq!(spec.sample_format, hound::SampleFormat::Float);
    assert_eq!(spec.bits_per_sample, 32);
    assert_eq!(samples, vec![0.5, -1.0]);
}

#[test]
fn cache_stft_writes_json_sidecar_from_wav() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let input_wav = temp_dir.path().join("input.wav");
    let output_json = temp_dir.path().join("cache/stft.json");
    write_test_wav(&input_wav, &[1.0, 0.0, -1.0, 0.0]);
    let input_arg = input_wav.to_string_lossy().to_string();
    let output_arg = output_json.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "cache-stft",
            input_arg.as_str(),
            output_arg.as_str(),
            "--fft-size",
            "4",
            "--hop-size",
            "2",
            "--window",
            "rectangular",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote STFT cache"));

    let json = fs::read_to_string(output_json).expect("read STFT cache");
    assert!(json.contains("\"cache_format\": \"stft_magnitude_v1\""));
    assert!(json.contains("\"bin_count\": 3"));
    assert!(json.contains("\"magnitudes\""));
}

#[test]
fn cache_onsets_writes_json_sidecar_from_wav() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let input_wav = temp_dir.path().join("input.wav");
    let output_json = temp_dir.path().join("cache/onsets.json");
    write_test_wav(&input_wav, &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0]);
    let input_arg = input_wav.to_string_lossy().to_string();
    let output_arg = output_json.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "cache-onsets",
            input_arg.as_str(),
            output_arg.as_str(),
            "--fft-size",
            "4",
            "--hop-size",
            "4",
            "--window",
            "rectangular",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote onset-strength cache"));

    let json = fs::read_to_string(output_json).expect("read onset cache");
    assert!(json.contains("\"cache_format\": \"onset_strength_v1\""));
    assert!(json.contains("\"strength\""));
}

#[test]
fn cache_rms_writes_json_sidecar_from_wav() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let input_wav = temp_dir.path().join("input.wav");
    let output_json = temp_dir.path().join("cache/rms.json");
    write_test_wav(&input_wav, &[0.0, 1.0, 0.0, -1.0]);
    let input_arg = input_wav.to_string_lossy().to_string();
    let output_arg = output_json.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "cache-rms",
            input_arg.as_str(),
            output_arg.as_str(),
            "--window-size",
            "2",
            "--hop-size",
            "2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote RMS envelope cache"));

    let json = fs::read_to_string(output_json).expect("read RMS cache");
    assert!(json.contains("\"cache_format\": \"rms_envelope_v1\""));
    assert!(json.contains("\"sample_rate\": 4"));
    assert!(json.contains("\"frames\""));
}

#[test]
fn project_register_proxy_persists_proxy_and_analysis_cache_references() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let project_path = temp_dir.path().join("project.morphogen.json");
    let project_arg = project_path.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["init-example", project_arg.as_str()])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "project-register-proxy",
            project_arg.as_str(),
            "--source-role",
            "modulator",
            "--frame-dir",
            "/tmp/proxy/source-a/frames",
            "--audio",
            "/tmp/proxy/source-a/audio.wav",
            "--analysis-cache",
            "audio_rms=/tmp/proxy/source-a/analysis/rms.json",
            "--analysis-cache",
            "stft=/tmp/proxy/source-a/analysis/stft.json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "registered proxy for source 'source-a'",
        ));

    let project_json = fs::read_to_string(project_path).expect("read project");
    let project: serde_json::Value = serde_json::from_str(&project_json).expect("parse project");
    assert_eq!(
        project["sources"][0]["proxy"]["frame_directory"],
        "/tmp/proxy/source-a/frames"
    );
    assert_eq!(
        project["sources"][0]["proxy"]["audio_path"],
        "/tmp/proxy/source-a/audio.wav"
    );
    assert!(project["cache_manifest"]["entries"]
        .as_array()
        .expect("cache entries")
        .iter()
        .any(|entry| entry["kind"] == "audio_rms"));
    assert!(project["cache_manifest"]["entries"]
        .as_array()
        .expect("cache entries")
        .iter()
        .any(|entry| entry["kind"] == "stft"));
}

#[test]
fn queue_cancel_marks_a_queued_job_as_cancelled() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-init", queue_arg.as_str()])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-add-test", queue_arg.as_str()])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-cancel", queue_arg.as_str(), "job-0001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cancelled job job-0001"));

    let queue_json = fs::read_to_string(queue_path).expect("read queue");
    let queue: serde_json::Value = serde_json::from_str(&queue_json).expect("parse queue");
    assert_eq!(queue["jobs"][0]["status"], "cancelled");
}

#[test]
fn failed_frame_sequence_job_records_a_durable_failure() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let queue_path = temp_dir.path().join("queue.json");
    let output_root = temp_dir.path().join("output");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_arg = output_root.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-init", queue_arg.as_str()])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-frame-sequence",
            queue_arg.as_str(),
            "/tmp/does-not-exist/modulator",
            "/tmp/does-not-exist/carrier",
            output_arg.as_str(),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-frame-sequence", queue_arg.as_str()])
        .assert()
        .failure();

    let queue_json = fs::read_to_string(queue_path).expect("read queue");
    let queue: serde_json::Value = serde_json::from_str(&queue_json).expect("parse queue");
    assert_eq!(queue["jobs"][0]["status"], "failed");
    assert!(queue["jobs"][0]["failure"]["message"]
        .as_str()
        .expect("failure message")
        .contains("No such file"));
}

#[test]
fn queue_spectral_cross_synth_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_wav = temp_dir.path().join("modulator.wav");
    let carrier_wav = temp_dir.path().join("carrier.wav");
    // A silent->loud envelope over a steady carrier (gain mode, small buffers).
    write_test_wav(&modulator_wav, &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
    write_test_wav(&carrier_wav, &[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);

    let modulator_arg = modulator_wav.to_string_lossy().to_string();
    let carrier_arg = carrier_wav.to_string_lossy().to_string();
    let direct_wav = temp_dir.path().join("direct.wav");
    let direct_arg = direct_wav.to_string_lossy().to_string();
    let common = [
        "--mode", "gain", "--amount", "1", "--rms-window", "4", "--rms-hop", "4",
    ];

    // Direct render.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-spectral-cross-synth",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
        ])
        .args(common)
        .assert()
        .success();

    // Queue add + run.
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-spectral-cross-synth",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
        ])
        .args(common)
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-spectral-cross-synth", queue_arg.as_str()])
        .assert()
        .success();

    // Queue render is byte-identical to the direct render (path-independent).
    let queued_wav = output_root.join("job-0001/audio/cross_synth.wav");
    assert_eq!(
        fs::read(&queued_wav).expect("read queued wav"),
        fs::read(&direct_wav).expect("read direct wav"),
        "queue render must be byte-identical to the direct render"
    );

    // Manifest records the algorithm + knobs.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "audio_spectral_cross_synth");
    assert_eq!(manifest["audio_stems"][0], "audio/cross_synth.wav");
    let knobs = &manifest["spectral_cross_synth"];
    assert_eq!(knobs["algorithm"], "rms_gain_cross_synth_cpu_v1");
    assert_eq!(knobs["mode"], "gain");
    assert_eq!(knobs["amount"], 1.0);
    assert_eq!(knobs["rms_window"], 4);
    assert_eq!(knobs["rms_hop"], 4);
}

#[test]
fn queue_audio_video_route_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_wav = temp_dir.path().join("modulator.wav");
    // A loud modulator ⇒ full normalized gain ⇒ a non-trivial displacement.
    write_test_wav(&modulator_wav, &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);

    // Two carrier frames (render-test writes a 256x256 displaced gradient PNG).
    let carrier_dir = temp_dir.path().join("carrier-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let frame_arg = carrier_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }

    let modulator_arg = modulator_wav.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let direct_dir = temp_dir.path().join("direct");
    let direct_arg = direct_dir.to_string_lossy().to_string();

    // Direct render.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-audio-video-route-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
            "--amount",
            "1",
            "--shift-x",
            "8",
            "--rms-window",
            "4",
            "--rms-hop",
            "4",
            "--fps",
            "1",
        ])
        .assert()
        .success();

    // Queue add + run (the queue uses --frame-rate where the direct uses --fps).
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-audio-video-route-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--amount",
            "1",
            "--shift-x",
            "8",
            "--rms-window",
            "4",
            "--rms-hop",
            "4",
            "--frame-rate",
            "1",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-audio-video-route-sequence", queue_arg.as_str()])
        .assert()
        .success();

    // Every queued frame is byte-identical to the direct render (path-independent).
    for frame in ["frame_000000.png", "frame_000001.png"] {
        let queued = output_root.join("job-0001/frames").join(frame);
        let direct = direct_dir.join(frame);
        assert_eq!(
            fs::read(&queued).expect("read queued frame"),
            fs::read(&direct).expect("read direct frame"),
            "queue render must be byte-identical to the direct render ({frame})"
        );
    }

    // Manifest records the routing algorithm + knobs.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "frame_sequence_audio_video_route");
    let knobs = &manifest["audio_video_route"];
    assert_eq!(knobs["algorithm"], "rms_displacement_route_cpu_v1");
    assert_eq!(knobs["amount"], 1.0);
    assert_eq!(knobs["shift_x"], 8.0);
    assert_eq!(knobs["shift_y"], 0.0);
    assert_eq!(knobs["rms_window"], 4);
    assert_eq!(knobs["rms_hop"], 4);
}

#[test]
fn queue_convolution_blend_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    // Two modulator frames + two carrier frames (render-test writes a 256x256
    // gradient PNG; the same content makes the kernel + carrier well-defined).
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    for dir in [&modulator_dir, &carrier_dir] {
        for frame_name in ["frame_000001.png", "frame_000002.png"] {
            let frame_arg = dir.join(frame_name).to_string_lossy().to_string();
            Command::cargo_bin("morphogen")
                .expect("morphogen binary")
                .args(["render-test", frame_arg.as_str()])
                .assert()
                .success();
        }
    }

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let direct_dir = temp_dir.path().join("direct");
    let direct_arg = direct_dir.to_string_lossy().to_string();

    // Direct render.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-convolutional-blend-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
            "--kernel-size",
            "5",
            "--amount",
            "1",
        ])
        .assert()
        .success();

    // Queue add + run.
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-convolutional-blend-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--kernel-size",
            "5",
            "--amount",
            "1",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-convolutional-blend-sequence", queue_arg.as_str()])
        .assert()
        .success();

    // Every queued frame is byte-identical to the direct render (path-independent).
    for frame in ["frame_000000.png", "frame_000001.png"] {
        let queued = output_root.join("job-0001/frames").join(frame);
        let direct = direct_dir.join(frame);
        assert_eq!(
            fs::read(&queued).expect("read queued frame"),
            fs::read(&direct).expect("read direct frame"),
            "queue render must be byte-identical to the direct render ({frame})"
        );
    }

    // Manifest records the convolution algorithm + knobs.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "frame_sequence_convolution_blend");
    let knobs = &manifest["convolution_blend"];
    assert_eq!(knobs["algorithm"], "image_kernel_convolution_blend_cpu_v1");
    assert_eq!(knobs["kernel_size"], 5);
    assert_eq!(knobs["amount"], 1.0);
    assert_eq!(knobs["backend"], "CPU");
}

fn write_test_wav(path: &Path, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 4,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
    for sample in samples {
        writer.write_sample(*sample).expect("write sample");
    }
    writer.finalize().expect("finalize wav");
}
