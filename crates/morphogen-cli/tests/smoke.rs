use std::{fs, path::Path};

use assert_cmd::Command;
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
        .stdout(predicate::str::contains("job-0001 status=Running"));

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
        .stdout(predicate::str::contains("job-0001 status=Complete"));
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
        .stdout(predicate::str::contains("--sample-rate"));
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
