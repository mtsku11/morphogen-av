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
fn datamosh_bitstream_help_lists_keyframe_removal_operation() {
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["datamosh-bitstream", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--operation"))
        .stdout(predicate::str::contains("remove-keyframe"));
}

#[test]
fn render_rutt_etra_sequence_writes_frames_and_manifest_with_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    let output_dir = temp_dir.path().join("rutt-etra-frames");
    fs::create_dir_all(&source_dir).expect("create source frames");

    for index in 0..2u32 {
        let frame = ImageBuffer::from_fn(16, 16, |x, _| {
            let value = (x as u8).wrapping_mul(16).wrapping_add(index as u8);
            Rgba([value, value, value, u8::MAX])
        });
        frame
            .save(source_dir.join(format!("frame_{index:06}.png")))
            .expect("write source frame");
    }

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_dir.to_string_lossy().as_ref(),
            output_dir.to_string_lossy().as_ref(),
            "--frames",
            "2",
            "--line-pitch",
            "4",
            "--displacement-depth",
            "12.5",
            "--line-thickness",
            "2",
            "--mono",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered rutt-etra scanline sequence with 2 frame(s)",
        ));

    for index in 0..2 {
        assert!(
            output_dir.join(format!("frame_{index:06}.png")).exists(),
            "frame {index} must be written"
        );
    }

    let manifest = read_json(&output_dir.join("manifest.json"));
    assert_eq!(manifest["algorithm"], "rutt_etra_scanline_cpu_v1");
    assert_eq!(manifest["line_pitch"], 4);
    assert_eq!(manifest["displacement_depth"], 12.5);
    assert_eq!(manifest["line_thickness"], 2);
    assert_eq!(manifest["mono"], true);
    assert_eq!(manifest["frame_count"], 2);
}

/// Pin (Tier 5.4 S1): without `--matte`, the manifest carries no `matte` block —
/// pre-slice manifests stay byte-identical.
#[test]
fn render_rutt_etra_sequence_no_matte_manifest_has_no_matte_block() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    let output_dir = temp_dir.path().join("out");
    write_solid_frames(&source_dir, 2, [200, 100, 50, 255]);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_dir.to_string_lossy().as_ref(),
            output_dir.to_string_lossy().as_ref(),
            "--frames",
            "2",
            "--displacement-depth",
            "20",
        ])
        .assert()
        .success();

    let manifest = read_json(&output_dir.join("manifest.json"));
    assert!(
        manifest.get("matte").is_none(),
        "no --matte ⇒ no matte block in manifest"
    );
}

fn png_color_type(path: &Path) -> image::ColorType {
    image::open(path).expect("open frame").color()
}

/// Tier 5.6 S2 representative shape #1: an effect (rutt-etra) whose manifest
/// is written unconditionally. Off case: omitting `--output-bit-depth` (⇒ 8)
/// is byte-identical to passing `--output-bit-depth 8` explicitly, and the
/// manifest carries no `output_bit_depth` key (skip-when-default). On case:
/// `--output-bit-depth 16` gains the manifest key and writes 16-bit PNGs.
#[test]
fn render_rutt_etra_sequence_output_bit_depth_off_case_pin_and_16_bit_on_case() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_solid_frames(&source_dir, 2, [200, 100, 50, 255]);

    let run = |name: &str, extra_args: &[&str]| -> std::path::PathBuf {
        let output_dir = temp_dir.path().join(name);
        let mut args: Vec<String> = vec![
            "render-rutt-etra-sequence".to_string(),
            source_dir.to_string_lossy().to_string(),
            output_dir.to_string_lossy().to_string(),
            "--frames".to_string(),
            "2".to_string(),
            "--displacement-depth".to_string(),
            "20".to_string(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
            .success();
        output_dir
    };

    let implicit_dir = run("implicit", &[]);
    let explicit_8_dir = run("explicit-8", &["--output-bit-depth", "8"]);
    let explicit_16_dir = run("explicit-16", &["--output-bit-depth", "16"]);

    assert_png_frames_identical(&implicit_dir, &explicit_8_dir, 2);
    assert_eq!(
        fs::read(implicit_dir.join("manifest.json")).expect("implicit manifest"),
        fs::read(explicit_8_dir.join("manifest.json")).expect("explicit-8 manifest"),
        "omitting --output-bit-depth must be byte-identical to passing 8 explicitly"
    );
    let manifest_8 = read_json(&implicit_dir.join("manifest.json"));
    assert!(
        manifest_8.get("output_bit_depth").is_none(),
        "depth 8 (the default) must not add an output_bit_depth key to the manifest"
    );
    assert_eq!(
        png_color_type(&implicit_dir.join("frame_000000.png")),
        image::ColorType::Rgba8
    );

    let manifest_16 = read_json(&explicit_16_dir.join("manifest.json"));
    assert_eq!(manifest_16["output_bit_depth"], 16);
    assert_eq!(
        png_color_type(&explicit_16_dir.join("frame_000000.png")),
        image::ColorType::Rgba16
    );
}

/// Tier 5.6 S2 representative shape #2: an effect (channel-shift) whose
/// manifest is written *conditionally* — only when a matte is set OR the
/// output bit depth is 16 (pre-slice: only on matte). Without `--matte`, the
/// off case (depth 8) must write no manifest at all, and the on case (depth
/// 16) must write one carrying just the `output_bit_depth` key.
#[test]
fn render_channel_shift_sequence_output_bit_depth_off_case_pin_and_16_bit_on_case() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_b_dir = temp_dir.path().join("source-b");
    write_solid_frames(&source_b_dir, 2, [200, 100, 50, 255]);

    let run = |name: &str, extra_args: &[&str]| -> std::path::PathBuf {
        let output_dir = temp_dir.path().join(name);
        let mut args: Vec<String> = vec![
            "render-channel-shift-sequence".to_string(),
            source_b_dir.to_string_lossy().to_string(),
            output_dir.to_string_lossy().to_string(),
            "--frames".to_string(),
            "2".to_string(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
            .success();
        output_dir
    };

    let implicit_dir = run("implicit", &[]);
    let explicit_8_dir = run("explicit-8", &["--output-bit-depth", "8"]);
    let explicit_16_dir = run("explicit-16", &["--output-bit-depth", "16"]);

    assert_png_frames_identical(&implicit_dir, &explicit_8_dir, 2);
    assert!(
        !implicit_dir.join("manifest.json").exists(),
        "depth 8 with no --matte must write no manifest at all (pre-slice shape)"
    );
    assert!(!explicit_8_dir.join("manifest.json").exists());
    assert_eq!(
        png_color_type(&implicit_dir.join("frame_000000.png")),
        image::ColorType::Rgba8
    );

    let manifest_16 = read_json(&explicit_16_dir.join("manifest.json"));
    assert_eq!(manifest_16["output_bit_depth"], 16);
    assert_eq!(
        png_color_type(&explicit_16_dir.join("frame_000000.png")),
        image::ColorType::Rgba16
    );
}

/// Tier 5.6 S2 representative shape #3: an effect (pixel-sort) that writes NO
/// manifest at all (stdout-only convention) — just the flag, no new manifest.
#[test]
fn render_pixel_sort_sequence_output_bit_depth_off_case_pin_and_16_bit_on_case() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_a_dir = temp_dir.path().join("source-a");
    let source_b_dir = temp_dir.path().join("source-b");
    write_solid_frames(&source_a_dir, 2, [10, 20, 30, 255]);
    write_solid_frames(&source_b_dir, 2, [200, 100, 50, 255]);

    let run = |name: &str, extra_args: &[&str]| -> std::path::PathBuf {
        let output_dir = temp_dir.path().join(name);
        let mut args: Vec<String> = vec![
            "render-pixel-sort-sequence".to_string(),
            source_a_dir.to_string_lossy().to_string(),
            source_b_dir.to_string_lossy().to_string(),
            output_dir.to_string_lossy().to_string(),
            "--frames".to_string(),
            "2".to_string(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
            .success();
        output_dir
    };

    let implicit_dir = run("implicit", &[]);
    let explicit_8_dir = run("explicit-8", &["--output-bit-depth", "8"]);
    let explicit_16_dir = run("explicit-16", &["--output-bit-depth", "16"]);

    assert_png_frames_identical(&implicit_dir, &explicit_8_dir, 2);
    assert!(
        !implicit_dir.join("manifest.json").exists(),
        "pixel-sort writes no manifest regardless of bit depth"
    );
    assert_eq!(
        png_color_type(&implicit_dir.join("frame_000000.png")),
        image::ColorType::Rgba8
    );
    assert_eq!(
        png_color_type(&explicit_16_dir.join("frame_000000.png")),
        image::ColorType::Rgba16
    );
}

/// Anchor 1 (matte-1 identity): an all-white `a-luma` matte (gain 1) reproduces
/// the render without `--matte`, byte-for-byte.
#[test]
fn render_rutt_etra_sequence_matte_all_white_is_identity_to_no_matte() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    let matte_dir = temp_dir.path().join("matte-frames");
    let plain_dir = temp_dir.path().join("plain-out");
    let matted_dir = temp_dir.path().join("matted-out");
    write_solid_frames(&source_dir, 3, [180, 90, 30, 255]);
    write_solid_frames(&matte_dir, 3, [255, 255, 255, 255]);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_dir.to_string_lossy().as_ref(),
            plain_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--displacement-depth",
            "20",
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_dir.to_string_lossy().as_ref(),
            matted_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--displacement-depth",
            "20",
            "--matte",
            "a-luma",
            "--matte-frames",
            matte_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    for index in 0..3 {
        let file_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(plain_dir.join(&file_name)).expect("plain frame"),
            fs::read(matted_dir.join(&file_name)).expect("matted frame"),
            "matte-1 identity: all-white matte must reproduce no-matte output ({file_name})"
        );
    }
}

/// Anchor 2 (matte-0 identity): an all-black `a-luma` matte reproduces the
/// plain carrier frames verbatim (pure passthrough), byte-for-byte.
#[test]
fn render_rutt_etra_sequence_matte_all_black_is_plain_carrier() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    let matte_dir = temp_dir.path().join("matte-frames");
    let matted_dir = temp_dir.path().join("matted-out");
    write_solid_frames(&source_dir, 3, [180, 90, 30, 255]);
    write_solid_frames(&matte_dir, 3, [0, 0, 0, 255]);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_dir.to_string_lossy().as_ref(),
            matted_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--displacement-depth",
            "20",
            "--matte",
            "a-luma",
            "--matte-frames",
            matte_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    let source_frames = {
        let mut frames: Vec<_> = fs::read_dir(&source_dir)
            .expect("read source dir")
            .map(|entry| entry.expect("entry").path())
            .collect();
        frames.sort();
        frames
    };
    for (index, source_frame) in source_frames.iter().enumerate().take(3) {
        let file_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(source_frame).expect("source frame"),
            fs::read(matted_dir.join(&file_name)).expect("matted frame"),
            "matte-0 identity: all-black matte must reproduce the plain carrier ({file_name})"
        );
    }
}

/// `--matte-frames`/`--matte-gain` without `--matte` is a dead-flag user error.
#[test]
fn render_rutt_etra_sequence_matte_frames_without_matte_flag_errors() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    let matte_dir = temp_dir.path().join("matte-frames");
    let output_dir = temp_dir.path().join("out");
    write_solid_frames(&source_dir, 2, [100, 100, 100, 255]);
    write_solid_frames(&matte_dir, 2, [255, 255, 255, 255]);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_dir.to_string_lossy().as_ref(),
            output_dir.to_string_lossy().as_ref(),
            "--frames",
            "2",
            "--matte-frames",
            matte_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("require --matte"));
}

/// Matte frames shorter than the rendered range is a clear error (not a silent
/// truncation — the modulator-media convention differs here on purpose).
#[test]
fn render_rutt_etra_sequence_matte_frames_too_short_errors() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    let matte_dir = temp_dir.path().join("matte-frames");
    let output_dir = temp_dir.path().join("out");
    write_solid_frames(&source_dir, 4, [100, 100, 100, 255]);
    write_solid_frames(&matte_dir, 2, [255, 255, 255, 255]);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_dir.to_string_lossy().as_ref(),
            output_dir.to_string_lossy().as_ref(),
            "--frames",
            "4",
            "--matte",
            "a-luma",
            "--matte-frames",
            matte_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("do not cover the rendered range"));
}

/// `generate-frames` writes a manifest with every knob and PNG frames; a generated
/// dir is "just a frame dir" — feeding it straight into `render-rutt-etra-sequence`
/// must render without complaint (the oscillator bank's engine-integration anchor).
#[test]
fn generate_frames_writes_manifest_and_feeds_rutt_etra_sequence() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let generated_dir = temp_dir.path().join("scan-bars-frames");
    let rutt_etra_dir = temp_dir.path().join("rutt-etra-from-generated");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "scan-bars",
            generated_dir.to_string_lossy().as_ref(),
            "--width",
            "16",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.25",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("rendered 3 frame(s)"));

    for index in 0..3 {
        assert!(
            generated_dir.join(format!("frame_{index:06}.png")).exists(),
            "generated frame {index} must be written"
        );
    }

    let manifest = read_json(&generated_dir.join("manifest.json"));
    assert_eq!(manifest["algorithm"], "oscillator_scan_bars_cpu_v1");
    assert_eq!(manifest["preset"], "scan_bars");
    assert_eq!(manifest["width"], 16);
    assert_eq!(manifest["height"], 16);
    assert_eq!(manifest["frames"], 3);
    assert_eq!(manifest["rate"], 0.25);
    assert_eq!(manifest["phase"], 0.0);
    assert_eq!(manifest["scale"], 4.0);
    assert_eq!(manifest["seed"], 71);

    // "just a frame dir": feed it straight into an existing effect.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            generated_dir.to_string_lossy().as_ref(),
            rutt_etra_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered rutt-etra scanline sequence with 3 frame(s)",
        ));

    for index in 0..3 {
        let path = rutt_etra_dir.join(format!("frame_{index:06}.png"));
        assert!(path.exists(), "rutt-etra output frame {index} must exist");
        let image = image::open(&path).expect("decode rutt-etra output frame");
        assert_eq!((image.width(), image.height()), (16, 16));
    }
}

/// Two-source A→B: `--source-a-dir` pointing at the same dir as B is
/// byte-identical to the single-source render (the continuity identity);
/// a distinct A switches the algorithm id, records `source_a`, and diverges.
#[test]
fn render_rutt_etra_two_source_matches_single_when_a_equals_b_and_diverges_otherwise() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let b_dir = temp_dir.path().join("source-b");
    let a_dir = temp_dir.path().join("source-a");
    let single_dir = temp_dir.path().join("single");
    let two_ab_dir = temp_dir.path().join("two-ab");
    let two_distinct_dir = temp_dir.path().join("two-distinct");
    fs::create_dir_all(&b_dir).expect("create b frames");
    fs::create_dir_all(&a_dir).expect("create a frames");

    for index in 0..2u32 {
        // B: horizontal luma ramp (colour source).
        let b = ImageBuffer::from_fn(16, 16, |x, _| {
            let value = (x as u8).wrapping_mul(16).wrapping_add(index as u8);
            Rgba([value, value, value, u8::MAX])
        });
        b.save(b_dir.join(format!("frame_{index:06}.png")))
            .expect("write b frame");
        // A: vertical luma ramp — distinct structure, so displacement differs.
        let a = ImageBuffer::from_fn(16, 16, |_, y| {
            let value = (y as u8).wrapping_mul(16);
            Rgba([value, value, value, u8::MAX])
        });
        a.save(a_dir.join(format!("frame_{index:06}.png")))
            .expect("write a frame");
    }

    let common = [
        "--frames",
        "2",
        "--line-pitch",
        "4",
        "--displacement-depth",
        "24.0",
    ];

    // single-source on B
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(
            [
                "render-rutt-etra-sequence",
                b_dir.to_string_lossy().as_ref(),
                single_dir.to_string_lossy().as_ref(),
            ]
            .iter()
            .copied()
            .chain(common.iter().copied()),
        )
        .assert()
        .success();

    // two-source with A == B (source-a points at B) → must byte-match single
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(
            [
                "render-rutt-etra-sequence",
                b_dir.to_string_lossy().as_ref(),
                two_ab_dir.to_string_lossy().as_ref(),
                "--source-a-dir",
                b_dir.to_string_lossy().as_ref(),
            ]
            .iter()
            .copied()
            .chain(common.iter().copied()),
        )
        .assert()
        .success();

    // two-source with a distinct A → diverges, records the two-source id
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(
            [
                "render-rutt-etra-sequence",
                b_dir.to_string_lossy().as_ref(),
                two_distinct_dir.to_string_lossy().as_ref(),
                "--source-a-dir",
                a_dir.to_string_lossy().as_ref(),
            ]
            .iter()
            .copied()
            .chain(common.iter().copied()),
        )
        .assert()
        .success();

    // Continuity identity: A == B is byte-identical to single-source.
    for index in 0..2 {
        let file_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(single_dir.join(&file_name)).expect("single frame"),
            fs::read(two_ab_dir.join(&file_name)).expect("two-ab frame"),
            "A==B two-source must byte-match single-source ({file_name})"
        );
    }

    // A distinct A must actually change at least one frame.
    let single0 = fs::read(single_dir.join("frame_000000.png")).expect("single 0");
    let distinct0 = fs::read(two_distinct_dir.join("frame_000000.png")).expect("distinct 0");
    assert_ne!(
        single0, distinct0,
        "a distinct Source A must change the displacement"
    );

    // Algorithm id switches and source_a provenance is recorded.
    let single_manifest = read_json(&single_dir.join("manifest.json"));
    assert_eq!(single_manifest["algorithm"], "rutt_etra_scanline_cpu_v1");
    assert!(single_manifest.get("source_a").is_none());
    let two_manifest = read_json(&two_distinct_dir.join("manifest.json"));
    assert_eq!(two_manifest["algorithm"], "rutt_etra_two_source_cpu_v1");
    assert_eq!(
        two_manifest["source_a"],
        a_dir.to_string_lossy().to_string()
    );
}

/// Two-source Metal renders byte-identical to CPU on the gather kernel, and the
/// Metal manifest records the two-source Metal algorithm id.
#[cfg(target_os = "macos")]
#[test]
fn render_rutt_etra_two_source_metal_matches_cpu_byte_identical() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let b_dir = temp_dir.path().join("source-b");
    let a_dir = temp_dir.path().join("source-a");
    let cpu_dir = temp_dir.path().join("cpu");
    let metal_dir = temp_dir.path().join("metal");
    fs::create_dir_all(&b_dir).expect("create b frames");
    fs::create_dir_all(&a_dir).expect("create a frames");

    for index in 0..2u32 {
        let b = ImageBuffer::from_fn(16, 16, |x, _| {
            let value = (x as u8).wrapping_mul(16).wrapping_add(index as u8);
            Rgba([value, 255 - value, value / 2, u8::MAX])
        });
        b.save(b_dir.join(format!("frame_{index:06}.png")))
            .expect("write b frame");
        let a = ImageBuffer::from_fn(16, 16, |_, y| {
            let value = (y as u8).wrapping_mul(16);
            Rgba([value, value, value, u8::MAX])
        });
        a.save(a_dir.join(format!("frame_{index:06}.png")))
            .expect("write a frame");
    }

    for (backend, out_dir) in [("cpu", &cpu_dir), ("metal", &metal_dir)] {
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-rutt-etra-sequence",
                b_dir.to_string_lossy().as_ref(),
                out_dir.to_string_lossy().as_ref(),
                "--source-a-dir",
                a_dir.to_string_lossy().as_ref(),
                "--frames",
                "2",
                "--line-pitch",
                "4",
                "--displacement-depth",
                "12.5",
                "--line-thickness",
                "2",
                "--backend",
                backend,
            ])
            .assert()
            .success();
    }

    for index in 0..2 {
        let file_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(cpu_dir.join(&file_name)).expect("cpu frame"),
            fs::read(metal_dir.join(&file_name)).expect("metal frame"),
            "metal two-source must be byte-identical to cpu ({file_name})"
        );
    }

    let manifest = read_json(&metal_dir.join("manifest.json"));
    assert_eq!(manifest["algorithm"], "rutt_etra_two_source_metal_v1");
    assert_eq!(manifest["frame_count"], 2);
}

/// AC 3: `--backend metal` renders byte-identical to `--backend cpu` on the
/// gather kernel, and the Metal manifest records the Metal algorithm id.
#[cfg(target_os = "macos")]
#[test]
fn render_rutt_etra_sequence_metal_matches_cpu_byte_identical() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    let cpu_dir = temp_dir.path().join("cpu-frames");
    let metal_dir = temp_dir.path().join("metal-frames");
    fs::create_dir_all(&source_dir).expect("create source frames");

    for index in 0..2u32 {
        let frame = ImageBuffer::from_fn(16, 16, |x, _| {
            let value = (x as u8).wrapping_mul(16).wrapping_add(index as u8);
            Rgba([value, value, value, u8::MAX])
        });
        frame
            .save(source_dir.join(format!("frame_{index:06}.png")))
            .expect("write source frame");
    }

    for (backend, out_dir) in [("cpu", &cpu_dir), ("metal", &metal_dir)] {
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-rutt-etra-sequence",
                source_dir.to_string_lossy().as_ref(),
                out_dir.to_string_lossy().as_ref(),
                "--frames",
                "2",
                "--line-pitch",
                "4",
                "--displacement-depth",
                "12.5",
                "--line-thickness",
                "2",
                "--backend",
                backend,
            ])
            .assert()
            .success();
    }

    // The gather kernel is stateless and currently byte-identical on Apple
    // silicon; if this ever fails while the epsilon parity gate passes, that is
    // real hardware drift — loosen this to an epsilon comparison then, not now.
    for index in 0..2 {
        let file_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(cpu_dir.join(&file_name)).expect("cpu frame"),
            fs::read(metal_dir.join(&file_name)).expect("metal frame"),
            "metal render must be byte-identical to cpu render ({file_name})"
        );
    }

    let manifest = read_json(&metal_dir.join("manifest.json"));
    assert_eq!(manifest["algorithm"], "rutt_etra_scanline_metal_v1");
    assert_eq!(manifest["line_pitch"], 4);
    assert_eq!(manifest["displacement_depth"], 12.5);
    assert_eq!(manifest["line_thickness"], 2);
    assert_eq!(manifest["frame_count"], 2);
}

/// AC 4: a queued `--backend metal` render is byte-identical to the direct
/// `--backend metal` CLI render, and the queue task JSON pins `backend: metal`.
#[cfg(target_os = "macos")]
#[test]
fn queue_rutt_etra_metal_matches_direct_render() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    fs::create_dir_all(&source_dir).expect("create source frames");

    for index in 0..2u32 {
        let frame = ImageBuffer::from_fn(16, 16, |x, _| {
            let value = (x as u8).wrapping_mul(16).wrapping_add(index as u8);
            Rgba([value, value, value, u8::MAX])
        });
        frame
            .save(source_dir.join(format!("frame_{index:06}.png")))
            .expect("write source frame");
    }

    let source_arg = source_dir.to_string_lossy().to_string();
    let direct_dir = temp_dir.path().join("direct");
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_arg.as_str(),
            direct_dir.to_string_lossy().as_ref(),
            "--frames",
            "2",
            "--line-pitch",
            "4",
            "--backend",
            "metal",
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-rutt-etra-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root.to_string_lossy().as_ref(),
            "--frames",
            "2",
            "--frame-rate",
            "4",
            "--line-pitch",
            "4",
            "--backend",
            "metal",
        ])
        .assert()
        .success();

    let queue_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queue"))
            .expect("parse queue");
    assert_eq!(queue_json["jobs"][0]["task"]["backend"], "metal");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-rutt-etra-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for file_name in ["frame_000000.png", "frame_000001.png", "manifest.json"] {
        assert_eq!(
            fs::read(output_root.join("job-0001/frames").join(file_name)).expect("queued file"),
            fs::read(direct_dir.join(file_name)).expect("direct file"),
            "queue render must be byte-identical to direct render ({file_name})"
        );
    }
}

/// Two-source queue add→run is byte-identical to the direct two-source render,
/// the persisted task records `source_a_directory`, and the manifest carries the
/// two-source algorithm id. CPU backend so this runs on every platform.
#[test]
fn queue_rutt_etra_two_source_matches_direct_render() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let b_dir = temp_dir.path().join("source-b");
    let a_dir = temp_dir.path().join("source-a");
    fs::create_dir_all(&b_dir).expect("create b frames");
    fs::create_dir_all(&a_dir).expect("create a frames");

    for index in 0..2u32 {
        let b = ImageBuffer::from_fn(16, 16, |x, _| {
            let value = (x as u8).wrapping_mul(16).wrapping_add(index as u8);
            Rgba([value, 255 - value, value / 2, u8::MAX])
        });
        b.save(b_dir.join(format!("frame_{index:06}.png")))
            .expect("write b frame");
        let a = ImageBuffer::from_fn(16, 16, |_, y| {
            let value = (y as u8).wrapping_mul(16);
            Rgba([value, value, value, u8::MAX])
        });
        a.save(a_dir.join(format!("frame_{index:06}.png")))
            .expect("write a frame");
    }

    let b_arg = b_dir.to_string_lossy().to_string();
    let a_arg = a_dir.to_string_lossy().to_string();
    let direct_dir = temp_dir.path().join("direct");
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            b_arg.as_str(),
            direct_dir.to_string_lossy().as_ref(),
            "--source-a-dir",
            a_arg.as_str(),
            "--frames",
            "2",
            "--line-pitch",
            "4",
            "--displacement-depth",
            "18.0",
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-rutt-etra-sequence",
            queue_arg.as_str(),
            b_arg.as_str(),
            output_root.to_string_lossy().as_ref(),
            "--source-a-dir",
            a_arg.as_str(),
            "--frames",
            "2",
            "--frame-rate",
            "4",
            "--line-pitch",
            "4",
            "--displacement-depth",
            "18.0",
        ])
        .assert()
        .success();

    let queue_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queue"))
            .expect("parse queue");
    assert_eq!(queue_json["jobs"][0]["task"]["source_a_directory"], a_arg);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-rutt-etra-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for file_name in ["frame_000000.png", "frame_000001.png", "manifest.json"] {
        assert_eq!(
            fs::read(output_root.join("job-0001/frames").join(file_name)).expect("queued file"),
            fs::read(direct_dir.join(file_name)).expect("direct file"),
            "queued two-source render must be byte-identical to direct ({file_name})"
        );
    }

    let manifest = read_json(&direct_dir.join("manifest.json"));
    assert_eq!(manifest["algorithm"], "rutt_etra_two_source_cpu_v1");
    assert_eq!(manifest["source_a"], a_arg);
}

/// Tier 5.4 S2: queue add→run with an active spatial matte (half-black/
/// half-white `a-luma` over a strong displacement — the half-frame readout
/// shape) is byte-identical to the direct render, on both backends. The queue
/// path shares the direct render function, so the persisted `frames/
/// manifest.json` gains the exact same `matte` block as the direct manifest
/// (the milestone's "shares the direct code path" claim, asserted directly).
fn queue_rutt_etra_matte_matches_direct_render(backend: &str) {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    fs::create_dir_all(&source_dir).expect("create source frames");
    for index in 0..2u32 {
        let frame = ImageBuffer::from_fn(16, 16, |x, _| {
            let value = (x as u8).wrapping_mul(16).wrapping_add(index as u8);
            Rgba([value, value, value, u8::MAX])
        });
        frame
            .save(source_dir.join(format!("frame_{index:06}.png")))
            .expect("write source frame");
    }

    let matte_dir = temp_dir.path().join("matte-frames");
    fs::create_dir_all(&matte_dir).expect("create matte frames");
    for index in 0..2u32 {
        let frame: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(16, 16, |x, _| {
            if x < 8 {
                Rgba([0u8, 0, 0, 255])
            } else {
                Rgba([255u8, 255, 255, 255])
            }
        });
        frame
            .save(matte_dir.join(format!("frame_{index:06}.png")))
            .expect("write matte frame");
    }

    let source_arg = source_dir.to_string_lossy().to_string();
    let matte_arg = matte_dir.to_string_lossy().to_string();
    let direct_dir = temp_dir.path().join("direct");
    let plain_dir = temp_dir.path().join("plain");
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_arg.as_str(),
            direct_dir.to_string_lossy().as_ref(),
            "--frames",
            "2",
            "--line-pitch",
            "4",
            "--displacement-depth",
            "40.0",
            "--backend",
            backend,
            "--matte",
            "a-luma",
            "--matte-frames",
            matte_arg.as_str(),
        ])
        .assert()
        .success();

    // No-matte render of the identical source/settings: matte=1 half must
    // reproduce this exactly (Anchor 1); matte=0 half must reproduce the
    // plain carrier exactly (Anchor 2, checked directly against source frames).
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_arg.as_str(),
            plain_dir.to_string_lossy().as_ref(),
            "--frames",
            "2",
            "--line-pitch",
            "4",
            "--displacement-depth",
            "40.0",
            "--backend",
            backend,
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-rutt-etra-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root.to_string_lossy().as_ref(),
            "--frames",
            "2",
            "--frame-rate",
            "4",
            "--line-pitch",
            "4",
            "--displacement-depth",
            "40.0",
            "--backend",
            backend,
            "--matte",
            "a-luma",
            "--matte-frames",
            matte_arg.as_str(),
        ])
        .assert()
        .success();

    let queue_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queue"))
            .expect("parse queue");
    assert_eq!(queue_json["jobs"][0]["task"]["matte_source"], "a-luma");
    assert_eq!(queue_json["jobs"][0]["task"]["matte_frames"], matte_arg);
    assert_eq!(
        queue_json["jobs"][0]["task"]["matte_gain"],
        serde_json::Value::Null
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-rutt-etra-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for file_name in ["frame_000000.png", "frame_000001.png", "manifest.json"] {
        assert_eq!(
            fs::read(output_root.join("job-0001/frames").join(file_name)).expect("queued file"),
            fs::read(direct_dir.join(file_name)).expect("direct file"),
            "queued matte render must be byte-identical to direct render ({file_name}, backend {backend})"
        );
    }

    let direct_manifest = read_json(&direct_dir.join("manifest.json"));
    assert_eq!(direct_manifest["matte"]["source"], "a-luma");
    assert_eq!(direct_manifest["matte"]["frames"], matte_arg);

    // Half-frame readout: left half (black matte, value 0) is carrier-
    // identical; right half (white matte, value 1) reproduces the plain
    // (unmatted) effect exactly.
    let matted = image::open(direct_dir.join("frame_000001.png"))
        .expect("open matted")
        .to_rgba8();
    let plain = image::open(plain_dir.join("frame_000001.png"))
        .expect("open plain")
        .to_rgba8();
    let carrier = image::open(source_dir.join("frame_000001.png"))
        .expect("open carrier")
        .to_rgba8();

    for y in 0..16u32 {
        for x in 0..8u32 {
            assert_eq!(
                matted.get_pixel(x, y),
                carrier.get_pixel(x, y),
                "black-matte half must equal the plain carrier at ({x},{y}), backend {backend}"
            );
        }
        for x in 8..16u32 {
            assert_eq!(
                matted.get_pixel(x, y),
                plain.get_pixel(x, y),
                "white-matte half must equal the unmatted effect at ({x},{y}), backend {backend}"
            );
        }
    }
}

#[test]
fn queue_rutt_etra_matte_cpu_matches_direct_render() {
    queue_rutt_etra_matte_matches_direct_render("cpu");
}

#[test]
fn queue_rutt_etra_matte_metal_matches_direct_render() {
    queue_rutt_etra_matte_matches_direct_render("metal");
}

#[test]
fn render_rutt_etra_sequence_modulation_continuity_identity() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);

    let source_arg = source_dir.to_string_lossy().to_string();
    let run = |output_dir: &str, extra: &[&str]| {
        let mut args = vec![
            "render-rutt-etra-sequence",
            source_arg.as_str(),
            output_dir,
            "--frames",
            "3",
        ];
        args.extend_from_slice(extra);
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
    };

    // Continuity identity: `scale 0, offset K` pins the knob to K — byte-
    // identical to passing the constant directly (the zero-route settings-copy
    // path leaves the unrouted knobs untouched).
    let constant_dir = temp_dir.path().join("constant-output");
    run(
        &constant_dir.to_string_lossy(),
        &["--displacement-depth", "6"],
    )
    .success();
    let routed_dir = temp_dir.path().join("routed-output");
    run(
        &routed_dir.to_string_lossy(),
        &[
            "--modulate",
            "displacement_depth=luma:0,6",
            "--modulator-frames",
            source_arg.as_str(),
        ],
    )
    .success()
    .stdout(predicate::str::contains(
        "modulation routes: displacement_depth=luma:0,6",
    ));
    assert_png_frames_identical(&constant_dir, &routed_dir, 3);

    // The route reaches the render: the pinned 6 differs from the default 48.
    let default_dir = temp_dir.path().join("default-output");
    run(&default_dir.to_string_lossy(), &[]).success();
    assert_ne!(
        fs::read(routed_dir.join("frame_000000.png")).expect("routed frame"),
        fs::read(default_dir.join("frame_000000.png")).expect("default frame"),
        "routed displacement_depth must change the rendered sequence"
    );

    // `mono` is a flag, not a modulation target.
    let rejected_dir = temp_dir.path().join("rejected-output");
    run(
        &rejected_dir.to_string_lossy(),
        &[
            "--modulate",
            "mono=luma",
            "--modulator-frames",
            source_arg.as_str(),
        ],
    )
    .failure()
    .stderr(predicate::str::contains(
        "unknown rutt-etra modulation target",
    ));
}

fn write_chain_spec(path: &Path, spec_json: &str) {
    fs::write(path, spec_json).expect("write chain spec");
}

#[test]
fn field_particles_modulation_continuity_and_queue_match_direct() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("src");
    write_texture_sequence(&source_dir, &[0, 2, 4]);
    let src = source_dir.to_string_lossy().to_string();

    // Continuity identity: routed advect scale-0 offset-4 ≡ constant --advect 4.
    let constant_dir = temp_dir.path().join("const");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-field-particles-sequence", &src,
            constant_dir.to_string_lossy().as_ref(),
            "--frames", "3", "--seed", "1", "--advect", "4",
        ])
        .assert()
        .success();
    let routed_dir = temp_dir.path().join("routed");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-field-particles-sequence", &src,
            routed_dir.to_string_lossy().as_ref(),
            "--frames", "3", "--seed", "1",
            "--modulate", "advect=lfo(sine,1):0,4",
        ])
        .assert()
        .success();
    assert_png_frames_identical(&constant_dir, &routed_dir, 3);

    // Queue add→run byte-identical to a direct modulated render (frame_rate 12
    // matches the direct default --modulation-fps 12 so the LFO samples equal).
    let queue_path = temp_dir.path().join("queue.json");
    let queued_root = temp_dir.path().join("queued-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-field-particles-sequence", queue_path.to_string_lossy().as_ref(),
            &src, queued_root.to_string_lossy().as_ref(),
            "--frames", "3", "--seed", "1", "--frame-rate", "12",
            "--modulate", "advect=lfo(sine,0.5):3,6",
        ])
        .assert()
        .success();
    let task = read_json(&queue_path)["jobs"][0]["task"].clone();
    assert_eq!(task["type"], "frame_sequence_field_particles");
    assert_eq!(task["modulation_routes"][0]["target"], "advect");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-field-particles-sequence", queue_path.to_string_lossy().as_ref()])
        .assert()
        .success();
    let direct_mod = temp_dir.path().join("direct-mod");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-field-particles-sequence", &src,
            direct_mod.to_string_lossy().as_ref(),
            "--frames", "3", "--seed", "1",
            "--modulate", "advect=lfo(sine,0.5):3,6",
        ])
        .assert()
        .success();
    assert_png_frames_identical(&direct_mod, &queued_root.join("job-0001/frames"), 3);
}

#[test]
fn queue_coagulated_add_run_matches_direct() {
    // The queued run (queue-add → queue-run) is byte-identical to a direct
    // render-coagulated-blend-sequence with the same knobs + modulation.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_a = temp_dir.path().join("A");
    let source_b = temp_dir.path().join("B");
    write_texture_sequence(&source_a, &[0, 2, 4]);
    write_texture_sequence(&source_b, &[1, 3, 5]);
    let a = source_a.to_string_lossy().to_string();
    let b = source_b.to_string_lossy().to_string();
    let queue_path = temp_dir.path().join("queue.json");
    let queued_root = temp_dir.path().join("queued-out");

    // Shared knobs: a modulated coagulation_strength (LFO — no media) + bias.
    let knobs = [
        "--bias",
        "1",
        "--edge-hardness",
        "0.6",
        "--max-frames",
        "3",
        "--modulate",
        "coagulation_strength=lfo(sine,0.5):6,6",
    ];

    let mut add_args = vec![
        "queue-add-coagulated-blend-sequence".to_string(),
        queue_path.to_string_lossy().to_string(),
        a.clone(),
        b.clone(),
        queued_root.to_string_lossy().to_string(),
    ];
    add_args.extend(knobs.iter().map(|s| s.to_string()));
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&add_args)
        .assert()
        .success();

    // The persisted task records the resolved spec (route + provenance-only).
    let task = read_json(&queue_path)["jobs"][0]["task"].clone();
    assert_eq!(task["type"], "frame_sequence_coagulated_blend");
    assert_eq!(task["coagulation_strength"], 0.0);
    assert_eq!(task["bias"], 1.0);
    assert_eq!(task["modulation_routes"][0]["target"], "coagulation_strength");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-coagulated-blend-sequence", queue_path.to_string_lossy().as_ref()])
        .assert()
        .success();

    let direct_dir = temp_dir.path().join("direct-out");
    let mut direct_args = vec![
        "render-coagulated-blend-sequence".to_string(),
        a,
        b,
        direct_dir.to_string_lossy().to_string(),
    ];
    direct_args.extend(knobs.iter().map(|s| s.to_string()));
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&direct_args)
        .assert()
        .success();

    assert_png_frames_identical(&direct_dir, &queued_root.join("job-0001/frames"), 3);
}

#[test]
fn coagulated_modulation_scale_zero_matches_constant_knob() {
    // Continuity identity: a route with scale 0, offset K is byte-identical to
    // the constant knob K — routing perturbs the engine only by the knob value.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_a = temp_dir.path().join("A");
    let source_b = temp_dir.path().join("B");
    write_texture_sequence(&source_a, &[0, 2, 4]);
    write_texture_sequence(&source_b, &[1, 3, 5]);

    let render = |out: &Path, extra: &[&str]| {
        let mut args = [
            "render-coagulated-blend-sequence",
            source_a.to_string_lossy().as_ref(),
            source_b.to_string_lossy().as_ref(),
            out.to_string_lossy().as_ref(),
            "--bias",
            "1",
            "--edge-hardness",
            "1",
            "--max-frames",
            "3",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
        args.extend(extra.iter().map(|s| s.to_string()));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
            .success();
    };

    let constant_dir = temp_dir.path().join("constant");
    render(&constant_dir, &["--coagulation-strength", "4"]);
    let routed_dir = temp_dir.path().join("routed");
    // lfo(sine,1) scaled by 0, offset 4 ⇒ the constant 4 at every frame.
    render(&routed_dir, &["--modulate", "coagulation_strength=lfo(sine,1):0,4"]);

    assert_png_frames_identical(&constant_dir, &routed_dir, 3);
}

#[test]
fn render_chain_spec_round_trips_and_writes_manifest_shape() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);

    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(
        &spec_path,
        r#"{
            "version": 1,
            "stages": [
                { "effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false },
                { "effect": "palette_quantize", "mode": "posterize", "levels": 4 }
            ]
        }"#,
    );
    let output_dir = temp_dir.path().join("chain-out");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-chain",
            spec_path.to_string_lossy().as_ref(),
            source_dir.to_string_lossy().as_ref(),
            output_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("rendered chain with 2 stage(s)"))
        .stdout(predicate::str::contains(
            output_dir
                .join("stage_02_palette_quantize")
                .display()
                .to_string(),
        ));

    // Stage directory names/order are the contracted shape.
    assert!(output_dir
        .join("stage_01_rutt_etra/frame_000000.png")
        .exists());
    assert!(output_dir.join("stage_01_rutt_etra/manifest.json").exists());
    assert!(output_dir
        .join("stage_02_palette_quantize/frame_000000.png")
        .exists());

    let manifest = read_json(&output_dir.join("chain-manifest.json"));
    assert_eq!(manifest["version"], 1);
    assert_eq!(manifest["frame_count"], 2);
    assert_eq!(manifest["stages"][0]["effect"], "rutt_etra");
    assert_eq!(
        manifest["stages"][0]["algorithm"],
        "rutt_etra_scanline_cpu_v1"
    );
    assert_eq!(manifest["stages"][0]["settings"]["line_pitch"], 4);
    assert_eq!(manifest["stages"][0]["settings"]["displacement_depth"], 6.0);
    assert_eq!(manifest["stages"][1]["effect"], "palette_quantize");
    assert_eq!(
        manifest["stages"][1]["algorithm"],
        "palette_quantize_posterize_cpu_v1"
    );
    assert_eq!(manifest["stages"][1]["settings"]["mode"], "posterize");
    assert_eq!(manifest["stages"][1]["settings"]["levels"], 4);
}

#[test]
fn render_chain_rejects_empty_stages_unknown_tag_unknown_field_and_bad_knob() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);
    let source_arg = source_dir.to_string_lossy().to_string();

    let run_case = |name: &str, spec_json: &str, expected_stderr: &str| {
        let spec_path = temp_dir.path().join(format!("{name}.json"));
        write_chain_spec(&spec_path, spec_json);
        let output_dir = temp_dir.path().join(format!("{name}-out"));

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-chain",
                spec_path.to_string_lossy().as_ref(),
                source_arg.as_str(),
                output_dir.to_string_lossy().as_ref(),
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected_stderr));

        assert!(
            !output_dir.exists(),
            "{name}: nothing must be written to the output dir on rejection"
        );
    };

    run_case(
        "empty-stages",
        r#"{"version": 1, "stages": []}"#,
        "at least one stage",
    );
    run_case(
        "unknown-tag",
        r#"{"version": 1, "stages": [{"effect": "not_a_real_effect"}]}"#,
        "unknown variant `not_a_real_effect`",
    );
    run_case(
        "unknown-field",
        r#"{"version": 1, "stages": [{"effect": "palette_quantize", "levels": 4, "bogus_field": true}]}"#,
        "unknown field `bogus_field`",
    );
    run_case(
        "bad-knob-palette",
        r#"{"version": 1, "stages": [{"effect": "palette_quantize", "mode": "posterize", "levels": 1}]}"#,
        "levels must be >= 2",
    );
    // Stage 2's typo must not leave stage 1's output on disk.
    run_case(
        "bad-knob-stage-2",
        r#"{"version": 1, "stages": [
            {"effect": "channel_shift"},
            {"effect": "rutt_etra", "line_pitch": 0}
        ]}"#,
        "line_pitch must be >= 1",
    );
}

#[test]
fn render_chain_interchange_bit_depth_rejects_invalid_value() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);
    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(
        &spec_path,
        r#"{"version": 1, "interchange_bit_depth": 12, "stages": [{"effect": "channel_shift"}]}"#,
    );
    let output_dir = temp_dir.path().join("chain-out");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-chain",
            spec_path.to_string_lossy().as_ref(),
            source_dir.to_string_lossy().as_ref(),
            output_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "interchange_bit_depth must be either 8 or 16",
        ));
    assert!(
        !output_dir.exists(),
        "an invalid interchange_bit_depth must leave no output"
    );
}

/// Tier 5.6 S1's falsifiable banding proof: a two-stage chain of near-identity
/// effects (`retro_static` strength 0.0 and `channel_shift` with all shifts 0 —
/// both exact f32 passthroughs) isolates the chain's *own* inter-stage PNG
/// round-trip as the only source of quantization error. The source is written
/// as a 16-bit PNG (near-continuous) so the loss under test is the chain's, not
/// the source file's. Depth 16 must strictly reduce both the max per-channel
/// error against the f32 reference and increase the distinct-value count along
/// a shallow (low-dynamic-range) gradient where 8-bit banding is visible.
#[test]
fn render_chain_interchange_depth_16_reduces_quantization_error_vs_f32_reference() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    fs::create_dir_all(&source_dir).expect("create source dir");

    // Shallow gradient: luma spans only [0.40, 0.42] across 256 columns — narrow
    // enough that 8-bit PNG's 256 global levels collapse it to a handful of
    // distinct values (visible banding), while 16-bit PNG's 65536 levels keep it
    // effectively continuous. Written at 16-bit so the *source* itself carries
    // near-continuous precision; load_image_f32 (image crate's to_rgba32f) reads
    // both 8- and 16-bit PNGs at full precision, so this isolates the chain's own
    // inter-stage bit depth rather than the source file's.
    let width = 256u32;
    let height = 2u32;
    let gradient_low = 0.40f32;
    let gradient_span = 0.02f32;
    let source_image: ImageBuffer<image::Rgba<u16>, Vec<u16>> =
        ImageBuffer::from_fn(width, height, |x, _y| {
            let t = x as f32 / (width - 1) as f32;
            let luma = gradient_low + t * gradient_span;
            let v = (luma.clamp(0.0, 1.0) * u16::MAX as f32).round() as u16;
            image::Rgba([v, v, v, u16::MAX])
        });
    let source_path = source_dir.join("frame_000000.png");
    source_image.save(&source_path).expect("write 16-bit gradient source");

    let chain_spec = |depth: u8| {
        format!(
            r#"{{"version": 1, "interchange_bit_depth": {depth}, "stages": [
                {{"effect": "retro_static", "strength": 0.0}},
                {{"effect": "channel_shift"}}
            ]}}"#
        )
    };

    let run_chain = |depth: u8| -> image::Rgba32FImage {
        let spec_path = temp_dir.path().join(format!("chain-depth-{depth}.json"));
        write_chain_spec(&spec_path, &chain_spec(depth));
        let output_dir = temp_dir.path().join(format!("chain-out-{depth}"));

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-chain",
                spec_path.to_string_lossy().as_ref(),
                source_dir.to_string_lossy().as_ref(),
                output_dir.to_string_lossy().as_ref(),
            ])
            .assert()
            .success();

        let final_frame = output_dir.join("stage_02_channel_shift/frame_000000.png");
        image::open(&final_frame)
            .expect("open chain output frame")
            .to_rgba32f()
    };

    // The f32 reference: both stages are exact passthroughs, so the reference is
    // just the source loaded at full precision — no PNG round-trip in between.
    let reference = image::open(&source_path)
        .expect("open source frame")
        .to_rgba32f();

    let depth8 = run_chain(8);
    let depth16 = run_chain(16);

    let row_errors_and_distinct = |frame: &image::Rgba32FImage| -> (f32, usize) {
        let mut max_error = 0f32;
        let mut distinct_bits = std::collections::HashSet::new();
        for x in 0..width {
            let reference_r = reference.get_pixel(x, 0).0[0];
            let sample_r = frame.get_pixel(x, 0).0[0];
            max_error = max_error.max((sample_r - reference_r).abs());
            distinct_bits.insert(sample_r.to_bits());
        }
        (max_error, distinct_bits.len())
    };

    let (max_error_8, distinct_8) = row_errors_and_distinct(&depth8);
    let (max_error_16, distinct_16) = row_errors_and_distinct(&depth16);

    println!(
        "banding proof: depth8 max_error={max_error_8:.6} distinct={distinct_8}; \
         depth16 max_error={max_error_16:.6} distinct={distinct_16}"
    );

    assert!(
        max_error_16 < max_error_8,
        "16-bit interchange must strictly reduce quantization error vs the f32 \
         reference (depth8={max_error_8}, depth16={max_error_16})"
    );
    assert!(
        distinct_16 > distinct_8,
        "16-bit interchange must strictly increase the distinct-value count along \
         the shallow gradient (depth8={distinct_8}, depth16={distinct_16})"
    );
    // The 8-bit path must show real banding on this shallow a gradient (not
    // merely "slightly fewer" distinct values) — otherwise the proof is moot.
    assert!(
        distinct_8 < 16,
        "the shallow gradient must actually band at 8-bit (got {distinct_8} distinct values)"
    );
}

/// The off-case pin: a pre-slice spec (no `interchange_bit_depth` field at
/// all) must render byte-identical output — stage frames and chain-manifest
/// — to the same spec with `"interchange_bit_depth": 8"` explicit. Absent ⇒ 8.
#[test]
fn render_chain_without_interchange_bit_depth_is_byte_identical_to_explicit_depth_8() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);

    let implicit_spec = r#"{"version": 1, "stages": [
        {"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false},
        {"effect": "palette_quantize", "mode": "posterize", "levels": 4}
    ]}"#;
    let explicit_spec = r#"{"version": 1, "interchange_bit_depth": 8, "stages": [
        {"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false},
        {"effect": "palette_quantize", "mode": "posterize", "levels": 4}
    ]}"#;

    let run = |name: &str, spec_json: &str| -> std::path::PathBuf {
        let spec_path = temp_dir.path().join(format!("{name}.json"));
        write_chain_spec(&spec_path, spec_json);
        let output_dir = temp_dir.path().join(format!("{name}-out"));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-chain",
                spec_path.to_string_lossy().as_ref(),
                source_dir.to_string_lossy().as_ref(),
                output_dir.to_string_lossy().as_ref(),
            ])
            .assert()
            .success();
        output_dir
    };

    let implicit_dir = run("implicit-depth", implicit_spec);
    let explicit_dir = run("explicit-depth-8", explicit_spec);

    assert_png_frames_identical(
        &implicit_dir.join("stage_01_rutt_etra"),
        &explicit_dir.join("stage_01_rutt_etra"),
        3,
    );
    assert_png_frames_identical(
        &implicit_dir.join("stage_02_palette_quantize"),
        &explicit_dir.join("stage_02_palette_quantize"),
        3,
    );
    assert_eq!(
        fs::read(implicit_dir.join("chain-manifest.json")).expect("implicit manifest"),
        fs::read(explicit_dir.join("chain-manifest.json")).expect("explicit manifest"),
        "an absent interchange_bit_depth must resolve identically to an explicit 8"
    );
}

#[test]
fn render_chain_single_stage_is_byte_identical_to_direct_render() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);
    let source_arg = source_dir.to_string_lossy().to_string();

    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(
        &spec_path,
        r#"{"version": 1, "stages": [
            {"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false}
        ]}"#,
    );
    let chain_output_dir = temp_dir.path().join("chain-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-chain",
            spec_path.to_string_lossy().as_ref(),
            source_arg.as_str(),
            chain_output_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    let direct_output_dir = temp_dir.path().join("direct-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_arg.as_str(),
            direct_output_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--line-pitch",
            "4",
            "--displacement-depth",
            "6.0",
            "--line-thickness",
            "1",
        ])
        .assert()
        .success();

    let chain_stage_dir = chain_output_dir.join("stage_01_rutt_etra");
    assert_png_frames_identical(&direct_output_dir, &chain_stage_dir, 3);
    assert_eq!(
        fs::read(chain_stage_dir.join("manifest.json")).expect("chain stage manifest"),
        fs::read(direct_output_dir.join("manifest.json")).expect("direct manifest"),
        "chain stage-1 manifest.json must be byte-identical to the direct render's manifest.json"
    );
}

#[test]
fn render_composition_single_scene_is_byte_identical_to_direct_chain() {
    // Anchor A1: a one-scene composition is byte-identical, frame for frame,
    // to `render-chain` run directly on that scene's chain spec + input —
    // compositions are a view over the engine, never a second engine.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);
    let source_arg = source_dir.to_string_lossy().to_string();

    // The chain spec the scene embeds, and the same spec rendered standalone.
    let chain_json = r#"{"version": 1, "stages": [
        {"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false}
    ]}"#;

    let comp_spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(
        &comp_spec_path,
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "solo", "duration_frames": 3, "input_dir": {source}, "chain": {chain}}}
            ]}}"#,
            source = serde_json::to_string(&source_arg).expect("encode source path"),
            chain = chain_json,
        ),
    );
    let comp_output_dir = temp_dir.path().join("comp-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            comp_spec_path.to_string_lossy().as_ref(),
            comp_output_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered composition with 1 scene(s) (3 frame(s))",
        ));

    let chain_spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(&chain_spec_path, chain_json);
    let chain_output_dir = temp_dir.path().join("chain-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-chain",
            chain_spec_path.to_string_lossy().as_ref(),
            source_arg.as_str(),
            chain_output_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    // The scene render is byte-identical to the standalone chain render
    // (same engine): stage frames + the chain manifest.
    let scene_stage_dir = comp_output_dir.join("scene_01_solo/stage_01_rutt_etra");
    let chain_stage_dir = chain_output_dir.join("stage_01_rutt_etra");
    assert_png_frames_identical(&chain_stage_dir, &scene_stage_dir, 3);
    assert_eq!(
        fs::read(comp_output_dir.join("scene_01_solo/chain-manifest.json"))
            .expect("scene chain manifest"),
        fs::read(chain_output_dir.join("chain-manifest.json")).expect("direct chain manifest"),
        "the scene's chain-manifest.json must be byte-identical to the direct render's"
    );

    // The assembled timeline (`frames/`) is byte-identical to the chain's
    // final stage frames — the single-scene global numbering is a verbatim copy.
    assert_png_frames_identical(&chain_stage_dir, &comp_output_dir.join("frames"), 3);
}

/// Tier 5.6 S1 composition smoke: a scene chain-spec carries
/// `interchange_bit_depth: 16` with zero composition changes needed (the scene
/// spec is inherited verbatim by `run_chain_spec`). A single-scene composition
/// has no crossfade, so its assembled `frames/` are the scene's solo-zone
/// output `fs::copy`'d verbatim (composition.rs's existing, unmodified
/// assembly path) — the composed output is therefore 16-bit too. Crossfade
/// zones stay 8-bit regardless (unmodified `crossfade_frame` → `save_png`),
/// consistent with this milestone's scope (S1 doesn't touch composition.rs).
#[test]
fn render_composition_scene_with_16_bit_interchange_composes_at_16_bit() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);
    let source_arg = source_dir.to_string_lossy().to_string();

    let chain_json =
        r#"{"version": 1, "interchange_bit_depth": 16, "stages": [{"effect": "channel_shift"}]}"#;

    let comp_spec_path = temp_dir.path().join("composition-16bit.json");
    write_chain_spec(
        &comp_spec_path,
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "solo", "duration_frames": 2, "input_dir": {source}, "chain": {chain}}}
            ]}}"#,
            source = serde_json::to_string(&source_arg).expect("encode source path"),
            chain = chain_json,
        ),
    );
    let comp_output_dir = temp_dir.path().join("comp-out-16bit");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            comp_spec_path.to_string_lossy().as_ref(),
            comp_output_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    let scene_stage_frame =
        comp_output_dir.join("scene_01_solo/stage_01_channel_shift/frame_000000.png");
    let composed_frame = comp_output_dir.join("frames/frame_000000.png");
    for (label, path) in [
        ("scene stage output", &scene_stage_frame),
        ("composed timeline", &composed_frame),
    ] {
        let color = image::open(path)
            .unwrap_or_else(|e| panic!("open {label} frame: {e}"))
            .color();
        assert_eq!(
            color,
            image::ColorType::Rgba16,
            "{label} must be a 16-bit PNG when the scene's interchange_bit_depth is 16; got {color:?}"
        );
    }
}

#[test]
fn render_composition_rejects_invalid_specs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);
    let source_arg = source_dir.to_string_lossy().to_string();
    let source = serde_json::to_string(&source_arg).expect("encode source path");
    let good_chain = r#"{"version": 1, "stages": [{"effect": "palette_quantize", "mode": "posterize", "levels": 4}]}"#;

    let run_case = |name: &str, spec_json: &str, expected_stderr: &str| {
        let spec_path = temp_dir.path().join(format!("{name}.json"));
        write_chain_spec(&spec_path, spec_json);
        let output_dir = temp_dir.path().join(format!("{name}-out"));

        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-composition",
                spec_path.to_string_lossy().as_ref(),
                output_dir.to_string_lossy().as_ref(),
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected_stderr));

        assert!(
            !output_dir.exists(),
            "{name}: nothing must be written to the output dir on rejection"
        );
    };

    run_case(
        "bad-version",
        &format!(
            r#"{{"version": 2, "fps": 12, "scenes": [{{"name": "a", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}}}]}}"#
        ),
        "unsupported composition spec version 2",
    );
    run_case(
        "bad-fps",
        &format!(
            r#"{{"version": 1, "fps": 0, "scenes": [{{"name": "a", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}}}]}}"#
        ),
        "fps must be positive and finite",
    );
    run_case(
        "no-scenes",
        r#"{"version": 1, "fps": 12, "scenes": []}"#,
        "at least one scene",
    );
    run_case(
        "bad-scene-name",
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [{{"name": "../evil", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}}}]}}"#
        ),
        "is invalid",
    );
    run_case(
        "zero-duration",
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [{{"name": "a", "duration_frames": 0, "input_dir": {source}, "chain": {good_chain}}}]}}"#
        ),
        "at least 1 frame",
    );
    run_case(
        "last-scene-transition",
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [{{"name": "a", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}, "transition_out": {{"type": "cut"}}}}]}}"#
        ),
        "must omit transition_out",
    );
    run_case(
        "bad-chain-knob",
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [{{"name": "a", "duration_frames": 2, "input_dir": {source}, "chain": {{"version": 1, "stages": [{{"effect": "palette_quantize", "mode": "posterize", "levels": 1}}]}}}}]}}"#
        ),
        "levels must be >= 2",
    );
    run_case(
        "unknown-field",
        &format!(
            r#"{{"version": 1, "fps": 12, "bogus": true, "scenes": [{{"name": "a", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}}}]}}"#
        ),
        "unknown field `bogus`",
    );
    // A cut has no overlap — a non-zero frames field on it is a spec error.
    run_case(
        "cut-with-frames",
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "a", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}, "transition_out": {{"type": "cut", "frames": 4}}}},
                {{"name": "b", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}}}
            ]}}"#
        ),
        "a cut has no overlap",
    );
    // A crossfade longer than the scene it consumes is a spec error.
    run_case(
        "crossfade-too-long",
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "a", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}, "transition_out": {{"type": "crossfade", "frames": 3}}}},
                {{"name": "b", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}}}
            ]}}"#
        ),
        "exceed its duration_frames",
    );
    // Duplicate names caught in the multi-scene shape.
    run_case(
        "duplicate-names",
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "a", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}, "transition_out": {{"type": "cut"}}}},
                {{"name": "a", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}}}
            ]}}"#
        ),
        "duplicate scene name",
    );
}

#[test]
fn render_composition_cut_is_concat_of_scene_renders() {
    // Anchor A2: with cut transitions, <out>/frames/ is byte-identical to the
    // concatenation (renumbering only) of the per-scene renders.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]); // 3 frames
    let source = serde_json::to_string(&source_dir.to_string_lossy().to_string())
        .expect("encode source path");

    // Two distinct scenes (different effects) so the concat is falsifiable.
    let spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(
        &spec_path,
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "quant", "duration_frames": 3, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "palette_quantize", "mode": "posterize", "levels": 4}}]}},
                  "transition_out": {{"type": "cut"}}}},
                {{"name": "scan", "duration_frames": 3, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false}}]}}}}
            ]}}"#
        ),
    );
    let out_dir = temp_dir.path().join("comp-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            spec_path.to_string_lossy().as_ref(),
            out_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered composition with 2 scene(s) (6 frame(s))",
        ));

    // frames/ 0..2 = scene 1's render; frames/ 3..5 = scene 2's render, renumbered.
    let scene1 = out_dir.join("scene_01_quant/stage_01_palette_quantize");
    let scene2 = out_dir.join("scene_02_scan/stage_01_rutt_etra");
    let frames = out_dir.join("frames");
    for i in 0..3 {
        assert_eq!(
            fs::read(frames.join(format!("frame_{i:06}.png"))).expect("global frame"),
            fs::read(scene1.join(format!("frame_{i:06}.png"))).expect("scene1 frame"),
            "timeline frame {i} must equal scene 1 frame {i}"
        );
    }
    for i in 0..3 {
        let global = 3 + i;
        assert_eq!(
            fs::read(frames.join(format!("frame_{global:06}.png"))).expect("global frame"),
            fs::read(scene2.join(format!("frame_{i:06}.png"))).expect("scene2 frame"),
            "timeline frame {global} must equal scene 2 frame {i}"
        );
    }
    assert!(
        !frames.join("frame_000006.png").exists(),
        "the timeline must be exactly 6 frames"
    );

    // composition-manifest.json: global timeline + embedded chain manifests.
    let manifest = read_json(&out_dir.join("composition-manifest.json"));
    assert_eq!(manifest["version"], 1);
    assert_eq!(manifest["fps"], 12.0);
    assert_eq!(manifest["frame_count"], 6);
    assert_eq!(manifest["scenes"][0]["name"], "quant");
    assert_eq!(manifest["scenes"][0]["start_frame"], 0);
    assert_eq!(manifest["scenes"][0]["length"], 3);
    assert_eq!(manifest["scenes"][0]["transition_out"]["type"], "cut");
    assert_eq!(
        manifest["scenes"][0]["chain_manifest"]["stages"][0]["algorithm"],
        "palette_quantize_posterize_cpu_v1"
    );
    assert_eq!(manifest["scenes"][1]["name"], "scan");
    assert_eq!(manifest["scenes"][1]["start_frame"], 3);
    assert_eq!(manifest["scenes"][1]["length"], 3);
    // The last scene transitions to nothing.
    assert!(manifest["scenes"][1]["transition_out"].is_null());
}

#[test]
fn render_composition_rejects_duration_mismatch() {
    // A post-render refusal: the declared timeline length must equal what the
    // scene actually rendered (a 3-frame source cannot be a 5-frame scene).
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]); // 3 frames, not 5
    let source = serde_json::to_string(&source_dir.to_string_lossy().to_string())
        .expect("encode source path");

    let spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(
        &spec_path,
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "solo", "duration_frames": 5, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "palette_quantize", "mode": "posterize", "levels": 4}}]}}}}
            ]}}"#
        ),
    );
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            spec_path.to_string_lossy().as_ref(),
            temp_dir.path().join("comp-out").to_string_lossy().as_ref(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "declares duration_frames 5 but its chain rendered 3 frame(s)",
        ));
}

fn write_solid_frames(dir: &Path, count: usize, color: [u8; 4]) {
    fs::create_dir_all(dir).expect("create solid frame dir");
    for index in 0..count {
        let image: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_pixel(8, 8, Rgba(color));
        image
            .save(dir.join(format!("frame_{:06}.png", index + 1)))
            .expect("save solid frame");
    }
}

fn read_pixel(path: &Path, x: u32, y: u32) -> [u8; 4] {
    image::open(path).expect("open frame").to_rgba8().get_pixel(x, y).0
}

#[test]
fn render_composition_crossfade_zero_is_byte_identical_to_cut() {
    // Anchor A3: a crossfade of 0 frames produces byte-identical output to cut.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);
    let source = serde_json::to_string(&source_dir.to_string_lossy().to_string())
        .expect("encode source path");

    let spec_for = |transition: &str| {
        format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "quant", "duration_frames": 3, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "palette_quantize", "mode": "posterize", "levels": 4}}]}},
                  "transition_out": {transition}}},
                {{"name": "scan", "duration_frames": 3, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false}}]}}}}
            ]}}"#
        )
    };

    let render = |name: &str, transition: &str| {
        let spec_path = temp_dir.path().join(format!("{name}.json"));
        write_chain_spec(&spec_path, &spec_for(transition));
        let out_dir = temp_dir.path().join(format!("{name}-out"));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-composition",
                spec_path.to_string_lossy().as_ref(),
                out_dir.to_string_lossy().as_ref(),
            ])
            .assert()
            .success();
        out_dir
    };

    let cut_out = render("cut", r#"{"type": "cut"}"#);
    let xfade0_out = render("xfade0", r#"{"type": "crossfade", "frames": 0}"#);

    // No overlap in either case: 3 + 3 = 6 timeline frames, byte-identical.
    assert_png_frames_identical(&cut_out.join("frames"), &xfade0_out.join("frames"), 6);
}

#[test]
fn render_composition_crossfade_blends_the_overlap_window() {
    // The visual/number proof, pinned to an exact value: a 1-frame crossfade
    // between a solid-red scene and a solid-blue scene has a single blend frame
    // at weight 1/2 — exactly the midpoint colour (128, 0, 128).
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let red_dir = temp_dir.path().join("red-frames");
    let blue_dir = temp_dir.path().join("blue-frames");
    write_solid_frames(&red_dir, 3, [255, 0, 0, 255]);
    write_solid_frames(&blue_dir, 3, [0, 0, 255, 255]);
    let red = serde_json::to_string(&red_dir.to_string_lossy().to_string()).expect("encode red");
    let blue = serde_json::to_string(&blue_dir.to_string_lossy().to_string()).expect("encode blue");

    // channel_shift with zero shifts is an identity passthrough, so each scene
    // renders its solid input verbatim.
    let spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(
        &spec_path,
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "warm", "duration_frames": 3, "input_dir": {red},
                  "chain": {{"version": 1, "stages": [{{"effect": "channel_shift"}}]}},
                  "transition_out": {{"type": "crossfade", "frames": 1}}}},
                {{"name": "cool", "duration_frames": 3, "input_dir": {blue},
                  "chain": {{"version": 1, "stages": [{{"effect": "channel_shift"}}]}}}}
            ]}}"#
        ),
    );
    let out_dir = temp_dir.path().join("comp-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            spec_path.to_string_lossy().as_ref(),
            out_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered composition with 2 scene(s) (5 frame(s))",
        ));

    let frames = out_dir.join("frames");
    // Solo red, blend, solo blue: 5 frames, with the blend at global index 2.
    assert_eq!(read_pixel(&frames.join("frame_000001.png"), 4, 4), [255, 0, 0, 255]);
    assert_eq!(
        read_pixel(&frames.join("frame_000002.png"), 4, 4),
        [128, 0, 128, 255],
        "the crossfade frame must be the weight-1/2 midpoint of red and blue"
    );
    assert_eq!(read_pixel(&frames.join("frame_000003.png"), 4, 4), [0, 0, 255, 255]);
    assert!(
        !frames.join("frame_000005.png").exists(),
        "the crossfade removes one frame: 3 + 3 - 1 = 5"
    );

    let manifest = read_json(&out_dir.join("composition-manifest.json"));
    assert_eq!(manifest["frame_count"], 5);
    assert_eq!(manifest["scenes"][0]["start_frame"], 0);
    assert_eq!(manifest["scenes"][0]["transition_out"]["type"], "crossfade");
    assert_eq!(manifest["scenes"][0]["transition_out"]["frames"], 1);
    assert_eq!(manifest["scenes"][1]["start_frame"], 2);
}

#[test]
fn render_composition_caches_unchanged_scenes_and_rerenders_edited_ones() {
    // Anchor A4: re-running an unchanged spec re-renders nothing and re-assembles
    // byte-identical output; changing only scene B leaves scene A byte-identical.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);
    let source = serde_json::to_string(&source_dir.to_string_lossy().to_string())
        .expect("encode source path");

    // Scene A is fixed; scene B's rutt-etra displacement is the tunable knob.
    let spec_for = |b_disp: f64| {
        format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "a", "duration_frames": 3, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "palette_quantize", "mode": "posterize", "levels": 4}}]}},
                  "transition_out": {{"type": "cut"}}}},
                {{"name": "b", "duration_frames": 3, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": {b_disp}, "line_thickness": 1, "mono": false}}]}}}}
            ]}}"#
        )
    };
    let spec_path = temp_dir.path().join("composition.json");
    let out_dir = temp_dir.path().join("comp-out");
    let render = |disp: f64| {
        write_chain_spec(&spec_path, &spec_for(disp));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-composition",
                spec_path.to_string_lossy().as_ref(),
                out_dir.to_string_lossy().as_ref(),
            ])
            .assert()
    };
    let scene_a = out_dir.join("scene_01_a/stage_01_palette_quantize/frame_000000.png");
    let scene_b = out_dir.join("scene_02_b/stage_01_rutt_etra/frame_000000.png");

    // Run 1: fresh.
    render(2.0)
        .success()
        .stdout(predicate::str::contains("scene 01 (a) rendered"))
        .stdout(predicate::str::contains("scene 02 (b) rendered"));
    let a_run1 = fs::read(&scene_a).expect("scene a frame run1");
    let b_run1 = fs::read(&scene_b).expect("scene b frame run1");
    let frames_run1: Vec<Vec<u8>> = (0..6)
        .map(|i| fs::read(out_dir.join(format!("frames/frame_{i:06}.png"))).expect("frame"))
        .collect();

    // Run 2: identical spec → every scene reused, output byte-identical.
    render(2.0)
        .success()
        .stdout(predicate::str::contains("scene 01 (a) reused from cache"))
        .stdout(predicate::str::contains("scene 02 (b) reused from cache"));
    assert_eq!(fs::read(&scene_a).expect("scene a run2"), a_run1);
    assert_eq!(fs::read(&scene_b).expect("scene b run2"), b_run1);
    for (i, expected) in frames_run1.iter().enumerate() {
        assert_eq!(
            &fs::read(out_dir.join(format!("frames/frame_{i:06}.png"))).expect("frame run2"),
            expected,
            "unchanged re-run must re-assemble byte-identical frame {i}"
        );
    }

    // Run 3: edit scene B only → A reused (byte-identical), B re-rendered (differs).
    render(20.0)
        .success()
        .stdout(predicate::str::contains("scene 01 (a) reused from cache"))
        .stdout(predicate::str::contains("scene 02 (b) rendered"));
    assert_eq!(
        fs::read(&scene_a).expect("scene a run3"),
        a_run1,
        "editing scene B must leave scene A byte-identical"
    );
    assert_ne!(
        fs::read(&scene_b).expect("scene b run3"),
        b_run1,
        "editing scene B's knob must change its render"
    );
}

#[test]
fn render_composition_resumes_after_a_scene_is_lost() {
    // Acceptance 4: a completed scene is reused, a lost scene re-renders, and
    // the result is byte-identical to an uninterrupted run.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);
    let source = serde_json::to_string(&source_dir.to_string_lossy().to_string())
        .expect("encode source path");
    let spec = format!(
        r#"{{"version": 1, "fps": 12, "scenes": [
            {{"name": "a", "duration_frames": 3, "input_dir": {source},
              "chain": {{"version": 1, "stages": [{{"effect": "palette_quantize", "mode": "posterize", "levels": 4}}]}},
              "transition_out": {{"type": "cut"}}}},
            {{"name": "b", "duration_frames": 3, "input_dir": {source},
              "chain": {{"version": 1, "stages": [{{"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false}}]}}}}
        ]}}"#
    );
    let spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(&spec_path, &spec);
    let render_into = |out: &Path| {
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-composition",
                spec_path.to_string_lossy().as_ref(),
                out.to_string_lossy().as_ref(),
            ])
            .assert()
    };

    // Reference: a single uninterrupted render.
    let ref_dir = temp_dir.path().join("ref-out");
    render_into(&ref_dir).success();

    // Working: render, then lose scene 2 (simulate an interruption after scene 1).
    let work_dir = temp_dir.path().join("work-out");
    render_into(&work_dir).success();
    fs::remove_dir_all(work_dir.join("scene_02_b")).expect("drop scene 2");

    // Re-run: scene 1 reused, scene 2 re-rendered, output byte-identical to ref.
    render_into(&work_dir)
        .success()
        .stdout(predicate::str::contains("scene 01 (a) reused from cache"))
        .stdout(predicate::str::contains("scene 02 (b) rendered"));
    assert_png_frames_identical(&ref_dir.join("frames"), &work_dir.join("frames"), 6);
}

#[test]
fn render_composition_refuses_cross_scene_dimension_mismatch() {
    // F1: a cut-only composition never reaches crossfade_frame's dimension
    // guard, so scenes of different sizes would assemble a mixed-dimension
    // frames/ silently and break ProRes/preview late. Pass 1 must refuse early.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let write_solid_seq = |dir: &Path, w: u32, h: u32, count: usize| {
        fs::create_dir_all(dir).expect("create source dir");
        for i in 0..count {
            let image: ImageBuffer<Rgba<u8>, Vec<u8>> =
                ImageBuffer::from_pixel(w, h, Rgba([40, 80, 160, 255]));
            image
                .save(dir.join(format!("frame_{:06}.png", i + 1)))
                .expect("save source frame");
        }
    };
    let src_a = temp_dir.path().join("src-a");
    let src_b = temp_dir.path().join("src-b");
    write_solid_seq(&src_a, 16, 16, 2);
    write_solid_seq(&src_b, 24, 12, 2); // different dimensions than scene 1
    let enc =
        |p: &Path| serde_json::to_string(&p.to_string_lossy().to_string()).expect("encode path");
    let a = enc(&src_a);
    let b = enc(&src_b);
    let chain = r#"{"version": 1, "stages": [{"effect": "palette_quantize", "mode": "posterize", "levels": 4}]}"#;

    let spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(
        &spec_path,
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "a", "duration_frames": 2, "input_dir": {a}, "chain": {chain}, "transition_out": {{"type": "cut"}}}},
                {{"name": "b", "duration_frames": 2, "input_dir": {b}, "chain": {chain}}}
            ]}}"#
        ),
    );
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            spec_path.to_string_lossy().as_ref(),
            temp_dir.path().join("comp-out").to_string_lossy().as_ref(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            r#"scene "b" renders at 24x12 but scene "a" established 16x16"#,
        ));
}

#[test]
fn render_composition_persists_fingerprint_before_rendering() {
    // F3: a scene's fingerprint must be persisted to the composition record
    // *before* its chain renders, not after. Otherwise a first run killed
    // mid-scene records `""` for the partial scene, so the re-run sees a
    // fingerprint mismatch, wipes the partial dir, and restarts from frame 0 —
    // discarding the chain's stage markers and any stateful checkpoint.
    //
    // We provoke a post-fingerprint failure deterministically: scene 2 declares
    // a duration that mismatches its source, which fails the post-render length
    // check. Under the fix, scene 2's fingerprint is already in the record when
    // the run aborts; without it, the record holds `""`.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]); // 3 frames
    let source = serde_json::to_string(&source_dir.to_string_lossy().to_string())
        .expect("encode source path");

    // Scene 1 is well-formed; scene 2 declares 5 frames over a 3-frame source,
    // so it renders then fails the length check *after* its fingerprint is set.
    let spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(
        &spec_path,
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "a", "duration_frames": 3, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "palette_quantize", "mode": "posterize", "levels": 4}}]}},
                  "transition_out": {{"type": "cut"}}}},
                {{"name": "b", "duration_frames": 5, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "palette_quantize", "mode": "posterize", "levels": 4}}]}}}}
            ]}}"#
        ),
    );
    let out_dir = temp_dir.path().join("comp-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            spec_path.to_string_lossy().as_ref(),
            out_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "declares duration_frames 5 but its chain rendered 3 frame(s)",
        ));

    // The record must carry scene 2's real fingerprint even though the run
    // aborted before scene 2 "completed" — this is what lets a genuine
    // mid-scene interruption resume its partial dir instead of restarting.
    let record = read_json(&out_dir.join("composition-record.json"));
    assert_eq!(record["scenes"][1]["name"], "b");
    assert!(
        record["scenes"][1]["fingerprint"]
            .as_str()
            .is_some_and(|f| !f.is_empty()),
        "scene 2's fingerprint must be persisted before its render, got {:?}",
        record["scenes"][1]["fingerprint"]
    );
}

#[test]
fn render_composition_refuses_structural_change_into_existing_dir() {
    // Acceptance 5: a changed scene name/order refuses (rather than reuse
    // another scene's cached frames).
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);
    let source = serde_json::to_string(&source_dir.to_string_lossy().to_string())
        .expect("encode source path");
    let good_chain = r#"{"version": 1, "stages": [{"effect": "palette_quantize", "mode": "posterize", "levels": 4}]}"#;
    let spec_for = |first: &str| {
        format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "{first}", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}, "transition_out": {{"type": "cut"}}}},
                {{"name": "b", "duration_frames": 2, "input_dir": {source}, "chain": {good_chain}}}
            ]}}"#
        )
    };
    let spec_path = temp_dir.path().join("composition.json");
    let out_dir = temp_dir.path().join("comp-out");
    let render = |first: &str| {
        write_chain_spec(&spec_path, &spec_for(first));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-composition",
                spec_path.to_string_lossy().as_ref(),
                out_dir.to_string_lossy().as_ref(),
            ])
            .assert()
    };

    render("a").success();
    // Rename the first scene → structural change → refuse, original dir intact.
    render("a2")
        .failure()
        .stderr(predicate::str::contains("scenes changed (names or order)"));
    assert!(
        out_dir.join("scene_01_a").is_dir(),
        "a refused structural change must not destroy the existing render"
    );
}

// A varying master signal (quiet first `quiet_frames` frames, loud after) at
// 512 samples/frame for fps 12, so trimming by whole frames shifts whole hops
// and the RMS route visibly moves the render.
fn master_wav_samples(total_frames: usize, quiet_frames: usize) -> Vec<f32> {
    let per_frame = 512usize;
    (0..total_frames * per_frame)
        .map(|i| {
            let amp = if i < quiet_frames * per_frame { 0.08 } else { 0.9 };
            if i % 2 == 0 {
                amp
            } else {
                -amp
            }
        })
        .collect()
}

#[test]
fn render_composition_master_route_matches_named_modulator() {
    // A5 (half 1): a one-scene composition routing from master.<source> is
    // byte-identical to the same scene routing the same file via an ordinary
    // named modulator — master is a view over the named-modulator engine.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4, 6]); // 4 frames
    let wav = temp_dir.path().join("score.wav");
    write_test_wav_at(&wav, 6144, &master_wav_samples(6, 3));

    let stage = |modulation: serde_json::Value| {
        serde_json::json!({
            "effect": "rutt_etra", "line_pitch": 2, "displacement_depth": 0.0,
            "line_thickness": 1, "mono": false, "modulation": modulation
        })
    };
    let scene = |chain_modulation: serde_json::Value| {
        serde_json::json!({
            "name": "main", "duration_frames": 4,
            "input_dir": source_dir.to_string_lossy(),
            "chain": {"version": 1, "stages": [stage(chain_modulation)]}
        })
    };

    let master_spec = serde_json::json!({
        "version": 1, "fps": 12, "master": {"audio": wav.to_string_lossy()},
        "scenes": [scene(serde_json::json!({"routes": ["displacement_depth=master.audio-rms:40"]}))]
    });
    let named_spec = serde_json::json!({
        "version": 1, "fps": 12,
        "scenes": [scene(serde_json::json!({
            "routes": ["displacement_depth=clk.audio-rms:40"],
            "named_modulator_audio": [format!("clk={}", wav.to_string_lossy())]
        }))]
    });

    let render = |spec: &serde_json::Value, name: &str| {
        let spec_path = temp_dir.path().join(format!("{name}.json"));
        write_chain_spec(&spec_path, &spec.to_string());
        let out = temp_dir.path().join(format!("{name}-out"));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-composition",
                spec_path.to_string_lossy().as_ref(),
                out.to_string_lossy().as_ref(),
            ])
            .assert()
            .success();
        out
    };

    let master_out = render(&master_spec, "master");
    let named_out = render(&named_spec, "named");
    assert_png_frames_identical(
        &named_out.join("scene_01_main/stage_01_rutt_etra"),
        &master_out.join("scene_01_main/stage_01_rutt_etra"),
        4,
    );
}

#[test]
fn render_composition_master_offset_equals_trimmed_media() {
    // A5 (half 2): a scene at global offset F reading master.<source> is
    // byte-identical to the same scene reading the master trimmed by F frames —
    // and differs from reading the master at offset 0 (the offset is live).
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let intro_src = temp_dir.path().join("intro");
    let main_src = temp_dir.path().join("main");
    write_texture_sequence(&intro_src, &[0, 2, 4]); // 3 frames
    write_texture_sequence(&main_src, &[0, 2, 4, 6]); // 4 frames

    // 9-frame master: frames 0-2 quiet, 3-8 loud. Scene "main" (4 frames) at
    // global offset 3 reads the loud stretch; at offset 0 the quiet stretch.
    let samples = master_wav_samples(9, 3);
    let full_wav = temp_dir.path().join("score.wav");
    write_test_wav_at(&full_wav, 6144, &samples);
    // Trim by 3 frames = 3 * 6144/12 = 1536 samples (matches the engine's trim).
    let trimmed_wav = temp_dir.path().join("score_trimmed.wav");
    write_test_wav_at(&trimmed_wav, 6144, &samples[1536..]);

    let main_scene = serde_json::json!({
        "name": "main", "duration_frames": 4, "input_dir": main_src.to_string_lossy(),
        "chain": {"version": 1, "stages": [{
            "effect": "rutt_etra", "line_pitch": 2, "displacement_depth": 0.0,
            "line_thickness": 1, "mono": false,
            "modulation": {"routes": ["displacement_depth=master.audio-rms:40"]}
        }]}
    });
    let one_scene = |audio: &Path| {
        serde_json::json!({
            "version": 1, "fps": 12, "master": {"audio": audio.to_string_lossy()},
            "scenes": [main_scene]
        })
    };
    let offset3 = serde_json::json!({
        "version": 1, "fps": 12, "master": {"audio": full_wav.to_string_lossy()},
        "scenes": [
            {"name": "intro", "duration_frames": 3, "input_dir": intro_src.to_string_lossy(),
             "chain": {"version": 1, "stages": [{"effect": "channel_shift"}]},
             "transition_out": {"type": "cut"}},
            main_scene
        ]
    });

    let render = |spec: &serde_json::Value, name: &str| {
        let spec_path = temp_dir.path().join(format!("{name}.json"));
        write_chain_spec(&spec_path, &spec.to_string());
        let out = temp_dir.path().join(format!("{name}-out"));
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-composition",
                spec_path.to_string_lossy().as_ref(),
                out.to_string_lossy().as_ref(),
            ])
            .assert()
            .success();
        out
    };

    let offset3_out = render(&offset3, "offset3");
    let trimmed_out = render(&one_scene(&trimmed_wav), "trimmed");
    let offset0_out = render(&one_scene(&full_wav), "offset0");

    let offset3_main = offset3_out.join("scene_02_main/stage_01_rutt_etra");
    let trimmed_main = trimmed_out.join("scene_01_main/stage_01_rutt_etra");
    let offset0_main = offset0_out.join("scene_01_main/stage_01_rutt_etra");

    // Offset F ≡ master trimmed by F frames.
    assert_png_frames_identical(&trimmed_main, &offset3_main, 4);
    // ...and the offset actually shifts which stretch is read (frame 0: loud vs quiet).
    assert_ne!(
        fs::read(offset3_main.join("frame_000000.png")).expect("offset3 frame"),
        fs::read(offset0_main.join("frame_000000.png")).expect("offset0 frame"),
        "the master offset must change which part of the track drives the scene"
    );
}

#[test]
fn render_composition_master_route_refuses_misaligned_envelope_fps() {
    // F4: the master trim aligns to the composition fps; a master-routed stage
    // sampling at a different envelope fps would read the master off-time,
    // silently. Refused at validation (nothing written); aligning the stage's
    // modulation fps renders.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let src = temp_dir.path().join("src");
    write_texture_sequence(&src, &[0, 2, 4]);
    let wav = temp_dir.path().join("score.wav");
    write_test_wav_at(&wav, 6144, &master_wav_samples(9, 3));

    let spec_for = |fps: serde_json::Value, modulation: serde_json::Value| {
        serde_json::json!({
            "version": 1, "fps": fps, "master": {"audio": wav.to_string_lossy()},
            "scenes": [{"name": "solo", "duration_frames": 3, "input_dir": src.to_string_lossy(),
                "chain": {"version": 1, "stages": [{
                    "effect": "rutt_etra", "line_pitch": 2, "displacement_depth": 0.0,
                    "line_thickness": 1, "mono": false, "modulation": modulation
                }]}}]
        })
    };
    let render = |spec: &serde_json::Value, name: &str| {
        let spec_path = temp_dir.path().join(format!("{name}.json"));
        write_chain_spec(&spec_path, &spec.to_string());
        let out = temp_dir.path().join(format!("{name}-out"));
        let assert = Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-composition",
                spec_path.to_string_lossy().as_ref(),
                out.to_string_lossy().as_ref(),
            ])
            .assert();
        (assert, out)
    };

    // Composition at 24 fps; the master-routed stateless stage defaults to an
    // envelope fps of 12 — refused, nothing written.
    let misaligned = spec_for(
        serde_json::json!(24),
        serde_json::json!({"routes": ["displacement_depth=master.audio-rms:40"]}),
    );
    let (assert, out) = render(&misaligned, "misaligned");
    assert
        .failure()
        .stderr(predicate::str::contains("differs from the composition fps"));
    assert!(!out.exists(), "refused spec must write nothing");

    // Same composition with the stage's modulation fps aligned to 24 — renders.
    let aligned = spec_for(
        serde_json::json!(24),
        serde_json::json!({
            "routes": ["displacement_depth=master.audio-rms:40"], "fps": 24
        }),
    );
    let (assert, out) = render(&aligned, "aligned");
    assert.success();
    assert!(out.join("composition-manifest.json").is_file());

    // A flow_feedback stage is pinned to its own frame rate (12): a master
    // route on it under a 24-fps composition is refused the same way.
    let feedback_spec = serde_json::json!({
        "version": 1, "fps": 24, "master": {"audio": wav.to_string_lossy()},
        "scenes": [{"name": "solo", "duration_frames": 3, "input_dir": src.to_string_lossy(),
            "chain": {"version": 1, "stages": [{
                "effect": "flow_feedback",
                "modulation": {"routes": ["feedback_mix=master.audio-rms:0.5,0.2"]}
            }]}}]
    });
    let (assert, out) = render(&feedback_spec, "feedback-misaligned");
    assert
        .failure()
        .stderr(predicate::str::contains("differs from the composition fps"));
    assert!(!out.exists(), "refused feedback spec must write nothing");
}

#[test]
fn render_composition_single_scene_matches_full_composition_scene() {
    // F2: `--scene <name>` renders that one scene at its composition timeline
    // offset (so its master binding is identical to the full render) without
    // rendering the prior scenes and without assembling the timeline.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let intro_src = temp_dir.path().join("intro");
    let main_src = temp_dir.path().join("main");
    write_texture_sequence(&intro_src, &[0, 2, 4]); // 3 frames
    write_texture_sequence(&main_src, &[0, 2, 4, 6]); // 4 frames

    // 9-frame master, quiet 0-2 / loud 3-8: scene "main" sits at global offset 3
    // (after the 3-frame "intro"), so the offset materially drives its output.
    let samples = master_wav_samples(9, 3);
    let full_wav = temp_dir.path().join("score.wav");
    write_test_wav_at(&full_wav, 6144, &samples);

    let spec = serde_json::json!({
        "version": 1, "fps": 12, "master": {"audio": full_wav.to_string_lossy()},
        "scenes": [
            {"name": "intro", "duration_frames": 3, "input_dir": intro_src.to_string_lossy(),
             "chain": {"version": 1, "stages": [{"effect": "channel_shift"}]},
             "transition_out": {"type": "cut"}},
            {"name": "main", "duration_frames": 4, "input_dir": main_src.to_string_lossy(),
             "chain": {"version": 1, "stages": [{
                 "effect": "rutt_etra", "line_pitch": 2, "displacement_depth": 0.0,
                 "line_thickness": 1, "mono": false,
                 "modulation": {"routes": ["displacement_depth=master.audio-rms:40"]}
             }]}}
        ]
    });
    let spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(&spec_path, &spec.to_string());

    // Full render (assembles the whole piece) into one dir.
    let full_out = temp_dir.path().join("full-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            spec_path.to_string_lossy().as_ref(),
            full_out.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    // Single-scene render of "main" into a *fresh* dir.
    let scene_out = temp_dir.path().join("scene-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            spec_path.to_string_lossy().as_ref(),
            scene_out.to_string_lossy().as_ref(),
            "--scene",
            "main",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered scene \"main\" (4 frame(s)) at timeline offset 3",
        ));

    // The single-scene "main" is byte-identical to "main" inside the full
    // composition — proving it was bound to the master at the same offset 3.
    assert_png_frames_identical(
        &full_out.join("scene_02_main/stage_01_rutt_etra"),
        &scene_out.join("scene_02_main/stage_01_rutt_etra"),
        4,
    );

    // No prior scene was rendered, and the piece was not assembled.
    assert!(
        !scene_out.join("scene_01_intro").exists(),
        "single-scene render must not render the scenes before it"
    );
    assert!(
        !scene_out.join("frames").exists(),
        "single-scene render must not assemble the timeline"
    );
    assert!(
        !scene_out.join("composition-manifest.json").exists(),
        "single-scene render must not write a composition manifest"
    );

    // The record keeps the full skeleton, so a later full run stays coherent
    // and reuses the scene rendered here.
    let record = read_json(&scene_out.join("composition-record.json"));
    assert_eq!(record["scenes"][0]["name"], "intro");
    assert_eq!(record["scenes"][1]["name"], "main");
    assert!(
        record["scenes"][1]["fingerprint"].as_str().is_some_and(|f| !f.is_empty()),
        "the rendered scene's fingerprint must be recorded"
    );
}

#[test]
fn render_composition_single_scene_rejects_unknown_name() {
    // F2: an unknown --scene name refuses and lists the available scenes.
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);
    let source = serde_json::to_string(&source_dir.to_string_lossy().to_string())
        .expect("encode source path");

    let spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(
        &spec_path,
        &format!(
            r#"{{"version": 1, "fps": 12, "scenes": [
                {{"name": "solo", "duration_frames": 3, "input_dir": {source},
                  "chain": {{"version": 1, "stages": [{{"effect": "channel_shift"}}]}}}}
            ]}}"#
        ),
    );
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            spec_path.to_string_lossy().as_ref(),
            temp_dir.path().join("comp-out").to_string_lossy().as_ref(),
            "--scene",
            "nope",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no scene named \"nope\" in the composition; available scenes: solo",
        ));
}

#[test]
fn render_composition_master_is_additive_and_refuses_misuse() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]); // 3 frames
    let wav = temp_dir.path().join("score.wav");
    write_test_wav_at(&wav, 6144, &master_wav_samples(3, 1));
    let src = source_dir.to_string_lossy().to_string();

    let render = |spec: &serde_json::Value, name: &str| {
        let spec_path = temp_dir.path().join(format!("{name}.json"));
        write_chain_spec(&spec_path, &spec.to_string());
        let out = temp_dir.path().join(format!("{name}-out"));
        let assert = Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-composition",
                spec_path.to_string_lossy().as_ref(),
                out.to_string_lossy().as_ref(),
            ])
            .assert();
        (assert, out)
    };

    // Additive: a master present but not used by a scene leaves that scene
    // byte-identical to the same composition without any master.
    let plain_scene = serde_json::json!({
        "name": "solo", "duration_frames": 3, "input_dir": src,
        "chain": {"version": 1, "stages": [{"effect": "palette_quantize", "mode": "posterize", "levels": 4}]}
    });
    let with_master = serde_json::json!({
        "version": 1, "fps": 12, "master": {"audio": wav.to_string_lossy()}, "scenes": [plain_scene]
    });
    let no_master = serde_json::json!({ "version": 1, "fps": 12, "scenes": [plain_scene] });
    let (a, with_out) = render(&with_master, "with-master");
    a.success();
    let (b, no_out) = render(&no_master, "no-master");
    b.success();
    assert_png_frames_identical(
        &no_out.join("scene_01_solo/stage_01_palette_quantize"),
        &with_out.join("scene_01_solo/stage_01_palette_quantize"),
        3,
    );

    // Refusals (all pre-render → nothing written).
    let refuse = |spec: &serde_json::Value, name: &str, needle: &str| {
        let (assert, out) = render(spec, name);
        assert
            .failure()
            .stderr(predicate::str::contains(needle));
        assert!(!out.exists(), "{name}: nothing written on a refused spec");
    };
    let master_route_stage = serde_json::json!({
        "effect": "rutt_etra", "line_pitch": 2, "displacement_depth": 0.0,
        "line_thickness": 1, "mono": false,
        "modulation": {"routes": ["displacement_depth=master.audio-rms:40"]}
    });
    // A master route with no master declared.
    refuse(
        &serde_json::json!({"version": 1, "fps": 12, "scenes": [{
            "name": "a", "duration_frames": 3, "input_dir": src, "chain": {"version": 1, "stages": [master_route_stage]}
        }]}),
        "no-master-declared",
        "declares no \"master\"",
    );
    // A scene colliding with the reserved name.
    refuse(
        &serde_json::json!({"version": 1, "fps": 12, "master": {"audio": wav.to_string_lossy()}, "scenes": [{
            "name": "a", "duration_frames": 3, "input_dir": src, "chain": {"version": 1, "stages": [{
                "effect": "rutt_etra", "line_pitch": 2, "displacement_depth": 0.0, "line_thickness": 1, "mono": false,
                "modulation": {"routes": ["displacement_depth=master.audio-rms:40"],
                               "named_modulator_audio": [format!("master={}", wav.to_string_lossy())]}
            }]}
        }]}),
        "reserved-name",
        "reserved for the composition master clock",
    );
    // A video (frame-descriptor) master route — not supported yet.
    refuse(
        &serde_json::json!({"version": 1, "fps": 12, "master": {"audio": wav.to_string_lossy()}, "scenes": [{
            "name": "a", "duration_frames": 3, "input_dir": src, "chain": {"version": 1, "stages": [{
                "effect": "rutt_etra", "line_pitch": 2, "displacement_depth": 0.0, "line_thickness": 1, "mono": false,
                "modulation": {"routes": ["displacement_depth=master.luma:40"]}
            }]}
        }]}),
        "video-master",
        "video master (source_a) is not supported yet",
    );
}

#[test]
fn render_chain_same_spec_twice_is_byte_identical() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);
    let source_arg = source_dir.to_string_lossy().to_string();

    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(
        &spec_path,
        r#"{"version": 1, "stages": [
            {"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false},
            {"effect": "palette_quantize", "mode": "posterize", "levels": 4}
        ]}"#,
    );

    let run_1_dir = temp_dir.path().join("run-1");
    let run_2_dir = temp_dir.path().join("run-2");
    for output_dir in [&run_1_dir, &run_2_dir] {
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-chain",
                spec_path.to_string_lossy().as_ref(),
                source_arg.as_str(),
                output_dir.to_string_lossy().as_ref(),
            ])
            .assert()
            .success();
    }

    for relative in [
        "stage_01_rutt_etra/frame_000000.png",
        "stage_01_rutt_etra/frame_000001.png",
        "stage_01_rutt_etra/manifest.json",
        "stage_02_palette_quantize/frame_000000.png",
        "stage_02_palette_quantize/frame_000001.png",
        "chain-manifest.json",
    ] {
        assert_eq!(
            fs::read(run_1_dir.join(relative)).unwrap_or_else(|_| panic!("run 1 {relative}")),
            fs::read(run_2_dir.join(relative)).unwrap_or_else(|_| panic!("run 2 {relative}")),
            "two runs of the same chain spec must be byte-identical ({relative})"
        );
    }
}

/// The default-knob 2-stage feedback chain spec shared by the Slice-2 tests.
const FEEDBACK_CHAIN_SPEC: &str = r#"{"version": 1, "stages": [
    {"effect": "flow_feedback"},
    {"effect": "palette_quantize", "mode": "posterize", "levels": 4}
]}"#;

/// Every chain artifact of the 2-stage feedback chain on a 3-frame input,
/// for whole-output byte comparisons.
const FEEDBACK_CHAIN_ARTIFACTS: [&str; 12] = [
    "chain-record.json",
    "chain-manifest.json",
    "stage_01_flow_feedback/frames/frame_000000.png",
    "stage_01_flow_feedback/frames/frame_000001.png",
    "stage_01_flow_feedback/frames/frame_000002.png",
    "stage_01_flow_feedback/checkpoint.json",
    "stage_01_flow_feedback/manifest.json",
    "stage_01_flow_feedback/stage-complete.json",
    "stage_02_palette_quantize/frame_000000.png",
    "stage_02_palette_quantize/frame_000001.png",
    "stage_02_palette_quantize/frame_000002.png",
    "stage_02_palette_quantize/stage-complete.json",
];

fn run_chain(spec_path: &Path, input_dir: &Path, output_dir: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-chain",
            spec_path.to_string_lossy().as_ref(),
            input_dir.to_string_lossy().as_ref(),
            output_dir.to_string_lossy().as_ref(),
        ])
        .assert()
}

#[test]
fn render_chain_flow_feedback_stage_matches_manual_two_step_render() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    // The feedback effect needs motion in its input; translated texture.
    write_texture_sequence(&source_dir, &[0, 2, 4]);

    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(&spec_path, FEEDBACK_CHAIN_SPEC);
    let chain_dir = temp_dir.path().join("chain-out");
    run_chain(&spec_path, &source_dir, &chain_dir)
        .success()
        .stdout(predicate::str::contains(
            chain_dir
                .join("stage_02_palette_quantize")
                .display()
                .to_string(),
        ));

    // Manual step 1: the direct feedback render with the same (default)
    // knobs — the chain's stage input feeds both modulator and carrier.
    let manual_feedback_dir = temp_dir.path().join("manual-feedback");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            source_dir.to_string_lossy().as_ref(),
            source_dir.to_string_lossy().as_ref(),
            manual_feedback_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();

    // Manual step 2: the direct next-stage render on the feedback frames.
    let manual_quantize_dir = temp_dir.path().join("manual-quantize");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-palette-quantize-sequence",
            manual_feedback_dir
                .join("frames")
                .to_string_lossy()
                .as_ref(),
            manual_quantize_dir.to_string_lossy().as_ref(),
            "--mode",
            "posterize",
            "--levels",
            "4",
        ])
        .assert()
        .success();

    // The stage directory is byte-for-byte the standalone render (shared
    // handler, shared job id, checkpoint contract scoped inside the stage).
    for relative in [
        "frames/frame_000000.png",
        "frames/frame_000001.png",
        "frames/frame_000002.png",
        "checkpoint.json",
        "manifest.json",
    ] {
        assert_eq!(
            fs::read(chain_dir.join("stage_01_flow_feedback").join(relative))
                .unwrap_or_else(|_| panic!("chain stage 1 {relative}")),
            fs::read(manual_feedback_dir.join(relative))
                .unwrap_or_else(|_| panic!("manual feedback {relative}")),
            "feedback stage must be byte-identical to the direct render ({relative})"
        );
    }
    assert_png_frames_identical(
        &manual_quantize_dir,
        &chain_dir.join("stage_02_palette_quantize"),
        3,
    );

    // Chain-manifest records the feedback stage's derived algorithm id and
    // resolved (default) knobs.
    let manifest = read_json(&chain_dir.join("chain-manifest.json"));
    assert_eq!(manifest["stages"][0]["effect"], "flow_feedback");
    assert_eq!(manifest["stages"][0]["algorithm"], "flow_feedback_cpu_v2");
    assert_eq!(manifest["stages"][0]["settings"]["feedback_mix"], 0.72);
    assert_eq!(manifest["stages"][0]["settings"]["decay"], 0.995);
    assert_eq!(
        manifest["stages"][0]["settings"]["flow_source"],
        "optical_flow"
    );
}

#[test]
fn render_chain_resumes_interrupted_feedback_stage_to_byte_identity() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);

    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(&spec_path, FEEDBACK_CHAIN_SPEC);

    // Reference: one uninterrupted chain run.
    let reference_dir = temp_dir.path().join("reference-out");
    run_chain(&spec_path, &source_dir, &reference_dir).success();

    // Seed an interrupted stage 1: the direct CLI with --stop-after-frame
    // leaves a running checkpoint after frame 0 inside the stage directory
    // (the chain shares the direct command's job id, so the checkpoint
    // contract matches), plus the chain record the interrupted run would
    // have written before stage 1.
    let seeded_dir = temp_dir.path().join("seeded-out");
    fs::create_dir_all(&seeded_dir).expect("create seeded output dir");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            source_dir.to_string_lossy().as_ref(),
            source_dir.to_string_lossy().as_ref(),
            seeded_dir
                .join("stage_01_flow_feedback")
                .to_string_lossy()
                .as_ref(),
            "--stop-after-frame",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed flow-feedback sequence after frame 0",
        ));
    fs::copy(
        reference_dir.join("chain-record.json"),
        seeded_dir.join("chain-record.json"),
    )
    .expect("seed chain record");
    assert!(!seeded_dir
        .join("stage_01_flow_feedback/frames/frame_000001.png")
        .exists());

    // Re-running the chain resumes stage 1 from its checkpoint and renders
    // stage 2; the whole output is byte-identical to the uninterrupted run.
    run_chain(&spec_path, &source_dir, &seeded_dir).success();
    for relative in FEEDBACK_CHAIN_ARTIFACTS {
        assert_eq!(
            fs::read(seeded_dir.join(relative)).unwrap_or_else(|_| panic!("seeded {relative}")),
            fs::read(reference_dir.join(relative))
                .unwrap_or_else(|_| panic!("reference {relative}")),
            "resumed chain must be byte-identical to the uninterrupted run ({relative})"
        );
    }
}

#[test]
fn render_chain_rerun_of_completed_chain_is_a_clean_skip() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);

    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(&spec_path, FEEDBACK_CHAIN_SPEC);
    let output_dir = temp_dir.path().join("chain-out");
    run_chain(&spec_path, &source_dir, &output_dir).success();

    let before: Vec<Vec<u8>> = FEEDBACK_CHAIN_ARTIFACTS
        .iter()
        .map(|relative| {
            fs::read(output_dir.join(relative)).unwrap_or_else(|_| panic!("before {relative}"))
        })
        .collect();

    // Pinned semantics: a completed chain re-runs as a clean skip — every
    // stage reports "already complete", the summary still prints, exit 0,
    // and every artifact stays byte-identical.
    run_chain(&spec_path, &source_dir, &output_dir)
        .success()
        .stdout(predicate::str::contains(
            "stage 01 (flow_feedback) already complete — skipped",
        ))
        .stdout(predicate::str::contains(
            "stage 02 (palette_quantize) already complete — skipped",
        ))
        .stdout(predicate::str::contains("rendered chain with 2 stage(s)"));

    for (relative, expected) in FEEDBACK_CHAIN_ARTIFACTS.iter().zip(&before) {
        assert_eq!(
            &fs::read(output_dir.join(relative)).unwrap_or_else(|_| panic!("after {relative}")),
            expected,
            "clean skip must leave every artifact byte-identical ({relative})"
        );
    }
}

#[test]
fn render_chain_refuses_changed_spec_changed_input_and_unrecorded_dirty_dir() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);

    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(&spec_path, FEEDBACK_CHAIN_SPEC);
    let output_dir = temp_dir.path().join("chain-out");
    run_chain(&spec_path, &source_dir, &output_dir).success();
    let manifest_before =
        fs::read(output_dir.join("chain-manifest.json")).expect("manifest before");
    let frame_before = fs::read(output_dir.join("stage_02_palette_quantize/frame_000000.png"))
        .expect("frame before");

    // A changed spec against the recorded output refuses and touches nothing.
    let changed_spec_path = temp_dir.path().join("chain-changed.json");
    write_chain_spec(
        &changed_spec_path,
        r#"{"version": 1, "stages": [
            {"effect": "flow_feedback"},
            {"effect": "palette_quantize", "mode": "posterize", "levels": 8}
        ]}"#,
    );
    run_chain(&changed_spec_path, &source_dir, &output_dir)
        .failure()
        .stderr(predicate::str::contains(
            "a changed spec invalidates existing stage outputs",
        ));

    // Changed input frames (same spec) also refuse: skipping completed
    // stages assumes the recorded input fingerprint still holds.
    let other_source_dir = temp_dir.path().join("other-source-frames");
    write_texture_sequence(&other_source_dir, &[1, 3, 5]);
    run_chain(&spec_path, &other_source_dir, &output_dir)
        .failure()
        .stderr(predicate::str::contains("input frames changed"));

    assert_eq!(
        fs::read(output_dir.join("chain-manifest.json")).expect("manifest after"),
        manifest_before,
        "refusal must leave the chain manifest untouched"
    );
    assert_eq!(
        fs::read(output_dir.join("stage_02_palette_quantize/frame_000000.png"))
            .expect("frame after"),
        frame_before,
        "refusal must leave stage frames untouched"
    );

    // A non-empty output directory without a chain record refuses too.
    let dirty_dir = temp_dir.path().join("dirty-out");
    fs::create_dir_all(&dirty_dir).expect("create dirty dir");
    fs::write(dirty_dir.join("notes.txt"), "not chain output").expect("write stray file");
    run_chain(&spec_path, &source_dir, &dirty_dir)
        .failure()
        .stderr(predicate::str::contains("no chain-record.json"));
    assert_eq!(
        fs::read_dir(&dirty_dir).expect("read dirty dir").count(),
        1,
        "refusal must write nothing into the unrecorded directory"
    );
}

#[test]
fn render_chain_rejects_invalid_flow_feedback_stage() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);

    let run_case = |name: &str, spec_json: &str, expected_stderr: &str| {
        let spec_path = temp_dir.path().join(format!("{name}.json"));
        write_chain_spec(&spec_path, spec_json);
        let output_dir = temp_dir.path().join(format!("{name}-out"));
        run_chain(&spec_path, &source_dir, &output_dir)
            .failure()
            .stderr(predicate::str::contains(expected_stderr));
        assert!(
            !output_dir.exists(),
            "{name}: nothing must be written on rejection"
        );
    };

    run_case(
        "bad-mix",
        r#"{"version": 1, "stages": [{"effect": "flow_feedback", "feedback_mix": 2.0}]}"#,
        "feedback_mix must be between zero and one",
    );
    run_case(
        "bad-iterations",
        r#"{"version": 1, "stages": [{"effect": "flow_feedback", "iterations": 2}]}"#,
        "exactly one iteration",
    );
    run_case(
        "unknown-field",
        r#"{"version": 1, "stages": [{"effect": "flow_feedback", "bogus_knob": 1.0}]}"#,
        "unknown field `bogus_knob`",
    );
    // A stage-2 feedback typo after a valid stage 1 must leave nothing.
    run_case(
        "bad-stage-2",
        r#"{"version": 1, "stages": [
            {"effect": "palette_quantize", "levels": 4},
            {"effect": "flow_feedback", "decay": -1.0}
        ]}"#,
        "decay must be greater than or equal to zero",
    );
}

#[test]
fn render_chain_stage_lfo_modulation_matches_direct_render() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    // Static input (identical frames): the LFO is the only source of change.
    write_texture_sequence(&source_dir, &[0, 0, 0]);

    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(
        &spec_path,
        r#"{"version": 1, "stages": [
            {"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0,
             "line_thickness": 1, "mono": false,
             "modulation": {"routes": ["displacement_depth=lfo(sine,0.5):64"]}},
            {"effect": "palette_quantize", "mode": "posterize", "levels": 4}
        ]}"#,
    );
    let chain_dir = temp_dir.path().join("chain-out");
    run_chain(&spec_path, &source_dir, &chain_dir).success();

    // The modulated stage is byte-identical to the direct CLI render with
    // the same route (a pure-LFO route set needs no --modulator-* flags).
    let direct_dir = temp_dir.path().join("direct-rutt-etra");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_dir.to_string_lossy().as_ref(),
            direct_dir.to_string_lossy().as_ref(),
            "--line-pitch",
            "4",
            "--displacement-depth",
            "6.0",
            "--modulate",
            "displacement_depth=lfo(sine,0.5):64",
            "--modulation-fps",
            "12",
        ])
        .assert()
        .success();
    for relative in [
        "frame_000000.png",
        "frame_000001.png",
        "frame_000002.png",
        "manifest.json",
    ] {
        assert_eq!(
            fs::read(chain_dir.join("stage_01_rutt_etra").join(relative))
                .unwrap_or_else(|_| panic!("chain {relative}")),
            fs::read(direct_dir.join(relative)).unwrap_or_else(|_| panic!("direct {relative}")),
            "modulated chain stage must be byte-identical to the direct render ({relative})"
        );
    }

    // The chain manifest records the stage's modulation block; the
    // unmodulated stage's settings carry no modulation key at all (the
    // pre-slice marker/manifest shape, skip-when-absent).
    let manifest = read_json(&chain_dir.join("chain-manifest.json"));
    assert_eq!(
        manifest["stages"][0]["settings"]["modulation"]["routes"][0],
        "displacement_depth=lfo(sine,0.5):64"
    );
    assert!(manifest["stages"][1]["settings"]
        .as_object()
        .expect("stage 2 settings object")
        .get("modulation")
        .is_none());
    let marker = read_json(&chain_dir.join("stage_02_palette_quantize/stage-complete.json"));
    assert!(marker["settings"]
        .as_object()
        .expect("marker settings object")
        .get("modulation")
        .is_none());
}

#[test]
fn render_chain_rejects_invalid_stage_modulation() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);

    let run_case = |name: &str, spec_json: &str, expected_error: &str| {
        let spec_path = temp_dir.path().join(format!("chain-{name}.json"));
        write_chain_spec(&spec_path, spec_json);
        let output_dir = temp_dir.path().join(format!("out-{name}"));
        run_chain(&spec_path, &source_dir, &output_dir)
            .failure()
            .stderr(predicate::str::contains(expected_error));
        assert!(
            !output_dir.exists(),
            "rejected modulation ({name}) must leave nothing on disk"
        );
    };

    // Unknown target for the stage's effect (apply-fn probe), in stage 2 so
    // a valid stage 1 can't render first.
    run_case(
        "unknown-target",
        r#"{"version": 1, "stages": [
            {"effect": "palette_quantize", "levels": 4},
            {"effect": "rutt_etra", "modulation": {"routes": ["mono=luma:1"]}}
        ]}"#,
        "unknown rutt-etra modulation target",
    );
    // A media-sourced route with no media declared on the stage.
    run_case(
        "missing-media",
        r#"{"version": 1, "stages": [
            {"effect": "rutt_etra",
             "modulation": {"routes": ["displacement_depth=audio-rms:64"]}}
        ]}"#,
        "requires a modulation.modulator_audio path on this stage",
    );
    // The feedback envelope base is the pinned frame rate, not a free knob.
    run_case(
        "feedback-fps",
        r#"{"version": 1, "stages": [
            {"effect": "flow_feedback",
             "modulation": {"routes": ["feedback_mix=lfo(sine,1):0.5"], "fps": 6.0}}
        ]}"#,
        "sample against its pinned frame rate",
    );
}

#[test]
fn render_chain_modulated_feedback_stage_checkpoint_carries_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);

    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(
        &spec_path,
        r#"{"version": 1, "stages": [
            {"effect": "flow_feedback",
             "modulation": {"routes": ["feedback_mix=lfo(saw,1,0.25):0.5,0.25"]}}
        ]}"#,
    );
    let chain_dir = temp_dir.path().join("chain-out");
    run_chain(&spec_path, &source_dir, &chain_dir).success();

    // The LFO route rides the stage's checkpoint contract exactly as it does
    // standalone; the envelope base is the pinned chain frame rate.
    let checkpoint = read_json(&chain_dir.join("stage_01_flow_feedback/checkpoint.json"));
    let modulation = &checkpoint["contract"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "feedback_mix");
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["shape"], "saw");
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["rate_hz"], 1.0);
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["phase"], 0.25);
    assert_eq!(modulation["fps"], 12.0);
    assert!(modulation["modulator_audio"].is_null());
    assert!(modulation["modulator_frames"].is_null());
}

#[test]
fn queue_chain_add_run_matches_direct_and_validates_at_add() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 0, 0]);
    let queue_path = temp_dir.path().join("queue.json");

    // Add-time validation is the whole-spec gate: an unknown modulation
    // target rejects and persists no queue file.
    let bad_spec_path = temp_dir.path().join("bad-chain.json");
    write_chain_spec(
        &bad_spec_path,
        r#"{"version": 1, "stages": [
            {"effect": "rutt_etra", "modulation": {"routes": ["mono=luma:1"]}}
        ]}"#,
    );
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-chain",
            queue_path.to_string_lossy().as_ref(),
            bad_spec_path.to_string_lossy().as_ref(),
            source_dir.to_string_lossy().as_ref(),
            temp_dir
                .path()
                .join("queued-out")
                .to_string_lossy()
                .as_ref(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown rutt-etra modulation target",
        ));
    assert!(
        !queue_path.exists(),
        "rejected chain spec must persist no queue file"
    );

    // A modulated chain (LFO — no media) queues, and the persisted task
    // records the resolved spec document.
    let spec_path = temp_dir.path().join("chain.json");
    write_chain_spec(
        &spec_path,
        r#"{"version": 1, "stages": [
            {"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0,
             "modulation": {"routes": ["displacement_depth=lfo(sine,0.5):64"]}},
            {"effect": "palette_quantize", "mode": "posterize", "levels": 4}
        ]}"#,
    );
    let queued_root = temp_dir.path().join("queued-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-chain",
            queue_path.to_string_lossy().as_ref(),
            spec_path.to_string_lossy().as_ref(),
            source_dir.to_string_lossy().as_ref(),
            queued_root.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();
    let queue_json = read_json(&queue_path);
    let task = &queue_json["jobs"][0]["task"];
    assert_eq!(task["type"], "render_chain");
    assert_eq!(task["spec"]["version"], 1);
    assert_eq!(task["spec"]["stages"][0]["effect"], "rutt_etra");
    // Resolved defaults are filled in the persisted document.
    assert_eq!(task["spec"]["stages"][0]["line_thickness"], 1);
    assert_eq!(
        task["spec"]["stages"][0]["modulation"]["routes"][0],
        "displacement_depth=lfo(sine,0.5):64"
    );

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-chain", queue_path.to_string_lossy().as_ref()])
        .assert()
        .success();

    // The queued run is byte-identical to the direct render-chain run —
    // every chain artifact, including record, markers, and manifests.
    let direct_dir = temp_dir.path().join("direct-out");
    run_chain(&spec_path, &source_dir, &direct_dir).success();
    for relative in [
        "chain-record.json",
        "chain-manifest.json",
        "stage_01_rutt_etra/frame_000000.png",
        "stage_01_rutt_etra/frame_000002.png",
        "stage_01_rutt_etra/manifest.json",
        "stage_01_rutt_etra/stage-complete.json",
        "stage_02_palette_quantize/frame_000000.png",
        "stage_02_palette_quantize/frame_000002.png",
        "stage_02_palette_quantize/stage-complete.json",
    ] {
        assert_eq!(
            fs::read(queued_root.join("job-0001").join(relative))
                .unwrap_or_else(|_| panic!("queued {relative}")),
            fs::read(direct_dir.join(relative)).unwrap_or_else(|_| panic!("direct {relative}")),
            "queued chain must be byte-identical to the direct run ({relative})"
        );
    }

    // The completed job records the final stage's frames.
    let finished = read_json(&queue_path);
    assert_eq!(finished["jobs"][0]["status"], "complete");
    assert_eq!(
        finished["jobs"][0]["output"]["frame_paths"][0],
        "stage_02_palette_quantize/frame_000000.png"
    );
}

#[test]
fn queue_composition_add_run_matches_direct_and_validates_at_add() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]); // 3 frames
    let src = source_dir.to_string_lossy().to_string();
    let queue_path = temp_dir.path().join("queue.json");

    // Add-time validation is the whole-spec gate: a master route with no master
    // declared rejects and persists no queue file.
    let bad_spec_path = temp_dir.path().join("bad.json");
    write_chain_spec(
        &bad_spec_path,
        &serde_json::json!({
            "version": 1, "fps": 12, "scenes": [{
                "name": "a", "duration_frames": 3, "input_dir": src,
                "chain": {"version": 1, "stages": [{
                    "effect": "rutt_etra", "line_pitch": 2, "displacement_depth": 0.0,
                    "line_thickness": 1, "mono": false,
                    "modulation": {"routes": ["displacement_depth=master.audio-rms:40"]}
                }]}
            }]
        })
        .to_string(),
    );
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-composition",
            queue_path.to_string_lossy().as_ref(),
            bad_spec_path.to_string_lossy().as_ref(),
            temp_dir.path().join("queued-out").to_string_lossy().as_ref(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("declares no \"master\""));
    assert!(
        !queue_path.exists(),
        "rejected composition spec must persist no queue file"
    );

    // A two-scene composition (cut) queues; the task records the resolved spec.
    let spec = serde_json::json!({
        "version": 1, "fps": 12, "scenes": [
            {"name": "a", "duration_frames": 3, "input_dir": src,
             "chain": {"version": 1, "stages": [{"effect": "palette_quantize", "mode": "posterize", "levels": 4}]},
             "transition_out": {"type": "cut"}},
            {"name": "b", "duration_frames": 3, "input_dir": src,
             "chain": {"version": 1, "stages": [{"effect": "rutt_etra", "line_pitch": 4, "displacement_depth": 6.0, "line_thickness": 1, "mono": false}]}}
        ]
    });
    let spec_path = temp_dir.path().join("composition.json");
    write_chain_spec(&spec_path, &spec.to_string());
    let queued_root = temp_dir.path().join("queued-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-composition",
            queue_path.to_string_lossy().as_ref(),
            spec_path.to_string_lossy().as_ref(),
            queued_root.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();
    let task = read_json(&queue_path)["jobs"][0]["task"].clone();
    assert_eq!(task["type"], "render_composition");
    assert_eq!(task["spec"]["version"], 1);
    assert_eq!(task["spec"]["fps"], 12.0);
    assert_eq!(task["spec"]["scenes"][0]["name"], "a");
    // A resolved default is filled in the persisted document.
    assert_eq!(task["spec"]["scenes"][0]["chain"]["stages"][0]["levels"], 4);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-composition", queue_path.to_string_lossy().as_ref()])
        .assert()
        .success();

    // The queued run is byte-identical to a direct render-composition — every
    // artifact: assembled frames, both manifests, the record, scene renders.
    let direct_dir = temp_dir.path().join("direct-out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-composition",
            spec_path.to_string_lossy().as_ref(),
            direct_dir.to_string_lossy().as_ref(),
        ])
        .assert()
        .success();
    for relative in [
        "composition-manifest.json",
        "composition-record.json",
        "frames/frame_000000.png",
        "frames/frame_000005.png",
        "scene_01_a/chain-manifest.json",
        "scene_02_b/stage_01_rutt_etra/frame_000000.png",
    ] {
        assert_eq!(
            fs::read(queued_root.join("job-0001").join(relative))
                .unwrap_or_else(|_| panic!("queued {relative}")),
            fs::read(direct_dir.join(relative)).unwrap_or_else(|_| panic!("direct {relative}")),
            "queued composition must be byte-identical to the direct run ({relative})"
        );
    }

    let finished = read_json(&queue_path);
    assert_eq!(finished["jobs"][0]["status"], "complete");
    assert_eq!(
        finished["jobs"][0]["output"]["frame_paths"][0],
        "frames/frame_000000.png"
    );
    assert_eq!(finished["jobs"][0]["output"]["frame_paths"][5], "frames/frame_000005.png");
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
fn render_showcase_writes_preview_bundle_without_requiring_ffmpeg() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator");
    let carrier_dir = temp_dir.path().join("carrier");
    let output_dir = temp_dir.path().join("showcase");
    write_texture_sequence(&modulator_dir, &[0, 1]);
    write_texture_sequence(&carrier_dir, &[2, 3]);

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-showcase",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--frames-per-effect",
            "2",
            "--frame-rate",
            "12",
            "--granular-grain-size",
            "8",
            "--seed",
            "7",
            "--no-mp4",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("rendered destructive showcase"));

    assert!(output_dir.join("showcase.json").exists());
    assert!(output_dir.join("contact_sheet.png").exists());
    assert!(output_dir.join("stills/01_flow_displace.png").exists());
    assert!(output_dir.join("stills/04_vector_datamosh.png").exists());
    for index in 0..8 {
        assert!(output_dir
            .join("frames")
            .join(format!("frame_{index:06}.png"))
            .exists());
    }
    assert!(!output_dir.join("showcase.mp4").exists());
}

#[test]
fn feedback_iterations_are_rejected_by_cli_contract() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator");
    let carrier_dir = temp_dir.path().join("carrier");
    let output_dir = temp_dir.path().join("feedback");
    write_texture_sequence(&modulator_dir, &[0, 1]);
    write_texture_sequence(&carrier_dir, &[2, 3]);

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--iterations",
            "2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("supports exactly one iteration"));
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
    write_test_wav(
        &modulator_wav,
        &[0.0, 0.5, -0.5, 1.0, -1.0, 0.25, -0.25, 0.75],
    );
    write_test_wav(&carrier_wav, &[1.0, -1.0, 0.5, -0.5, 0.0, 0.8, -0.8, 0.2]);
    let modulator_rms = temp_dir.path().join("mod-rms.json");
    let carrier_rms = temp_dir.path().join("car-rms.json");
    for (wav, json) in [
        (&modulator_wav, &modulator_rms),
        (&carrier_wav, &carrier_rms),
    ] {
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
fn render_feedback_sequence_modulated_routes_join_checkpoint_contract() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");

    // Three identical source frames; the per-frame variation comes from the
    // WAV's RMS ramp driving the routed knob.
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        for dir in [&modulator_dir, &carrier_dir] {
            let frame_arg = dir.join(frame_name).to_string_lossy().to_string();
            Command::cargo_bin("morphogen")
                .expect("morphogen binary")
                .args(["render-test", frame_arg.as_str()])
                .assert()
                .success();
        }
    }

    // A 0.75 s quiet→loud amplitude ramp at 8192 Hz: the RMS envelope rises
    // across the three output frames (frame-rate 4), so the routed
    // feedback_mix varies per frame — and frame N's state consumed frames
    // 0..N's values, which is what resume must reproduce.
    let modulator_wav = temp_dir.path().join("ramp.wav");
    let ramp: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&modulator_wav, 8192, &ramp);

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let wav_arg = modulator_wav.to_string_lossy().to_string();
    let route = "feedback_mix=audio-rms:0.5,0.25";
    let base_args = |output_dir: &str| {
        vec![
            "render-feedback-sequence".to_string(),
            modulator_arg.clone(),
            carrier_arg.clone(),
            output_dir.to_string(),
            "--carrier-amount".to_string(),
            "8".to_string(),
            "--feedback-amount".to_string(),
            "12".to_string(),
            "--feedback-mix".to_string(),
            "0.7".to_string(),
            "--decay".to_string(),
            "0.95".to_string(),
            "--max-frames".to_string(),
            "3".to_string(),
            "--frame-rate".to_string(),
            "4".to_string(),
            "--flow-source".to_string(),
            "luminance".to_string(),
        ]
    };
    let modulated_args = |output_dir: &str| {
        let mut args = base_args(output_dir);
        args.extend([
            "--modulate".to_string(),
            route.to_string(),
            "--modulator-audio".to_string(),
            wav_arg.clone(),
        ]);
        args
    };

    // Unmodulated reference (the off case) and the uninterrupted modulated
    // render the resumed one must match.
    let off_dir = temp_dir.path().join("off-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&off_dir.to_string_lossy()))
        .assert()
        .success();
    let uninterrupted_dir = temp_dir.path().join("uninterrupted-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(modulated_args(&uninterrupted_dir.to_string_lossy()))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "modulation routes: feedback_mix=audio-rms:0.5,0.25",
        ));

    // The route actually drives the state evolution.
    assert_ne!(
        fs::read(uninterrupted_dir.join("frames/frame_000002.png")).expect("modulated frame"),
        fs::read(off_dir.join("frames/frame_000002.png")).expect("unmodulated frame"),
        "routed feedback_mix must change the rendered sequence"
    );

    // Interrupt after frame 0, then resume with identical arguments: the
    // milestone's acceptance test is byte-identity with the uninterrupted
    // render (the envelope re-samples at the same absolute frame indices).
    let resumed_dir = temp_dir.path().join("resumed-output");
    let resumed_args = modulated_args(&resumed_dir.to_string_lossy());
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .arg("--stop-after-frame")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed flow-feedback sequence after frame 0",
        ));

    // The checkpoint's contract carries the modulation block: routes in CLI
    // order, sampling, envelope fps, and a content fingerprint of the
    // modulator WAV (no frames modulator was used).
    let checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(resumed_dir.join("checkpoint.json")).expect("read checkpoint"),
    )
    .expect("parse checkpoint");
    let modulation = &checkpoint["contract"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "feedback_mix");
    assert_eq!(modulation["routes"][0]["source"], "audio-rms");
    // Exactly-representable f32 literals so the JSON round-trip compares clean.
    assert_eq!(modulation["routes"][0]["scale"], 0.5);
    assert_eq!(modulation["routes"][0]["offset"], 0.25);
    assert_eq!(modulation["sampling"], "hold");
    assert_eq!(modulation["fps"], 4.0);
    assert_eq!(modulation["modulator_audio"]["path"], wav_arg.as_str());
    assert!(modulation["modulator_audio"]["checksum"]
        .as_str()
        .expect("audio checksum")
        .starts_with("fnv1a64:"));
    assert!(modulation["modulator_frames"].is_null());

    // A changed route must refuse to resume — the knob history would differ.
    let mut changed_route_args = base_args(&resumed_dir.to_string_lossy());
    changed_route_args.extend([
        "--modulate".to_string(),
        "feedback_mix=audio-rms:0.75,0.25".to_string(),
        "--modulator-audio".to_string(),
        wav_arg.clone(),
    ]);
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&changed_route_args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));
    // Dropping the routes entirely must refuse for the same reason.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&resumed_dir.to_string_lossy()))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered flow-feedback sequence with 3 frame(s)",
        ));
    for frame_name in ["frame_000000.png", "frame_000001.png", "frame_000002.png"] {
        assert_eq!(
            fs::read(resumed_dir.join("frames").join(frame_name)).expect("resumed frame"),
            fs::read(uninterrupted_dir.join("frames").join(frame_name))
                .expect("uninterrupted frame"),
            "resumed modulated render must be byte-identical ({frame_name})"
        );
    }

    // A pre-slice checkpoint (no modulation key at all) deserializes as
    // unmodulated and stays resumable by an unmodulated render.
    let legacy_dir = temp_dir.path().join("legacy-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&legacy_dir.to_string_lossy()))
        .arg("--stop-after-frame")
        .assert()
        .success();
    let checkpoint_path = legacy_dir.join("checkpoint.json");
    let mut legacy_checkpoint: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&checkpoint_path).expect("read legacy"))
            .expect("parse legacy");
    let contract = legacy_checkpoint["contract"]
        .as_object_mut()
        .expect("contract object");
    assert!(contract.remove("modulation").is_some());
    fs::write(
        &checkpoint_path,
        serde_json::to_string(&legacy_checkpoint).expect("serialize legacy"),
    )
    .expect("write legacy checkpoint");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&legacy_dir.to_string_lossy()))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered flow-feedback sequence with 3 frame(s)",
        ));
}

#[test]
fn render_feedback_sequence_lfo_route_joins_checkpoint_contract() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");

    // Three identical source frames; the per-frame variation comes from the
    // LFO alone — no modulator media of any kind exists in this test.
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
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
    // saw at 1 Hz, phase 0.25, fps 4: p = 0.25, 0.5, 0.75 across the three
    // frames — a distinct routed feedback_mix per frame, so frame N's state
    // consumed frames 0..N's values (what resume must reproduce). All
    // literals are exactly representable in f32.
    let route = "feedback_mix=lfo(saw,1,0.25):0.5,0.25";
    let base_args = |output_dir: &str| {
        vec![
            "render-feedback-sequence".to_string(),
            modulator_arg.clone(),
            carrier_arg.clone(),
            output_dir.to_string(),
            "--carrier-amount".to_string(),
            "8".to_string(),
            "--feedback-amount".to_string(),
            "12".to_string(),
            "--feedback-mix".to_string(),
            "0.7".to_string(),
            "--decay".to_string(),
            "0.95".to_string(),
            "--max-frames".to_string(),
            "3".to_string(),
            "--frame-rate".to_string(),
            "4".to_string(),
            "--flow-source".to_string(),
            "luminance".to_string(),
        ]
    };
    let modulated_args = |output_dir: &str| {
        let mut args = base_args(output_dir);
        args.extend(["--modulate".to_string(), route.to_string()]);
        args
    };

    // Unmodulated reference (the off case) and the uninterrupted modulated
    // render the resumed one must match.
    let off_dir = temp_dir.path().join("off-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&off_dir.to_string_lossy()))
        .assert()
        .success();
    let uninterrupted_dir = temp_dir.path().join("uninterrupted-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(modulated_args(&uninterrupted_dir.to_string_lossy()))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "modulation routes: feedback_mix=lfo(saw,1,0.25):0.5,0.25",
        ));

    // The route actually drives the state evolution.
    assert_ne!(
        fs::read(uninterrupted_dir.join("frames/frame_000002.png")).expect("modulated frame"),
        fs::read(off_dir.join("frames/frame_000002.png")).expect("unmodulated frame"),
        "routed LFO feedback_mix must change the rendered sequence"
    );

    // Interrupt after frame 0, then resume with identical arguments.
    let resumed_dir = temp_dir.path().join("resumed-output");
    let resumed_args = modulated_args(&resumed_dir.to_string_lossy());
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .arg("--stop-after-frame")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed flow-feedback sequence after frame 0",
        ));

    // The LFO params ride the route inside the checkpoint's modulation block
    // (no new contract fields); no media fingerprints exist to record.
    let checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(resumed_dir.join("checkpoint.json")).expect("read checkpoint"),
    )
    .expect("parse checkpoint");
    let modulation = &checkpoint["contract"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "feedback_mix");
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["shape"], "saw");
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["rate_hz"], 1.0);
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["phase"], 0.25);
    assert_eq!(modulation["routes"][0]["scale"], 0.5);
    assert_eq!(modulation["routes"][0]["offset"], 0.25);
    assert_eq!(modulation["fps"], 4.0);
    assert!(modulation["modulator_audio"].is_null());
    assert!(modulation["modulator_frames"].is_null());

    // A changed rate_hz must refuse to resume — the knob history would
    // differ (the existing contract-equality path, no new fields).
    let mut changed_rate_args = base_args(&resumed_dir.to_string_lossy());
    changed_rate_args.extend([
        "--modulate".to_string(),
        "feedback_mix=lfo(saw,2,0.25):0.5,0.25".to_string(),
    ]);
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&changed_rate_args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));
    // A changed shape must refuse for the same reason.
    let mut changed_shape_args = base_args(&resumed_dir.to_string_lossy());
    changed_shape_args.extend([
        "--modulate".to_string(),
        "feedback_mix=lfo(sine,1,0.25):0.5,0.25".to_string(),
    ]);
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&changed_shape_args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));

    // Identical arguments resume to byte-identity with uninterrupted.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered flow-feedback sequence with 3 frame(s)",
        ));
    for frame_name in ["frame_000000.png", "frame_000001.png", "frame_000002.png"] {
        assert_eq!(
            fs::read(resumed_dir.join("frames").join(frame_name)).expect("resumed frame"),
            fs::read(uninterrupted_dir.join("frames").join(frame_name))
                .expect("uninterrupted frame"),
            "resumed LFO-modulated render must be byte-identical ({frame_name})"
        );
    }

    // A legacy checkpoint (no modulation block at all) still deserializes
    // and resumes after the Lfo variant landed on the route source type.
    let legacy_dir = temp_dir.path().join("legacy-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&legacy_dir.to_string_lossy()))
        .arg("--stop-after-frame")
        .assert()
        .success();
    let checkpoint_path = legacy_dir.join("checkpoint.json");
    let mut legacy_checkpoint: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&checkpoint_path).expect("read legacy"))
            .expect("parse legacy");
    let contract = legacy_checkpoint["contract"]
        .as_object_mut()
        .expect("contract object");
    assert!(contract.remove("modulation").is_some());
    fs::write(
        &checkpoint_path,
        serde_json::to_string(&legacy_checkpoint).expect("serialize legacy"),
    )
    .expect("write legacy checkpoint");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&legacy_dir.to_string_lossy()))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered flow-feedback sequence with 3 frame(s)",
        ));
}

#[test]
fn render_datamosh_sequence_reuses_flow_sidecars_and_resumes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let resumed_output_dir = temp_dir.path().join("resumed-output");
    let uninterrupted_output_dir = temp_dir.path().join("uninterrupted-output");
    let flow_cache_dir = temp_dir.path().join("datamosh-flow-cache");

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
    let flow_cache_arg = flow_cache_dir.to_string_lossy().to_string();
    let datamosh_args = [
        "render-datamosh-sequence",
        modulator_arg.as_str(),
        carrier_arg.as_str(),
        resumed_arg.as_str(),
        "--keyframe-interval",
        "0",
        "--amount",
        "1",
        "--block-size",
        "16",
        "--residual-gain",
        "0.5",
        "--residual-decay",
        "0.8",
        "--flow-cache-dir",
        flow_cache_arg.as_str(),
        "--max-frames",
        "3",
    ];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(datamosh_args)
        .arg("--stop-after-frame")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed datamosh sequence after frame 0",
        ));

    let partial_checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(resumed_output_dir.join("checkpoint.json"))
            .expect("read partial datamosh checkpoint"),
    )
    .expect("parse partial datamosh checkpoint");
    assert_eq!(partial_checkpoint["task"], "frame_sequence_datamosh");
    assert_eq!(partial_checkpoint["status"], "running");
    assert_eq!(partial_checkpoint["next_frame_index"], 1);
    assert!(resumed_output_dir
        .join("state/datamosh_output_frame_000000.rgba32f")
        .exists());
    assert!(resumed_output_dir.join("frame_000000.png").exists());
    assert!(!flow_cache_dir.join("frame_000001/manifest.json").exists());

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(datamosh_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered datamosh sequence with 3 frame(s)",
        ));

    assert!(flow_cache_dir.join("frame_000001/manifest.json").exists());
    assert!(flow_cache_dir
        .join("frame_000001/frame_000000.flowf32")
        .exists());
    assert!(flow_cache_dir.join("frame_000002/manifest.json").exists());

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-datamosh-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            uninterrupted_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--block-size",
            "16",
            "--residual-gain",
            "0.5",
            "--residual-decay",
            "0.8",
            "--flow-cache-dir",
            flow_cache_arg.as_str(),
            "--max-frames",
            "3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "reused 2 and generated 0 datamosh optical-flow cache frame(s)",
        ));

    let final_checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(resumed_output_dir.join("checkpoint.json"))
            .expect("read final datamosh checkpoint"),
    )
    .expect("parse final datamosh checkpoint");
    assert_eq!(final_checkpoint["status"], "complete");
    assert_eq!(final_checkpoint["next_frame_index"], 3);
    assert_eq!(final_checkpoint["contract"]["settings"]["preset"], "custom");
    assert_eq!(
        final_checkpoint["provenance"]["analysis_caches"][0]["path"],
        flow_cache_arg
    );

    for frame in ["frame_000000.png", "frame_000001.png", "frame_000002.png"] {
        assert_eq!(
            fs::read(resumed_output_dir.join(frame)).expect("resumed datamosh frame"),
            fs::read(uninterrupted_output_dir.join(frame)).expect("uninterrupted datamosh frame"),
            "resumed datamosh output must match uninterrupted render ({frame})"
        );
    }
}

#[test]
fn render_datamosh_sequence_modulated_routes_join_checkpoint_contract() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");

    // A translating modulator so the Lucas-Kanade flow is non-zero — a routed
    // `amount` can only change the output where there is motion to scale.
    write_texture_sequence(&modulator_dir, &[0, 2, 4]);
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        write_horizontal_carrier(&carrier_dir.join(frame_name), 24, 16);
    }

    // A 0.75 s quiet→loud amplitude ramp at 8192 Hz: at modulation-fps 4 the
    // RMS envelope rises across the three output frames, so the routed amount
    // varies per frame — and frame N's held output consumed frames 0..N's
    // values, which is what resume must reproduce.
    let modulator_wav = temp_dir.path().join("ramp.wav");
    let ramp: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&modulator_wav, 8192, &ramp);

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let wav_arg = modulator_wav.to_string_lossy().to_string();
    let route = "amount=audio-rms:0.5,0.25";
    let base_args = |output_dir: &str| {
        vec![
            "render-datamosh-sequence".to_string(),
            modulator_arg.clone(),
            carrier_arg.clone(),
            output_dir.to_string(),
            "--keyframe-interval".to_string(),
            "0".to_string(),
            "--amount".to_string(),
            "1".to_string(),
            "--max-frames".to_string(),
            "3".to_string(),
            "--modulation-fps".to_string(),
            "4".to_string(),
        ]
    };
    let modulated_args = |output_dir: &str| {
        let mut args = base_args(output_dir);
        args.extend([
            "--modulate".to_string(),
            route.to_string(),
            "--modulator-audio".to_string(),
            wav_arg.clone(),
        ]);
        args
    };

    // Unmodulated reference (the off case) and the uninterrupted modulated
    // render the resumed one must match.
    let off_dir = temp_dir.path().join("off-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&off_dir.to_string_lossy()))
        .assert()
        .success();
    let uninterrupted_dir = temp_dir.path().join("uninterrupted-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(modulated_args(&uninterrupted_dir.to_string_lossy()))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "modulation routes: amount=audio-rms:0.5,0.25",
        ));

    // The route actually drives the state evolution (frame 0 is the carrier
    // verbatim in both, so compare a P-frame).
    assert_ne!(
        fs::read(uninterrupted_dir.join("frame_000002.png")).expect("modulated frame"),
        fs::read(off_dir.join("frame_000002.png")).expect("unmodulated frame"),
        "routed amount must change the rendered sequence"
    );

    // Interrupt after frame 0, then resume with identical arguments: the
    // milestone's acceptance test is byte-identity with the uninterrupted
    // render (the envelope re-samples at the same absolute frame indices).
    let resumed_dir = temp_dir.path().join("resumed-output");
    let resumed_args = modulated_args(&resumed_dir.to_string_lossy());
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .arg("--stop-after-frame")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed datamosh sequence after frame 0",
        ));

    // The checkpoint's contract carries the modulation block: routes in CLI
    // order, sampling, envelope fps, and a content fingerprint of the
    // modulator WAV (no frames modulator was used).
    let checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(resumed_dir.join("checkpoint.json")).expect("read checkpoint"),
    )
    .expect("parse checkpoint");
    let modulation = &checkpoint["contract"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "amount");
    assert_eq!(modulation["routes"][0]["source"], "audio-rms");
    // Exactly-representable f32 literals so the JSON round-trip compares clean.
    assert_eq!(modulation["routes"][0]["scale"], 0.5);
    assert_eq!(modulation["routes"][0]["offset"], 0.25);
    assert_eq!(modulation["sampling"], "hold");
    assert_eq!(modulation["fps"], 4.0);
    assert_eq!(modulation["modulator_audio"]["path"], wav_arg.as_str());
    assert!(modulation["modulator_audio"]["checksum"]
        .as_str()
        .expect("audio checksum")
        .starts_with("fnv1a64:"));
    assert!(modulation["modulator_frames"].is_null());

    // A changed route must refuse to resume — the knob history would differ.
    let mut changed_route_args = base_args(&resumed_dir.to_string_lossy());
    changed_route_args.extend([
        "--modulate".to_string(),
        "amount=audio-rms:0.75,0.25".to_string(),
        "--modulator-audio".to_string(),
        wav_arg.clone(),
    ]);
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&changed_route_args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));
    // Dropping the routes entirely must refuse for the same reason.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&resumed_dir.to_string_lossy()))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered datamosh sequence with 3 frame(s)",
        ));
    for frame_name in ["frame_000000.png", "frame_000001.png", "frame_000002.png"] {
        assert_eq!(
            fs::read(resumed_dir.join(frame_name)).expect("resumed frame"),
            fs::read(uninterrupted_dir.join(frame_name)).expect("uninterrupted frame"),
            "resumed modulated datamosh render must be byte-identical ({frame_name})"
        );
    }

    // A pre-slice checkpoint (no modulation key at all) deserializes as
    // unmodulated and stays resumable by an unmodulated render.
    let legacy_dir = temp_dir.path().join("legacy-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&legacy_dir.to_string_lossy()))
        .arg("--stop-after-frame")
        .assert()
        .success();
    let checkpoint_path = legacy_dir.join("checkpoint.json");
    let mut legacy_checkpoint: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&checkpoint_path).expect("read legacy"))
            .expect("parse legacy");
    let contract = legacy_checkpoint["contract"]
        .as_object_mut()
        .expect("contract object");
    assert!(contract.remove("modulation").is_some());
    fs::write(
        &checkpoint_path,
        serde_json::to_string(&legacy_checkpoint).expect("serialize legacy"),
    )
    .expect("write legacy checkpoint");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&legacy_dir.to_string_lossy()))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered datamosh sequence with 3 frame(s)",
        ));
}

#[test]
fn render_fluid_advect_sequence_modulation_continuity_identity() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);

    let source_arg = source_dir.to_string_lossy().to_string();
    let run = |output_dir: &str, extra: &[&str]| {
        let mut args = vec![
            "render-fluid-advect-sequence",
            source_arg.as_str(),
            output_dir,
            "--frames",
            "3",
        ];
        args.extend_from_slice(extra);
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
    };

    // Continuity identity: `scale 0, offset K` pins the knob to K — byte-
    // identical to passing the constant directly.
    let constant_dir = temp_dir.path().join("constant-output");
    run(&constant_dir.to_string_lossy(), &["--reinject", "0.3"]).success();
    let routed_dir = temp_dir.path().join("routed-output");
    run(
        &routed_dir.to_string_lossy(),
        &[
            "--modulate",
            "reinject=luma:0,0.3",
            "--modulator-frames",
            source_arg.as_str(),
        ],
    )
    .success()
    .stdout(predicate::str::contains(
        "modulation routes: reinject=luma:0,0.3",
    ));
    assert_png_frames_identical(&constant_dir, &routed_dir, 3);

    // The route reaches the render: the pinned 0.3 differs from the default
    // reinject (frame 0 is the source verbatim either way).
    let default_dir = temp_dir.path().join("default-output");
    run(&default_dir.to_string_lossy(), &[]).success();
    assert_ne!(
        fs::read(routed_dir.join("frame_000001.png")).expect("routed frame"),
        fs::read(default_dir.join("frame_000001.png")).expect("default frame"),
        "routed reinject must change the rendered sequence"
    );

    // `seed` is a structural field, not a modulation target.
    let rejected_dir = temp_dir.path().join("rejected-output");
    run(
        &rejected_dir.to_string_lossy(),
        &[
            "--modulate",
            "seed=luma",
            "--modulator-frames",
            source_arg.as_str(),
        ],
    )
    .failure()
    .stderr(predicate::str::contains(
        "unknown fluid-advect modulation target",
    ));
}

#[test]
fn render_optical_flow_advect_sequence_modulation_continuity_identity() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    // A translating source: its own Lucas-Kanade motion is the advecting
    // field, so a routed `advect` needs real motion to scale.
    write_texture_sequence(&source_dir, &[0, 2, 4]);

    let source_arg = source_dir.to_string_lossy().to_string();
    let run = |output_dir: &str, extra: &[&str]| {
        let mut args = vec![
            "render-optical-flow-advect-sequence",
            source_arg.as_str(),
            output_dir,
            "--frames",
            "3",
        ];
        args.extend_from_slice(extra);
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
    };

    // Continuity identity on the shared two-source apply fn: `scale 0,
    // offset K` is byte-identical to the constant knob.
    let constant_dir = temp_dir.path().join("constant-output");
    run(&constant_dir.to_string_lossy(), &["--advect", "2.5"]).success();
    let routed_dir = temp_dir.path().join("routed-output");
    run(
        &routed_dir.to_string_lossy(),
        &[
            "--modulate",
            "advect=luma:0,2.5",
            "--modulator-frames",
            source_arg.as_str(),
        ],
    )
    .success()
    .stdout(predicate::str::contains(
        "modulation routes: advect=luma:0,2.5",
    ));
    assert_png_frames_identical(&constant_dir, &routed_dir, 3);

    // The route reaches the render (default advect is 1.0).
    let default_dir = temp_dir.path().join("default-output");
    run(&default_dir.to_string_lossy(), &[]).success();
    assert_ne!(
        fs::read(routed_dir.join("frame_000001.png")).expect("routed frame"),
        fs::read(default_dir.join("frame_000001.png")).expect("default frame"),
        "routed advect must change the rendered sequence"
    );

    // Single-source-only knobs are not two-source targets.
    let rejected_dir = temp_dir.path().join("rejected-output");
    run(
        &rejected_dir.to_string_lossy(),
        &[
            "--modulate",
            "turbulence_scale=luma",
            "--modulator-frames",
            source_arg.as_str(),
        ],
    )
    .failure()
    .stderr(predicate::str::contains(
        "unknown fluid-advect-two-source modulation target",
    ));
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

fn write_texture_sequence(directory: &Path, shifts: &[i32]) {
    for (index, shift) in shifts.iter().enumerate() {
        write_translated_texture(
            &directory.join(format!("frame_{:06}.png", index + 1)),
            24,
            16,
            *shift,
        );
    }
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

fn assert_png_frames_identical(direct_dir: &Path, queued_dir: &Path, frame_count: usize) {
    for index in 0..frame_count {
        let frame = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(queued_dir.join(&frame)).expect("read queued frame"),
            fs::read(direct_dir.join(&frame)).expect("read direct frame"),
            "queue render must be byte-identical to direct render ({frame})"
        );
    }
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read json")).expect("parse json")
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
        .args([
            "queue-run-granular-mosaic-pool-sequence",
            queue_arg.as_str(),
        ])
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

/// Tier 5.6 S2's chosen representative command for the queue add→run
/// byte-identity smoke at depth 16: `--output-bit-depth 16` persists on the
/// queued job (serde-default-8 otherwise), queue-run writes 16-bit PNGs and a
/// manifest carrying `output_bit_depth: 16`, and the queued render is
/// byte-identical to a direct `render-video-vocoder-sequence` at the same
/// knobs (queue and direct share the same render code path).
#[test]
fn queue_video_vocoder_sequence_output_bit_depth_16_matches_direct_render() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let queue_path = temp_dir.path().join("queue.json");
    let output_root = temp_dir.path().join("queue-output");
    let direct_output_dir = temp_dir.path().join("direct-output");

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
            "--output-bit-depth",
            "16",
        ])
        .assert()
        .success();

    let queued: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queued vocoder job"))
            .expect("parse vocoder queue");
    assert_eq!(queued["jobs"][0]["task"]["output_bit_depth"], 16);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-video-vocoder-sequence", queue_arg.as_str()])
        .assert()
        .success();

    let bundle_dir = output_root.join("job-0001");
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(bundle_dir.join("manifest.json")).expect("read vocoder manifest"),
    )
    .expect("parse vocoder manifest");
    assert_eq!(manifest["video_vocoder"]["output_bit_depth"], 16);
    assert_eq!(
        png_color_type(&bundle_dir.join("frames/frame_000000.png")),
        image::ColorType::Rgba16
    );

    // Direct render with the same knobs must be byte-identical (add→run
    // byte-identity: the queue and direct commands share the render code path).
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-video-vocoder-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_output_dir.to_string_lossy().as_ref(),
            "--bands",
            "8",
            "--amount",
            "0.5",
            "--mode",
            "gain",
            "--max-frames",
            "2",
            "--output-bit-depth",
            "16",
        ])
        .assert()
        .success();
    assert_png_frames_identical(&direct_output_dir, &bundle_dir.join("frames"), 2);
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
fn fluid_advect_queue_jobs_match_direct_and_record_manifests() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2]);
    let source_arg = source_dir.to_string_lossy().to_string();

    let direct_fluid = temp_dir.path().join("direct-fluid");
    let direct_fluid_arg = direct_fluid.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-fluid-advect-sequence",
            source_arg.as_str(),
            direct_fluid_arg.as_str(),
            "--frames",
            "2",
            "--advect",
            "2",
            "--reinject",
            "0.2",
            "--turbulence-scale",
            "0.03",
        ])
        .assert()
        .success();

    let fluid_queue = temp_dir.path().join("fluid-queue.json");
    let fluid_queue_arg = fluid_queue.to_string_lossy().to_string();
    let fluid_output = temp_dir.path().join("fluid-output");
    let fluid_output_arg = fluid_output.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-fluid-advect-sequence",
            fluid_queue_arg.as_str(),
            source_arg.as_str(),
            fluid_output_arg.as_str(),
            "--frames",
            "2",
            "--frame-rate",
            "12",
            "--advect",
            "2",
            "--reinject",
            "0.2",
            "--turbulence-scale",
            "0.03",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "queued fluid-advect render job job-0001",
        ));
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-fluid-advect-sequence", fluid_queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered queued fluid-advect job job-0001",
        ));

    let fluid_bundle = fluid_output.join("job-0001");
    assert_png_frames_identical(&direct_fluid, &fluid_bundle.join("frames"), 2);
    let fluid_manifest = read_json(&fluid_bundle.join("manifest.json"));
    assert_eq!(fluid_manifest["task"], "frame_sequence_fluid_advect");
    assert_eq!(
        fluid_manifest["fluid_advect"]["algorithm"],
        "fluid_advect_curl_noise_cpu_v2"
    );
    assert_eq!(fluid_manifest["fluid_advect"]["backend"], "CPU");
    assert_eq!(fluid_manifest["timing"]["frame_rate"], 12.0);

    let direct_particles = temp_dir.path().join("direct-particles");
    let direct_particles_arg = direct_particles.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-field-particles-sequence",
            source_arg.as_str(),
            direct_particles_arg.as_str(),
            "--frames",
            "2",
            "--spacing",
            "8",
            "--particle-size",
            "4",
            "--advect",
            "2",
            "--turbulence-scale",
            "0.03",
            "--live-colour",
        ])
        .assert()
        .success();

    let particles_queue = temp_dir.path().join("particles-queue.json");
    let particles_queue_arg = particles_queue.to_string_lossy().to_string();
    let particles_output = temp_dir.path().join("particles-output");
    let particles_output_arg = particles_output.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-field-particles-sequence",
            particles_queue_arg.as_str(),
            source_arg.as_str(),
            particles_output_arg.as_str(),
            "--frames",
            "2",
            "--spacing",
            "8",
            "--particle-size",
            "4",
            "--advect",
            "2",
            "--turbulence-scale",
            "0.03",
            "--live-colour",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "queued field-particles render job job-0001",
        ));
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-run-field-particles-sequence",
            particles_queue_arg.as_str(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered queued field-particles job job-0001",
        ));

    let particles_bundle = particles_output.join("job-0001");
    assert_png_frames_identical(&direct_particles, &particles_bundle.join("frames"), 2);
    let particles_manifest = read_json(&particles_bundle.join("manifest.json"));
    assert_eq!(particles_manifest["task"], "frame_sequence_field_particles");
    assert_eq!(
        particles_manifest["field_particles"]["algorithm"],
        "field_particles_vortex_cpu_v2"
    );
    assert_eq!(
        particles_manifest["field_particles"]["settings"]["live_color"],
        true
    );
}

#[test]
fn cascade_trails_queue_jobs_match_direct_and_record_manifests() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);
    let source_arg = source_dir.to_string_lossy().to_string();

    let direct = temp_dir.path().join("direct-cascade");
    let direct_arg = direct.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-cascade-trails-sequence",
            source_arg.as_str(),
            direct_arg.as_str(),
            "--frames",
            "3",
            "--tile-size",
            "8",
            "--grid-spacing",
            "16",
            "--advect",
            "2",
            "--turbulence-scale",
            "0.03",
        ])
        .assert()
        .success();

    let queue = temp_dir.path().join("cascade-queue.json");
    let queue_arg = queue.to_string_lossy().to_string();
    let output = temp_dir.path().join("cascade-output");
    let output_arg = output.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-cascade-trails-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_arg.as_str(),
            "--frames",
            "3",
            "--tile-size",
            "8",
            "--grid-spacing",
            "16",
            "--advect",
            "2",
            "--turbulence-scale",
            "0.03",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "queued cascade-trails render job job-0001",
        ));
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-cascade-trails-sequence", queue_arg.as_str()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered queued cascade-trails job job-0001",
        ));

    let bundle = output.join("job-0001");
    assert_png_frames_identical(&direct, &bundle.join("frames"), 3);
    let manifest = read_json(&bundle.join("manifest.json"));
    assert_eq!(manifest["task"], "frame_sequence_cascade_trails");
    assert_eq!(
        manifest["trail_cascade"]["algorithm"],
        "persistent_trail_vortex_cascade_cpu_v1"
    );
    assert_eq!(manifest["trail_cascade"]["backend"], "CPU");
    assert_eq!(manifest["trail_cascade"]["settings"]["grid_spacing"], 16);
    assert_eq!(manifest["trail_cascade"]["settings"]["live_refresh"], true);
}

#[test]
fn optical_flow_advect_queue_jobs_match_direct_and_record_manifests() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    write_texture_sequence(&modulator_dir, &[0, 3]);
    write_texture_sequence(&carrier_dir, &[1, 1]);
    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();

    let direct_two_source = temp_dir.path().join("direct-two-source");
    let direct_two_source_arg = direct_two_source.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-fluid-advect-two-source-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_two_source_arg.as_str(),
            "--frames",
            "2",
            "--advect",
            "0.75",
            "--reinject",
            "0.2",
        ])
        .assert()
        .success();

    let two_source_queue = temp_dir.path().join("two-source-queue.json");
    let two_source_queue_arg = two_source_queue.to_string_lossy().to_string();
    let two_source_output = temp_dir.path().join("two-source-output");
    let two_source_output_arg = two_source_output.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-fluid-advect-two-source-sequence",
            two_source_queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            two_source_output_arg.as_str(),
            "--frames",
            "2",
            "--frame-rate",
            "12",
            "--advect",
            "0.75",
            "--reinject",
            "0.2",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-run-fluid-advect-two-source-sequence",
            two_source_queue_arg.as_str(),
        ])
        .assert()
        .success();

    let two_source_bundle = two_source_output.join("job-0001");
    assert_png_frames_identical(&direct_two_source, &two_source_bundle.join("frames"), 2);
    let two_source_manifest = read_json(&two_source_bundle.join("manifest.json"));
    assert_eq!(
        two_source_manifest["task"],
        "frame_sequence_fluid_advect_two_source"
    );
    assert_eq!(
        two_source_manifest["fluid_advect_two_source"]["algorithm"],
        "fluid_advect_two_source_cpu_v1"
    );
    assert_eq!(
        two_source_manifest["provenance"]["sources"][0]["role"],
        "modulator"
    );

    let direct_self = temp_dir.path().join("direct-self");
    let direct_self_arg = direct_self.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-optical-flow-advect-sequence",
            modulator_arg.as_str(),
            direct_self_arg.as_str(),
            "--frames",
            "2",
            "--advect",
            "0.75",
            "--reinject",
            "0.2",
        ])
        .assert()
        .success();

    let self_queue = temp_dir.path().join("self-queue.json");
    let self_queue_arg = self_queue.to_string_lossy().to_string();
    let self_output = temp_dir.path().join("self-output");
    let self_output_arg = self_output.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-optical-flow-advect-sequence",
            self_queue_arg.as_str(),
            modulator_arg.as_str(),
            self_output_arg.as_str(),
            "--frames",
            "2",
            "--advect",
            "0.75",
            "--reinject",
            "0.2",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-run-optical-flow-advect-sequence",
            self_queue_arg.as_str(),
        ])
        .assert()
        .success();

    let self_bundle = self_output.join("job-0001");
    assert_png_frames_identical(&direct_self, &self_bundle.join("frames"), 2);
    let self_manifest = read_json(&self_bundle.join("manifest.json"));
    assert_eq!(self_manifest["task"], "frame_sequence_optical_flow_advect");
    assert_eq!(
        self_manifest["optical_flow_advect"]["flow_source"],
        "self_optical_flow"
    );
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
        "--mode",
        "gain",
        "--amount",
        "1",
        "--rms-window",
        "4",
        "--rms-hop",
        "4",
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
    // Non-vocode manifests keep their pre-vocode shape — no vocode_bands key.
    assert!(knobs.get("vocode_bands").is_none());
}

#[test]
fn queue_spectral_cross_synth_vocode_matches_direct_and_validates_at_add() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_wav = temp_dir.path().join("modulator.wav");
    let carrier_wav = temp_dir.path().join("carrier.wav");
    // A slow tone imposed on a busier carrier — enough samples for fft 64.
    let modulator: Vec<f32> = (0..256)
        .map(|i| (std::f32::consts::TAU * i as f32 / 32.0).sin())
        .collect();
    let carrier: Vec<f32> = (0..256)
        .map(|i| (0.7 * i as f32).sin() * 0.5 + (1.9 * i as f32).sin() * 0.3)
        .collect();
    write_test_wav(&modulator_wav, &modulator);
    write_test_wav(&carrier_wav, &carrier);

    let modulator_arg = modulator_wav.to_string_lossy().to_string();
    let carrier_arg = carrier_wav.to_string_lossy().to_string();
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();

    // Add-time validation mirrors the render path: bands > fft/2 rejects and
    // persists no queue file.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-spectral-cross-synth",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--mode",
            "vocode",
            "--fft-size",
            "64",
            "--stft-hop",
            "16",
            "--vocode-bands",
            "33",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("vocode-bands must be between"));
    assert!(
        !queue_path.exists(),
        "rejected vocode job must persist no queue file"
    );

    let common = [
        "--mode",
        "vocode",
        "--amount",
        "1",
        "--fft-size",
        "64",
        "--stft-hop",
        "16",
        "--vocode-bands",
        "8",
    ];
    let direct_wav = temp_dir.path().join("direct.wav");
    let direct_arg = direct_wav.to_string_lossy().to_string();
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
    // The persisted task carries the vocode mode + bands.
    let queue_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queue"))
            .expect("parse queue");
    assert_eq!(queue_json["jobs"][0]["task"]["mode"], "vocode");
    assert_eq!(queue_json["jobs"][0]["task"]["vocode_bands"], 8);

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-spectral-cross-synth", queue_arg.as_str()])
        .assert()
        .success();

    let queued_wav = output_root.join("job-0001/audio/cross_synth.wav");
    assert_eq!(
        fs::read(&queued_wav).expect("read queued wav"),
        fs::read(&direct_wav).expect("read direct wav"),
        "queued vocode render must be byte-identical to the direct render"
    );

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["spectral_cross_synth"];
    assert_eq!(knobs["algorithm"], "phase_vocoder_cross_synth_cpu_v1");
    assert_eq!(knobs["mode"], "vocode");
    assert_eq!(knobs["vocode_bands"], 8);
    assert_eq!(knobs["fft_size"], 64);
    assert_eq!(knobs["stft_hop"], 16);
}

#[test]
fn queue_video_audio_route_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_wav = temp_dir.path().join("carrier.wav");
    write_test_wav(&carrier_wav, &[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);

    // Two modulator frames (render-test writes a 256x256 gradient PNG; identical
    // frames give a constant luma — enough to pin path-independence).
    let modulator_dir = temp_dir.path().join("modulator-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let frame_arg = modulator_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_wav.to_string_lossy().to_string();
    let direct_wav = temp_dir.path().join("direct.wav");
    let direct_arg = direct_wav.to_string_lossy().to_string();
    let common = ["--mode", "pan", "--amount", "1", "--fps", "4"];

    // Direct render.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-video-audio-route",
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
            "queue-add-video-audio-route",
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
        .args(["queue-run-video-audio-route", queue_arg.as_str()])
        .assert()
        .success();

    // Queue render is byte-identical to the direct render (path-independent).
    let queued_wav = output_root.join("job-0001/audio/video_audio_route.wav");
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
    assert_eq!(manifest["task"], "video_audio_route");
    assert_eq!(manifest["audio_stems"][0], "audio/video_audio_route.wav");
    let knobs = &manifest["video_audio_route"];
    assert_eq!(knobs["algorithm"], "luma_pan_route_cpu_v1");
    assert_eq!(knobs["descriptor"], "luma");
    assert_eq!(knobs["mode"], "pan");
    assert_eq!(knobs["sampling"], "hold");
    assert_eq!(knobs["amount"], 1.0);
    assert_eq!(knobs["fps"], 4.0);
}

#[test]
fn queue_video_audio_route_flow_descriptor_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_wav = temp_dir.path().join("carrier.wav");
    write_test_wav(&carrier_wav, &[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);

    // Two identical render-test frames ⇒ ~zero optical-flow magnitude. Enough to
    // pin path-independence + the flow algorithm id (the effect's off-vs-on look
    // is proven separately on a moving readout fixture).
    let modulator_dir = temp_dir.path().join("modulator-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let frame_arg = modulator_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_wav.to_string_lossy().to_string();
    let direct_wav = temp_dir.path().join("direct.wav");
    let direct_arg = direct_wav.to_string_lossy().to_string();
    let common = [
        "--descriptor",
        "flow",
        "--mode",
        "gain",
        "--sampling",
        "smooth",
        "--amount",
        "1",
        "--fps",
        "4",
    ];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-video-audio-route",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
        ])
        .args(common)
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-video-audio-route",
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
        .args(["queue-run-video-audio-route", queue_arg.as_str()])
        .assert()
        .success();

    let queued_wav = output_root.join("job-0001/audio/video_audio_route.wav");
    assert_eq!(
        fs::read(&queued_wav).expect("read queued wav"),
        fs::read(&direct_wav).expect("read direct wav"),
        "flow-descriptor queue render must be byte-identical to the direct render"
    );

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["video_audio_route"];
    assert_eq!(knobs["algorithm"], "flow_gain_route_cpu_v1");
    assert_eq!(knobs["descriptor"], "flow");
    assert_eq!(knobs["mode"], "gain");
    assert_eq!(knobs["sampling"], "smooth");
}

#[test]
fn queue_video_audio_route_filter_mode_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_wav = temp_dir.path().join("carrier.wav");
    // Alternating carrier so the one-pole filter has HF content to act on.
    write_test_wav(&carrier_wav, &[1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0]);

    let modulator_dir = temp_dir.path().join("modulator-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let frame_arg = modulator_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_wav.to_string_lossy().to_string();
    let direct_wav = temp_dir.path().join("direct.wav");
    let direct_arg = direct_wav.to_string_lossy().to_string();
    let common = [
        "--mode",
        "filter",
        "--filter-type",
        "highpass",
        "--amount",
        "1",
        "--fps",
        "4",
    ];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-video-audio-route",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
        ])
        .args(common)
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-video-audio-route",
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
        .args(["queue-run-video-audio-route", queue_arg.as_str()])
        .assert()
        .success();

    let queued_wav = output_root.join("job-0001/audio/video_audio_route.wav");
    assert_eq!(
        fs::read(&queued_wav).expect("read queued wav"),
        fs::read(&direct_wav).expect("read direct wav"),
        "filter-mode queue render must be byte-identical to the direct render"
    );

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["video_audio_route"];
    assert_eq!(knobs["algorithm"], "luma_filter_route_cpu_v1");
    assert_eq!(knobs["mode"], "filter");
    assert_eq!(knobs["filter_type"], "highpass");
}

#[test]
fn queue_audio_impulse_convolution_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_wav = temp_dir.path().join("ir.wav");
    let carrier_wav = temp_dir.path().join("carrier.wav");
    // IR [1,1] L1-normalizes to [0.5,0.5] (a smoother); carrier is an alternation.
    write_test_wav(&modulator_wav, &[1.0, 1.0]);
    write_test_wav(&carrier_wav, &[1.0, -1.0, 1.0, -1.0]);

    let modulator_arg = modulator_wav.to_string_lossy().to_string();
    let carrier_arg = carrier_wav.to_string_lossy().to_string();
    let direct_wav = temp_dir.path().join("direct.wav");
    let direct_arg = direct_wav.to_string_lossy().to_string();
    let common = ["--amount", "1", "--max-impulse-samples", "8"];

    // Direct render.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-audio-impulse-convolution",
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
            "queue-add-audio-impulse-convolution",
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
        .args(["queue-run-audio-impulse-convolution", queue_arg.as_str()])
        .assert()
        .success();

    // Queue render is byte-identical to the direct render (path-independent).
    let queued_wav = output_root.join("job-0001/audio/impulse_convolution.wav");
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
    assert_eq!(manifest["task"], "audio_impulse_convolution");
    assert_eq!(manifest["audio_stems"][0], "audio/impulse_convolution.wav");
    let knobs = &manifest["impulse_convolution"];
    assert_eq!(
        knobs["algorithm"],
        "impulse_response_convolution_blend_cpu_v1"
    );
    assert_eq!(knobs["amount"], 1.0);
    assert_eq!(knobs["max_impulse_samples"], 8);
    // HQ-tier knobs default to the direct, non-resampling MVP path.
    assert_eq!(knobs["method"], "direct");
    assert_eq!(knobs["resample_impulse"], false);
    assert_eq!(knobs["ir_mode"], "mono");
}

#[test]
fn queue_audio_impulse_convolution_per_channel_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_wav = temp_dir.path().join("ir.wav");
    let carrier_wav = temp_dir.path().join("carrier.wav");
    // Stereo IR with distinct channels (L = [1,0], R = [0,1]) and a stereo carrier.
    write_stereo_test_wav(&modulator_wav, &[(1.0, 0.0), (0.0, 1.0)]);
    write_stereo_test_wav(&carrier_wav, &[(0.2, 0.6), (0.4, -0.8)]);

    let modulator_arg = modulator_wav.to_string_lossy().to_string();
    let carrier_arg = carrier_wav.to_string_lossy().to_string();
    let direct_wav = temp_dir.path().join("direct.wav");
    let direct_arg = direct_wav.to_string_lossy().to_string();
    let common = ["--amount", "1", "--ir-mode", "per-channel"];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-audio-impulse-convolution",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
        ])
        .args(common)
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-audio-impulse-convolution",
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
        .args(["queue-run-audio-impulse-convolution", queue_arg.as_str()])
        .assert()
        .success();

    let queued_wav = output_root.join("job-0001/audio/impulse_convolution.wav");
    assert_eq!(
        fs::read(&queued_wav).expect("read queued wav"),
        fs::read(&direct_wav).expect("read direct wav"),
        "per-channel queue render must be byte-identical to the direct render"
    );

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["impulse_convolution"];
    assert_eq!(
        knobs["algorithm"],
        "per_channel_impulse_response_convolution_blend_cpu_v1"
    );
    assert_eq!(knobs["ir_mode"], "per_channel");
}

#[test]
fn queue_audio_impulse_convolution_fft_resample_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    // 24 kHz IR + 48 kHz carrier ⇒ the rate mismatch forces --resample-impulse.
    let modulator_wav = temp_dir.path().join("ir.wav");
    let carrier_wav = temp_dir.path().join("carrier.wav");
    write_test_wav_at(&modulator_wav, 24_000, &[1.0, 0.5, -0.25, 0.1]);
    write_test_wav_at(
        &carrier_wav,
        48_000,
        &[0.3, -0.2, 0.4, -0.1, 0.6, -0.5, 0.2, -0.3],
    );

    let modulator_arg = modulator_wav.to_string_lossy().to_string();
    let carrier_arg = carrier_wav.to_string_lossy().to_string();
    let direct_wav = temp_dir.path().join("direct.wav");
    let direct_arg = direct_wav.to_string_lossy().to_string();
    let common = ["--amount", "1", "--method", "fft", "--resample-impulse"];

    // Direct (CLI) render with the FFT method + IR resampling.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-audio-impulse-convolution",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
        ])
        .args(common)
        .assert()
        .success();

    // Queue add + run with the same flags.
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-audio-impulse-convolution",
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
        .args(["queue-run-audio-impulse-convolution", queue_arg.as_str()])
        .assert()
        .success();

    // Queue render byte-identical to the direct render (path-independent, even on
    // the FFT + resampling path).
    let queued_wav = output_root.join("job-0001/audio/impulse_convolution.wav");
    assert_eq!(
        fs::read(&queued_wav).expect("read queued wav"),
        fs::read(&direct_wav).expect("read direct wav"),
        "FFT+resample queue render must be byte-identical to the direct render"
    );

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["impulse_convolution"];
    assert_eq!(knobs["method"], "fft");
    assert_eq!(knobs["resample_impulse"], true);
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
fn queue_datamosh_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    // Two modulator + two carrier frames (render-test writes a 256x256 gradient
    // PNG). Identical modulator frames ⇒ zero flow ⇒ the advect step is identity,
    // so the output equals the carrier — enough to pin determinism + queue==direct
    // (the motion-driven melt is exercised by the off-vs-on readout, not here).
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
            "render-datamosh-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
            "--keyframe-interval",
            "0",
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
            "queue-add-datamosh-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-datamosh-sequence", queue_arg.as_str()])
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

    // Manifest records the datamosh algorithm + knobs.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "frame_sequence_datamosh");
    let knobs = &manifest["datamosh"];
    assert_eq!(knobs["algorithm"], "flow_reuse_datamosh_bloom_cpu_v1");
    assert_eq!(knobs["preset"], "custom");
    assert_eq!(knobs["keyframe_interval"], 0);
    assert_eq!(knobs["amount"], 1.0);
    assert_eq!(knobs["block_size"], 1);
    // Residual defaults: gain 0 (off ⇒ bloom id unchanged), decay 0.9 (CLI default).
    // (decay is an f32 ⇒ serializes non-exactly, so compare approximately.)
    assert_eq!(knobs["residual_gain"], 0.0);
    assert!((knobs["residual_decay"].as_f64().expect("decay") - 0.9).abs() < 1e-6);
    // Per-block refresh default: threshold 0 (off ⇒ no block refresh).
    assert_eq!(knobs["block_refresh_threshold"], 0.0);
    assert_eq!(knobs["backend"], "CPU");
    assert!(output_root
        .join("job-0001/cache/datamosh-flow/frame_000001/manifest.json")
        .exists());
    assert_eq!(
        manifest["provenance"]["analysis_caches"][0]["producer"],
        "pyramidal_lucas_kanade_cpu_v1"
    );
}

#[test]
fn queue_datamosh_block_path_records_block_algorithm_and_matches_direct() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    // Same gradient fixture: identical modulator frames ⇒ zero flow ⇒ quantizing
    // to blocks is still zero ⇒ advect identity, so this pins determinism +
    // queue==direct + the resolved block algorithm id on the codec-simulated path
    // (the chunky macroblock look is exercised by the off-vs-on readout, not here).
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

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-datamosh-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--block-size",
            "16",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-datamosh-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--block-size",
            "16",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-datamosh-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for frame in ["frame_000000.png", "frame_000001.png"] {
        let queued = output_root.join("job-0001/frames").join(frame);
        let direct = direct_dir.join(frame);
        assert_eq!(
            fs::read(&queued).expect("read queued frame"),
            fs::read(&direct).expect("read direct frame"),
            "queue render must be byte-identical to the direct render ({frame})"
        );
    }

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["datamosh"];
    // block_size >= 2 ⇒ the codec-simulated block algorithm id, block_size recorded.
    assert_eq!(knobs["algorithm"], "flow_reuse_datamosh_block_cpu_v1");
    assert_eq!(knobs["block_size"], 16);
}

#[test]
fn queue_datamosh_residual_path_records_residual_algorithm_and_matches_direct() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    // Identical modulator frames ⇒ zero flow ⇒ zero block residual ⇒ advect
    // identity, so this pins determinism + queue==direct + the resolved
    // block-residual algorithm id on the residual path (the fine-motion haze is
    // exercised by the off-vs-on readout, not here).
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

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-datamosh-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--block-size",
            "16",
            "--residual-gain",
            "1.0",
            "--residual-decay",
            "0.5",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-datamosh-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--block-size",
            "16",
            "--residual-gain",
            "1.0",
            "--residual-decay",
            "0.5",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-datamosh-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for frame in ["frame_000000.png", "frame_000001.png"] {
        let queued = output_root.join("job-0001/frames").join(frame);
        let direct = direct_dir.join(frame);
        assert_eq!(
            fs::read(&queued).expect("read queued frame"),
            fs::read(&direct).expect("read direct frame"),
            "queue render must be byte-identical to the direct render ({frame})"
        );
    }

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["datamosh"];
    // block_size >= 2 AND residual_gain > 0 ⇒ the block-residual algorithm id.
    assert_eq!(
        knobs["algorithm"],
        "flow_reuse_datamosh_block_residual_cpu_v1"
    );
    assert_eq!(knobs["block_size"], 16);
    assert_eq!(knobs["residual_gain"], 1.0);
    assert_eq!(knobs["residual_decay"], 0.5);
}

#[test]
fn queue_datamosh_refresh_path_records_refresh_algorithm_and_matches_direct() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    // Identical modulator frames ⇒ zero flow ⇒ every block's mean motion is below
    // any positive threshold ⇒ every block refreshes to the carrier. This pins
    // determinism + queue==direct + the resolved per-block-refresh algorithm id and
    // recorded threshold (the patchy keep/rot look is exercised by the off-vs-on
    // readout, not here).
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

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-datamosh-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--block-size",
            "16",
            "--block-refresh-threshold",
            "0.5",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-datamosh-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--block-size",
            "16",
            "--block-refresh-threshold",
            "0.5",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-datamosh-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for frame in ["frame_000000.png", "frame_000001.png"] {
        let queued = output_root.join("job-0001/frames").join(frame);
        let direct = direct_dir.join(frame);
        assert_eq!(
            fs::read(&queued).expect("read queued frame"),
            fs::read(&direct).expect("read direct frame"),
            "queue render must be byte-identical to the direct render ({frame})"
        );
    }

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["datamosh"];
    // block_size >= 2 AND block_refresh_threshold > 0 ⇒ the per-block-refresh id.
    assert_eq!(
        knobs["algorithm"],
        "flow_reuse_datamosh_block_refresh_cpu_v1"
    );
    assert_eq!(knobs["block_size"], 16);
    assert_eq!(knobs["block_refresh_threshold"], 0.5);
}

#[test]
fn queue_datamosh_vector_remix_path_records_remix_algorithm_and_matches_direct() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    // Identical modulator frames ⇒ zero flow ⇒ every block MV is zero ⇒ permuting
    // zeros is still zero. This pins determinism + queue==direct + the curated
    // vector-shuffle preset resolving to the vector-remix algorithm id.
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

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-datamosh-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--preset",
            "vector-shuffle",
            "--remix-seed",
            "42",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-datamosh-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--preset",
            "vector-shuffle",
            "--remix-seed",
            "42",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-datamosh-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for frame in ["frame_000000.png", "frame_000001.png"] {
        let queued = output_root.join("job-0001/frames").join(frame);
        let direct = direct_dir.join(frame);
        assert_eq!(
            fs::read(&queued).expect("read queued frame"),
            fs::read(&direct).expect("read direct frame"),
            "queue render must be byte-identical to the direct render ({frame})"
        );
    }

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["datamosh"];
    // block_size >= 2 AND vector_remix != none ⇒ the vector-remix algorithm id;
    // the mode + seed round-trip through the persisted job into the manifest.
    assert_eq!(
        knobs["algorithm"],
        "flow_reuse_datamosh_vector_remix_cpu_v1"
    );
    assert_eq!(knobs["preset"], "vector_shuffle");
    assert_eq!(knobs["block_size"], 16);
    assert_eq!(knobs["vector_remix"], "shuffle");
    assert_eq!(knobs["remix_seed"], 42);
}

#[test]
fn queue_datamosh_scanline_smear_records_algorithm_and_matches_direct() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

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

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-datamosh-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
            "--preset",
            "scanline-smear",
            "--remix-seed",
            "99",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-datamosh-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--preset",
            "scanline-smear",
            "--remix-seed",
            "99",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-datamosh-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for frame in ["frame_000000.png", "frame_000001.png"] {
        let queued = output_root.join("job-0001/frames").join(frame);
        let direct = direct_dir.join(frame);
        assert_eq!(
            fs::read(&queued).expect("read queued frame"),
            fs::read(&direct).expect("read direct frame"),
            "queue render must be byte-identical to the direct render ({frame})"
        );
    }

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["datamosh"];
    assert_eq!(
        knobs["algorithm"],
        "flow_reuse_datamosh_scanline_smear_cpu_v1"
    );
    assert_eq!(knobs["preset"], "scanline_smear");
    assert_eq!(knobs["scanline_smear"], true);
    assert_eq!(knobs["vector_remix"], "sort");
    assert_eq!(knobs["remix_seed"], 99);
}

#[test]
fn queue_datamosh_codec_engrave_records_algorithm_and_matches_direct() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

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

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-datamosh-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_arg.as_str(),
            "--preset",
            "codec-engrave",
            "--remix-seed",
            "123",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-datamosh-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--preset",
            "codec-engrave",
            "--remix-seed",
            "123",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-datamosh-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for frame in ["frame_000000.png", "frame_000001.png"] {
        let queued = output_root.join("job-0001/frames").join(frame);
        let direct = direct_dir.join(frame);
        assert_eq!(
            fs::read(&queued).expect("read queued frame"),
            fs::read(&direct).expect("read direct frame"),
            "queue render must be byte-identical to the direct render ({frame})"
        );
    }

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["datamosh"];
    assert_eq!(
        knobs["algorithm"],
        "flow_reuse_datamosh_codec_engrave_cpu_v1"
    );
    assert_eq!(knobs["preset"], "codec_engrave");
    assert_eq!(knobs["scanline_smear"], true);
    assert_eq!(knobs["codec_engrave"], true);
    assert_eq!(knobs["vector_remix"], "sort");
    assert_eq!(knobs["remix_seed"], 123);
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
    assert_eq!(knobs["kernel_mode"], "luma");
    assert_eq!(knobs["backend"], "CPU");
}

#[test]
fn queue_convolution_blend_color_mode_matches_direct_and_records_knobs() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

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

    // Direct colour render.
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
            "--kernel-mode",
            "color",
        ])
        .assert()
        .success();

    // Queue add + run in colour mode.
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
            "--kernel-mode",
            "color",
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-convolutional-blend-sequence", queue_arg.as_str()])
        .assert()
        .success();

    for frame in ["frame_000000.png", "frame_000001.png"] {
        let queued = output_root.join("job-0001/frames").join(frame);
        let direct = direct_dir.join(frame);
        assert_eq!(
            fs::read(&queued).expect("read queued frame"),
            fs::read(&direct).expect("read direct frame"),
            "colour queue render must be byte-identical to the direct render ({frame})"
        );
    }

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let knobs = &manifest["convolution_blend"];
    assert_eq!(
        knobs["algorithm"],
        "image_color_kernel_convolution_blend_cpu_v1"
    );
    assert_eq!(knobs["kernel_mode"], "color");
    assert_eq!(knobs["kernel_size"], 5);
}

fn write_test_wav(path: &Path, samples: &[f32]) {
    write_test_wav_at(path, 4, samples);
}

fn write_stereo_test_wav(path: &Path, frames: &[(f32, f32)]) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 4,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
    for (left, right) in frames {
        writer.write_sample(*left).expect("write left");
        writer.write_sample(*right).expect("write right");
    }
    writer.finalize().expect("finalize wav");
}

fn write_test_wav_at(path: &Path, sample_rate: u32, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("create wav");
    for sample in samples {
        writer.write_sample(*sample).expect("write sample");
    }
    writer.finalize().expect("finalize wav");
}

// ── Minimal SMF byte-array builder for queue/checkpoint MIDI smokes ─────────
// Duplicates the small helper set from `morphogen-cli/src/modulate.rs`'s own
// `#[cfg(test)]` module (private, unreachable from this integration-test
// crate) — the S1 "no binary fixtures in the repo" precedent
// (`docs/MIDI_MODULATION_MILESTONE.md`).

fn midi_vlq(mut value: u32) -> Vec<u8> {
    let mut bytes = vec![(value & 0x7F) as u8];
    value >>= 7;
    while value > 0 {
        bytes.push(((value & 0x7F) as u8) | 0x80);
        value >>= 7;
    }
    bytes.reverse();
    bytes
}

fn midi_chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(id);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    out
}

fn midi_control_change(channel: u8, controller: u8, value: u8) -> Vec<u8> {
    vec![0xB0 | channel, controller, value]
}

/// A format-0, single-track, PPQ-480 SMF (default 120 BPM tempo) built from
/// `(delta_ticks, event_bytes)` pairs, with an End-of-Track appended.
fn write_test_smf(path: &Path, events: &[(u32, Vec<u8>)]) {
    let mut header = Vec::new();
    header.extend_from_slice(&0u16.to_be_bytes()); // format 0
    header.extend_from_slice(&1u16.to_be_bytes()); // ntrks
    header.extend_from_slice(&480u16.to_be_bytes()); // PPQ division
    let mut bytes = midi_chunk(b"MThd", &header);

    let mut track_body = Vec::new();
    for (delta, data) in events {
        track_body.extend_from_slice(&midi_vlq(*delta));
        track_body.extend_from_slice(data);
    }
    track_body.extend_from_slice(&midi_vlq(0));
    track_body.extend_from_slice(&[0xFF, 0x2F, 0x00]); // end-of-track
    bytes.extend_from_slice(&midi_chunk(b"MTrk", &track_body));

    fs::write(path, bytes).expect("write test SMF");
}

#[test]
fn pixel_sort_metal_parity_real_footage() {
    use image::ImageReader;
    use morphogen_render::{render_pixel_sort_frame, ImageBufferF32, PixelSortSettings, SortAxis};

    let path = "../../renders/cello2-frames/frame_000009.png";
    if !std::path::Path::new(path).exists() {
        eprintln!("skipping: {path} not found");
        return;
    }
    let decoded = ImageReader::open(path)
        .unwrap()
        .decode()
        .unwrap()
        .to_rgba32f();
    let pixels: Vec<[f32; 4]> = decoded.pixels().map(|p| p.0).collect();
    let source = ImageBufferF32::new(decoded.width(), decoded.height(), pixels).unwrap();

    let settings = PixelSortSettings {
        axis: SortAxis::Row,
        threshold_low: 0.20,
        threshold_high: 0.85,
        ..Default::default()
    };

    let cpu = render_pixel_sort_frame(&source, &settings, &[]).expect("cpu");
    let gpu = morphogen_metal::pixel_sort_metal(&source, &settings).expect("gpu");

    let mut worst_diff = 0.0_f32;
    let mut worst_pos = (0u32, 0u32);
    let mut worst_gpu = [0.0_f32; 4];
    let mut worst_cpu = [0.0_f32; 4];
    for (i, (g_px, c_px)) in gpu.pixels.iter().zip(cpu.pixels.iter()).enumerate() {
        let diff = g_px
            .iter()
            .zip(c_px.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        if diff > worst_diff {
            worst_diff = diff;
            worst_pos = (i as u32 % source.width, i as u32 / source.width);
            worst_gpu = *g_px;
            worst_cpu = *c_px;
        }
    }
    assert_eq!(
        worst_diff, 0.0,
        "Metal pixel sort diverged from CPU at ({},{}): gpu={:?} cpu={:?} diff={}",
        worst_pos.0, worst_pos.1, worst_gpu, worst_cpu, worst_diff
    );
}

#[test]
fn queue_retro_static_modulated_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    // Three identical gradient frames; the modulation variation comes from the
    // WAV's RMS ramp, not the frames.
    let source_dir = temp_dir.path().join("source-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        let frame_arg = source_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }

    // A 0.75 s quiet→loud amplitude ramp at 8192 Hz: the RMS envelope rises
    // across the three output frames (fps 4), so the routed strength varies.
    let modulator_wav = temp_dir.path().join("ramp.wav");
    let ramp: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&modulator_wav, 8192, &ramp);

    let source_arg = source_dir.to_string_lossy().to_string();
    let wav_arg = modulator_wav.to_string_lossy().to_string();
    let direct_dir = temp_dir.path().join("direct");
    let direct_arg = direct_dir.to_string_lossy().to_string();
    let route = "strength=audio-rms:0.9,0.05";

    // Direct render with the route; --modulation-fps must equal the queued
    // job's --frame-rate for identical envelope sampling.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-retro-static-sequence",
            source_arg.as_str(),
            direct_arg.as_str(),
            "--frames",
            "3",
            "--backend",
            "cpu",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
            "--modulation-fps",
            "4",
        ])
        .assert()
        .success();

    // Queue add + run with the same knobs.
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-retro-static-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--frames",
            "3",
            "--frame-rate",
            "4",
            "--backend",
            "cpu",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-retro-static-sequence", queue_arg.as_str()])
        .assert()
        .success();

    // Queue render is byte-identical to the direct render (path-independent).
    for frame_name in ["frame_000000.png", "frame_000001.png", "frame_000002.png"] {
        let direct_frame = direct_dir.join(frame_name);
        let queued_frame = output_root.join("job-0001/frames").join(frame_name);
        assert_eq!(
            fs::read(&queued_frame).expect("read queued frame"),
            fs::read(&direct_frame).expect("read direct frame"),
            "queue render must be byte-identical to direct render ({frame_name})"
        );
    }

    // The manifest records the persisted routes + envelope provenance.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "frame_sequence_retro_static");
    let modulation = &manifest["retro_static"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "strength");
    assert_eq!(modulation["routes"][0]["source"], "audio-rms");
    assert_eq!(modulation["routes"][0]["scale"], 0.9f32 as f64);
    assert_eq!(modulation["routes"][0]["offset"], 0.05f32 as f64);
    assert_eq!(modulation["sampling"], "hold");
    assert_eq!(modulation["fps"], 4.0);
    assert_eq!(modulation["modulator_audio"], wav_arg.as_str());

    // The routed strength actually varied: the queued frames differ from each
    // other despite identical source frames.
    let f0 = fs::read(output_root.join("job-0001/frames/frame_000000.png")).expect("f0");
    let f2 = fs::read(output_root.join("job-0001/frames/frame_000002.png")).expect("f2");
    assert_ne!(
        f0, f2,
        "RMS ramp must vary the routed strength across frames"
    );
}

#[test]
fn per_route_sampling_overrides_global_and_round_trips_through_queue() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        let frame_arg = source_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }
    let modulator_wav = temp_dir.path().join("ramp.wav");
    let ramp: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&modulator_wav, 8192, &ramp);

    let source_arg = source_dir.to_string_lossy().to_string();
    let wav_arg = modulator_wav.to_string_lossy().to_string();
    let render = |output_dir: &Path, route: &str| {
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "render-retro-static-sequence",
                source_arg.as_str(),
                output_dir.to_string_lossy().as_ref(),
                "--frames",
                "3",
                "--backend",
                "cpu",
                "--modulate",
                route,
                "--modulator-audio",
                wav_arg.as_str(),
                "--modulation-fps",
                "3",
            ])
            .assert()
            .success()
    };

    // `@hold` is the global default spelled per-route: byte-identical.
    let plain_dir = temp_dir.path().join("plain");
    render(&plain_dir, "strength=audio-rms:0.5,0.25");
    let hold_dir = temp_dir.path().join("hold");
    render(&hold_dir, "strength=audio-rms:0.5,0.25@hold");
    assert_png_frames_identical(&plain_dir, &hold_dir, 3);

    // `@smooth` interpolates the envelope, so the routed strength (and the
    // frames) differ from the held evaluation.
    let smooth_dir = temp_dir.path().join("smooth");
    render(&smooth_dir, "strength=audio-rms:0.5,0.25@smooth");
    assert_ne!(
        fs::read(smooth_dir.join("frame_000001.png")).expect("smooth frame"),
        fs::read(plain_dir.join("frame_000001.png")).expect("held frame"),
        "@smooth must change the envelope evaluation"
    );

    // The suffix persists on the queue and round-trips through queue-run —
    // byte-identical to the direct @smooth render, with the route's sampling
    // recorded in the manifest.
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-retro-static-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--frame-rate",
            "3",
            "--backend",
            "cpu",
            "--modulate",
            "strength=audio-rms:0.5,0.25@smooth",
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-retro-static-sequence", queue_arg.as_str()])
        .assert()
        .success();
    assert_png_frames_identical(&smooth_dir, &output_root.join("job-0001/frames"), 3);

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let route = &manifest["retro_static"]["modulation"]["routes"][0];
    assert_eq!(route["sampling"], "smooth");

    // A bad suffix is rejected up front.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-retro-static-sequence",
            source_arg.as_str(),
            temp_dir.path().join("bad").to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--modulate",
            "strength=audio-rms@linear",
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown sampling 'linear'"));
}

#[test]
fn modulation_envelope_sidecar_reuses_and_invalidates_on_content_change() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        let frame_arg = source_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }
    let modulator_dir = temp_dir.path().join("modulator-frames");
    write_texture_sequence(&modulator_dir, &[0, 2, 4]);

    let source_arg = source_dir.to_string_lossy().to_string();
    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let cache_dir = temp_dir.path().join("envelope-cache");
    let cache_arg = cache_dir.to_string_lossy().to_string();
    let render = |output_dir: &Path, cached: bool| {
        let mut args = vec![
            "render-retro-static-sequence".to_string(),
            source_arg.clone(),
            output_dir.to_string_lossy().to_string(),
            "--frames".to_string(),
            "3".to_string(),
            "--backend".to_string(),
            "cpu".to_string(),
            "--modulate".to_string(),
            "strength=luma:0.5,0.25".to_string(),
            "--modulator-frames".to_string(),
            modulator_arg.clone(),
            "--modulation-fps".to_string(),
            "4".to_string(),
        ];
        if cached {
            args.extend(["--modulation-cache-dir".to_string(), cache_arg.clone()]);
        }
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
            .success()
    };

    // Uncached reference, then a cold-cache render generates the sidecar and
    // a warm render reuses it — all three byte-identical.
    let reference_dir = temp_dir.path().join("reference");
    render(&reference_dir, false);
    let cold_dir = temp_dir.path().join("cold");
    render(&cold_dir, true).stdout(predicate::str::contains(
        "generated modulation envelope sidecar for 'luma'",
    ));
    let warm_dir = temp_dir.path().join("warm");
    render(&warm_dir, true).stdout(predicate::str::contains(
        "reused modulation envelope sidecar for 'luma'",
    ));
    assert_png_frames_identical(&reference_dir, &cold_dir, 3);
    assert_png_frames_identical(&reference_dir, &warm_dir, 3);

    // The sidecar records the contract fields.
    let sidecar: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(cache_dir.join("envelope_luma.json")).expect("read sidecar"),
    )
    .expect("parse sidecar");
    assert_eq!(sidecar["algorithm"], "modulation_envelope_luma_v1");
    assert_eq!(sidecar["fps"], 4.0);
    assert!(sidecar["checksum"]
        .as_str()
        .expect("checksum")
        .starts_with("fnv1a64:"));

    // Changing the modulator content invalidates the sidecar: the next run
    // regenerates instead of reusing, and still matches an uncached render
    // of the new content.
    write_translated_texture(&modulator_dir.join("frame_000002.png"), 24, 16, 9);
    let fresh_reference_dir = temp_dir.path().join("fresh-reference");
    render(&fresh_reference_dir, false);
    let regenerated_dir = temp_dir.path().join("regenerated");
    render(&regenerated_dir, true).stdout(predicate::str::contains(
        "generated modulation envelope sidecar for 'luma'",
    ));
    assert_png_frames_identical(&fresh_reference_dir, &regenerated_dir, 3);
}

#[test]
fn named_modulators_drive_independent_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        let frame_arg = source_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }
    // Two different envelopes: a rising and a falling amplitude ramp.
    let rising_wav = temp_dir.path().join("rising.wav");
    let rising: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&rising_wav, 8192, &rising);
    let falling_wav = temp_dir.path().join("falling.wav");
    let falling: Vec<f32> = (0..6144)
        .map(|i| (1.0 - i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&falling_wav, 8192, &falling);

    let source_arg = source_dir.to_string_lossy().to_string();
    let rising_arg = rising_wav.to_string_lossy().to_string();
    let falling_arg = falling_wav.to_string_lossy().to_string();
    let render = |output_dir: &Path, extra: &[&str]| {
        let mut args = vec!["render-channel-shift-sequence", source_arg.as_str()];
        let output_arg = output_dir.to_string_lossy().to_string();
        let output_arg = Box::leak(output_arg.into_boxed_str());
        args.push(output_arg);
        args.extend(["--frames", "3", "--modulation-fps", "4"]);
        args.extend_from_slice(extra);
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(&args)
            .assert()
    };

    // Two named modulators drive two knobs from different envelopes.
    let named_dir = temp_dir.path().join("named");
    render(
        &named_dir,
        &[
            "--modulate",
            "shift_r_x=a.audio-rms:32,0",
            "--modulate",
            "shift_b_x=b.audio-rms:32,0",
            "--named-modulator-audio",
            &format!("a={rising_arg}"),
            "--named-modulator-audio",
            &format!("b={falling_arg}"),
        ],
    )
    .success()
    .stdout(predicate::str::contains(
        "modulation routes: shift_r_x=a.audio-rms:32,0 shift_b_x=b.audio-rms:32,0",
    ));

    // Both knobs from ONE modulator differs — the second envelope matters.
    let single_dir = temp_dir.path().join("single");
    render(
        &single_dir,
        &[
            "--modulate",
            "shift_r_x=audio-rms:32,0",
            "--modulate",
            "shift_b_x=audio-rms:32,0",
            "--modulator-audio",
            rising_arg.as_str(),
        ],
    )
    .success();
    assert_ne!(
        fs::read(named_dir.join("frame_000002.png")).expect("named frame"),
        fs::read(single_dir.join("frame_000002.png")).expect("single frame"),
        "a second modulator must change the routed knob history"
    );

    // Continuity: a named route reading the same media as the default
    // modulator is byte-identical to the unnamed route.
    let aliased_dir = temp_dir.path().join("aliased");
    render(
        &aliased_dir,
        &[
            "--modulate",
            "shift_r_x=a.audio-rms:32,0",
            "--named-modulator-audio",
            &format!("a={rising_arg}"),
        ],
    )
    .success();
    let unnamed_dir = temp_dir.path().join("unnamed");
    render(
        &unnamed_dir,
        &[
            "--modulate",
            "shift_r_x=audio-rms:32,0",
            "--modulator-audio",
            rising_arg.as_str(),
        ],
    )
    .success();
    assert_png_frames_identical(&unnamed_dir, &aliased_dir, 3);

    // A named route without its media flag, and a duplicate name, both fail
    // up front.
    render(
        &temp_dir.path().join("missing"),
        &["--modulate", "shift_r_x=x.audio-rms"],
    )
    .failure()
    .stderr(predicate::str::contains(
        "requires --named-modulator-audio x=<path>",
    ));
    render(
        &temp_dir.path().join("duplicate"),
        &[
            "--modulate",
            "shift_r_x=a.audio-rms",
            "--named-modulator-audio",
            &format!("a={rising_arg}"),
            "--named-modulator-audio",
            &format!("a={falling_arg}"),
        ],
    )
    .failure()
    .stderr(predicate::str::contains(
        "duplicate --named-modulator-audio",
    ));

    // Named modulators are now also supported on the queue path (see
    // `queue_named_modulators_*` tests below); a named route whose media flag
    // is missing still fails up front and persists nothing, same as direct.
    let queue_path = temp_dir.path().join("queue.json");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-channel-shift-sequence",
            queue_path.to_string_lossy().as_ref(),
            source_arg.as_str(),
            temp_dir.path().join("out").to_string_lossy().as_ref(),
            "--modulate",
            "shift_r_x=a.audio-rms",
            "--modulator-audio",
            rising_arg.as_str(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "requires --named-modulator-audio a=<path>",
        ));
    assert!(!queue_path.exists());
}

#[test]
fn named_modulator_joins_feedback_checkpoint_contract() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        for dir in [&modulator_dir, &carrier_dir] {
            let frame_arg = dir.join(frame_name).to_string_lossy().to_string();
            Command::cargo_bin("morphogen")
                .expect("morphogen binary")
                .args(["render-test", frame_arg.as_str()])
                .assert()
                .success();
        }
    }
    let modulator_wav = temp_dir.path().join("ramp.wav");
    let ramp: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&modulator_wav, 8192, &ramp);

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let wav_arg = modulator_wav.to_string_lossy().to_string();
    let output_dir = temp_dir.path().join("output");
    let args = vec![
        "render-feedback-sequence".to_string(),
        modulator_arg,
        carrier_arg,
        output_dir.to_string_lossy().to_string(),
        "--feedback-mix".to_string(),
        "0.7".to_string(),
        "--max-frames".to_string(),
        "3".to_string(),
        "--frame-rate".to_string(),
        "4".to_string(),
        "--flow-source".to_string(),
        "luminance".to_string(),
        "--modulate".to_string(),
        "feedback_mix=fb.audio-rms:0.5,0.25".to_string(),
        "--named-modulator-audio".to_string(),
        format!("fb={wav_arg}"),
    ];
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&args)
        .arg("--stop-after-frame")
        .assert()
        .success();

    // The checkpoint contract fingerprints the named media the route consumes;
    // the default-modulator slots stay empty (no unnamed route used them).
    let checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_dir.join("checkpoint.json")).expect("read checkpoint"),
    )
    .expect("parse checkpoint");
    let modulation = &checkpoint["contract"]["modulation"];
    assert_eq!(modulation["routes"][0]["modulator"], "fb");
    assert!(modulation["modulator_audio"].is_null());
    let named = &modulation["named_modulators"][0];
    assert_eq!(named["name"], "fb");
    assert_eq!(named["kind"], "audio");
    assert_eq!(named["path"], wav_arg.as_str());
    assert!(named["checksum"]
        .as_str()
        .expect("checksum")
        .starts_with("fnv1a64:"));

    // Identical arguments resume to completion (contract equality holds
    // across the named fingerprints).
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered flow-feedback sequence with 3 frame(s)",
        ));

    // A renamed modulator changes the contract and refuses to resume.
    let mut renamed = args.clone();
    renamed[13] = "feedback_mix=fb2.audio-rms:0.5,0.25".to_string();
    renamed[15] = format!("fb2={wav_arg}");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&renamed)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));
}

#[test]
fn queue_add_rejects_bad_modulation_routes_before_persisting() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    let frame_arg = source_dir
        .join("frame_000001.png")
        .to_string_lossy()
        .to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["render-test", frame_arg.as_str()])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let source_arg = source_dir.to_string_lossy().to_string();
    let output_root_arg = temp_dir.path().join("out").to_string_lossy().to_string();

    // Unknown target for the effect fails at add time…
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-retro-static-sequence",
            queue_path.to_string_lossy().as_ref(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--modulate",
            "real_bpp=audio-rms",
            "--modulator-audio",
            "/tmp/unused.wav",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown retro-static modulation target",
        ));

    // …as does an audio route without --modulator-audio.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-retro-static-sequence",
            queue_path.to_string_lossy().as_ref(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--modulate",
            "strength=audio-rms",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --modulator-audio"));

    // Neither failure persisted a job.
    assert!(
        !queue_path.exists(),
        "rejected queue-add must not write a queue file"
    );
}

#[test]
fn queue_feedback_modulated_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        for dir in [&modulator_dir, &carrier_dir] {
            let frame_arg = dir.join(frame_name).to_string_lossy().to_string();
            Command::cargo_bin("morphogen")
                .expect("morphogen binary")
                .args(["render-test", frame_arg.as_str()])
                .assert()
                .success();
        }
    }
    let modulator_wav = temp_dir.path().join("ramp.wav");
    let ramp: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&modulator_wav, 8192, &ramp);

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let wav_arg = modulator_wav.to_string_lossy().to_string();
    let route = "feedback_mix=audio-rms:0.5,0.25";

    // Direct render: feedback samples envelopes against its own --frame-rate,
    // which is exactly the queued job's frame_rate — the shared time base.
    let direct_dir = temp_dir.path().join("direct");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-feedback-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_dir.to_string_lossy().as_ref(),
            "--feedback-mix",
            "0.7",
            "--max-frames",
            "3",
            "--frame-rate",
            "4",
            "--flow-source",
            "luminance",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-feedback-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root.to_string_lossy().as_ref(),
            "--feedback-mix",
            "0.7",
            "--max-frames",
            "3",
            "--frame-rate",
            "4",
            "--flow-source",
            "luminance",
            "--no-flow-cache",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-feedback-sequence", queue_arg.as_str()])
        .assert()
        .success();

    let job_frames = output_root.join("job-0001/frames");
    assert_png_frames_identical(&direct_dir.join("frames"), &job_frames, 3);

    // Stateful: the queued render's checkpoint contract carries the routes,
    // so a re-run with different routes would refuse to resume.
    let checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/checkpoint.json"))
            .expect("read queued feedback checkpoint"),
    )
    .expect("parse queued feedback checkpoint");
    let modulation = &checkpoint["contract"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "feedback_mix");
    assert_eq!(modulation["routes"][0]["source"], "audio-rms");
    assert_eq!(modulation["routes"][0]["scale"], 0.5);
    assert_eq!(modulation["routes"][0]["offset"], 0.25);
    assert_eq!(modulation["fps"], 4.0);
}

/// Morphogenesis (Tier "Morphogenesis" S4): queue add→run shares
/// `render_morphogenesis_sequence` with the direct CLI, so the two paths
/// must be byte-identical, and the queued job's checkpoint must carry the
/// routed contract exactly like flow-feedback's.
#[test]
fn queue_morphogenesis_modulated_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0",
        ])
        .assert()
        .success();

    let modulator_wav = temp_dir.path().join("ramp.wav");
    let ramp: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&modulator_wav, 8192, &ramp);

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let wav_arg = modulator_wav.to_string_lossy().to_string();
    let route = "feed=audio-rms:0.02,0.03";

    // Direct render: morphogenesis samples envelopes against its own
    // --frame-rate, which is exactly the queued job's frame_rate.
    let direct_dir = temp_dir.path().join("direct");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            direct_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--frame-rate",
            "4",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-morphogenesis-sequence",
            queue_arg.as_str(),
            carrier_arg.as_str(),
            output_root.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--frame-rate",
            "4",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-morphogenesis-sequence", queue_arg.as_str()])
        .assert()
        .success();

    let job_frames = output_root.join("job-0001/frames");
    assert_png_frames_identical(&direct_dir.join("frames"), &job_frames, 3);

    // The queued render's manifest records the algorithm/settings, mirroring
    // the direct CLI's own manifest shape.
    let manifest = read_json(&output_root.join("job-0001/manifest.json"));
    assert_eq!(manifest["algorithm"], "morphogenesis_cpu_v1");
    assert_eq!(manifest["task"], "render_morphogenesis_sequence");

    // Stateful: the queued render's checkpoint contract carries the routes,
    // so a re-run with different routes would refuse to resume.
    let checkpoint = read_json(&output_root.join("job-0001/checkpoint.json"));
    let modulation = &checkpoint["contract"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "feed");
    assert_eq!(modulation["routes"][0]["source"], "audio-rms");
    assert_eq!(modulation["routes"][0]["scale"], 0.02);
    assert_eq!(modulation["routes"][0]["offset"], 0.03);
    assert_eq!(modulation["fps"], 4.0);
}

/// `queue-add-morphogenesis-sequence` validates the full direct-CLI surface
/// (settings/composite/timing) before persisting anything — an invalid
/// override must leave no queue file behind, mirroring the palette-quantize
/// / datamosh add-time-rejection precedent.
#[test]
fn queue_add_morphogenesis_sequence_rejects_invalid_settings_before_persisting() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-morphogenesis-sequence",
            queue_arg.as_str(),
            carrier_dir.to_string_lossy().as_ref(),
            output_root.to_string_lossy().as_ref(),
            "--frames",
            "3",
            // seed-threshold must be in [0, 1]; out of range fails
            // MorphogenesisSettings::validate at add time.
            "--seed-threshold",
            "5.0",
        ])
        .assert()
        .failure();
    assert!(
        !queue_path.exists(),
        "rejected queue-add must not write a queue file"
    );
}

#[test]
fn queue_datamosh_modulated_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    // A translating modulator so the flow (and therefore a routed amount) has
    // something to scale.
    write_texture_sequence(&modulator_dir, &[0, 2, 4]);
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        write_horizontal_carrier(&carrier_dir.join(frame_name), 24, 16);
    }
    let modulator_wav = temp_dir.path().join("ramp.wav");
    let ramp: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&modulator_wav, 8192, &ramp);

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let wav_arg = modulator_wav.to_string_lossy().to_string();
    let route = "amount=audio-rms:0.5,0.25";
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();

    // Unknown target fails at add time and persists nothing.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-datamosh-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--modulate",
            "block_size=audio-rms",
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown datamosh modulation target",
        ));
    assert!(
        !queue_path.exists(),
        "rejected queue-add must not write a queue file"
    );

    // Direct render: the datamosh queue's fixed manifest rate (30 fps) is the
    // envelope time base, so the direct run must pass --modulation-fps 30.
    let direct_dir = temp_dir.path().join("direct");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-datamosh-sequence",
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            direct_dir.to_string_lossy().as_ref(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--max-frames",
            "3",
            "--modulation-fps",
            "30",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-datamosh-sequence",
            queue_arg.as_str(),
            modulator_arg.as_str(),
            carrier_arg.as_str(),
            output_root_arg.as_str(),
            "--keyframe-interval",
            "0",
            "--amount",
            "1",
            "--max-frames",
            "3",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-datamosh-sequence", queue_arg.as_str()])
        .assert()
        .success();

    assert_png_frames_identical(&direct_dir, &output_root.join("job-0001/frames"), 3);

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let modulation = &manifest["datamosh"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "amount");
    assert_eq!(modulation["routes"][0]["source"], "audio-rms");
    assert_eq!(modulation["routes"][0]["scale"], 0.5);
    assert_eq!(modulation["routes"][0]["offset"], 0.25);
    assert_eq!(modulation["sampling"], "hold");
    assert_eq!(modulation["fps"], 30.0);
    assert_eq!(modulation["modulator_audio"], wav_arg.as_str());
}

#[test]
fn queue_fluid_advect_modulated_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    write_texture_sequence(&source_dir, &[0, 2, 4]);
    let modulator_wav = temp_dir.path().join("ramp.wav");
    let ramp: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&modulator_wav, 8192, &ramp);

    let source_arg = source_dir.to_string_lossy().to_string();
    let wav_arg = modulator_wav.to_string_lossy().to_string();
    let route = "reinject=audio-rms:0.5,0.25";

    let direct_dir = temp_dir.path().join("direct");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-fluid-advect-sequence",
            source_arg.as_str(),
            direct_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--modulation-fps",
            "4",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-fluid-advect-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--frame-rate",
            "4",
            "--modulate",
            route,
            "--modulator-audio",
            wav_arg.as_str(),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-fluid-advect-sequence", queue_arg.as_str()])
        .assert()
        .success();

    assert_png_frames_identical(&direct_dir, &output_root.join("job-0001/frames"), 3);

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let modulation = &manifest["fluid_advect"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "reinject");
    assert_eq!(modulation["routes"][0]["source"], "audio-rms");
    assert_eq!(modulation["routes"][0]["scale"], 0.5);
    assert_eq!(modulation["routes"][0]["offset"], 0.25);
    assert_eq!(modulation["fps"], 4.0);
}

#[test]
fn queue_channel_shift_modulated_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let source_dir = temp_dir.path().join("source-b-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let frame_arg = source_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }

    let source_arg = source_dir.to_string_lossy().to_string();
    let direct_dir = temp_dir.path().join("direct");
    let direct_arg = direct_dir.to_string_lossy().to_string();
    // Luma-driven red shift: the modulator frames are the carrier frames
    // themselves, giving a constant non-zero envelope on the gradient fixture.
    let route = "shift_r_x=luma:12";

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-channel-shift-sequence",
            source_arg.as_str(),
            direct_arg.as_str(),
            "--frames",
            "2",
            "--shift-b-x=-6",
            "--modulate",
            route,
            "--modulator-frames",
            source_arg.as_str(),
            "--modulation-fps",
            "4",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-channel-shift-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--frames",
            "2",
            "--frame-rate",
            "4",
            "--shift-b-x=-6",
            "--modulate",
            route,
            "--modulator-frames",
            source_arg.as_str(),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-channel-shift-sequence", queue_arg.as_str()])
        .assert()
        .success();

    // Queue render is byte-identical to the direct render (path-independent).
    for frame_name in ["frame_000000.png", "frame_000001.png"] {
        assert_eq!(
            fs::read(output_root.join("job-0001/frames").join(frame_name)).expect("queued frame"),
            fs::read(direct_dir.join(frame_name)).expect("direct frame"),
            "queue render must be byte-identical to direct render ({frame_name})"
        );
    }

    // The modulated red shift actually displaced pixels: output differs from
    // the source even though the constant red shift is zero.
    assert_ne!(
        fs::read(output_root.join("job-0001/frames/frame_000000.png")).expect("queued frame"),
        fs::read(source_dir.join("frame_000001.png")).expect("source frame"),
        "luma-routed shift must displace the red channel"
    );

    // Manifest records the channel-shift algorithm, knobs, and routes.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "frame_sequence_channel_shift");
    let effect = &manifest["channel_shift"];
    assert_eq!(effect["algorithm"], "channel_shift_constant_cpu_v1");
    assert_eq!(effect["settings"]["shift_b_x"], -6.0);
    let modulation = &effect["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "shift_r_x");
    assert_eq!(modulation["routes"][0]["source"], "luma");
    assert_eq!(modulation["routes"][0]["scale"], 12.0);
    assert_eq!(modulation["fps"], 4.0);
    assert_eq!(modulation["modulator_frames"], source_arg.as_str());
}

#[test]
fn queue_channel_shift_named_modulators_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let source_dir = temp_dir.path().join("source-b-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        let frame_arg = source_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }

    // Two different envelopes: a rising and a falling amplitude ramp.
    let rise_wav = temp_dir.path().join("rise.wav");
    let rise: Vec<f32> = (0..6144)
        .map(|i| (i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&rise_wav, 8192, &rise);
    let fall_wav = temp_dir.path().join("fall.wav");
    let fall: Vec<f32> = (0..6144)
        .map(|i| (1.0 - i as f32 / 6144.0) * (i as f32 * 0.4).sin())
        .collect();
    write_test_wav_at(&fall_wav, 8192, &fall);

    let source_arg = source_dir.to_string_lossy().to_string();
    let rise_arg = rise_wav.to_string_lossy().to_string();
    let fall_arg = fall_wav.to_string_lossy().to_string();

    let direct_dir = temp_dir.path().join("direct");
    let direct_arg = direct_dir.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-channel-shift-sequence",
            source_arg.as_str(),
            direct_arg.as_str(),
            "--frames",
            "3",
            "--modulate",
            "shift_r_x=rise.audio-rms:12,0",
            "--modulate",
            "shift_b_y=fall.audio-rms:12,0",
            "--named-modulator-audio",
            &format!("rise={rise_arg}"),
            "--named-modulator-audio",
            &format!("fall={fall_arg}"),
            "--modulation-fps",
            "4",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-channel-shift-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--frames",
            "3",
            "--frame-rate",
            "4",
            "--modulate",
            "shift_r_x=rise.audio-rms:12,0",
            "--modulate",
            "shift_b_y=fall.audio-rms:12,0",
            "--named-modulator-audio",
            &format!("rise={rise_arg}"),
            "--named-modulator-audio",
            &format!("fall={fall_arg}"),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-channel-shift-sequence", queue_arg.as_str()])
        .assert()
        .success();

    // Byte-identical add→run vs the direct render (path-independent).
    assert_png_frames_identical(&direct_dir, &output_root.join("job-0001/frames"), 3);

    // Manifest records both routes WITH their modulator names.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let modulation = &manifest["channel_shift"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "shift_r_x");
    assert_eq!(modulation["routes"][0]["modulator"], "rise");
    assert_eq!(modulation["routes"][1]["target"], "shift_b_y");
    assert_eq!(modulation["routes"][1]["modulator"], "fall");

    // The persisted job also records the named-modulator media itself, in
    // given order.
    let queue_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queue"))
            .expect("parse queue json");
    let task = &queue_json["jobs"][0]["task"];
    assert_eq!(task["named_modulator_audio"][0]["name"], "rise");
    assert_eq!(task["named_modulator_audio"][0]["path"], rise_arg.as_str());
    assert_eq!(task["named_modulator_audio"][1]["name"], "fall");
    assert_eq!(task["named_modulator_audio"][1]["path"], fall_arg.as_str());
}

#[test]
fn queue_channel_shift_named_modulator_missing_media_rejects_at_add_and_persists_nothing() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let source_dir = temp_dir.path().join("source-b-frames");
    let frame_arg = source_dir
        .join("frame_000001.png")
        .to_string_lossy()
        .to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["render-test", frame_arg.as_str()])
        .assert()
        .success();
    let source_arg = source_dir.to_string_lossy().to_string();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root_arg = temp_dir.path().join("out").to_string_lossy().to_string();
    // "rise" is referenced by the route but never supplied via
    // --named-modulator-audio: add-time validation must reject this before
    // the job (or the queue file) is ever written.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-channel-shift-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--frames",
            "1",
            "--frame-rate",
            "4",
            "--modulate",
            "shift_r_x=rise.audio-rms",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "requires --named-modulator-audio rise=<path>",
        ));
    assert!(!queue_path.exists());
}

#[test]
fn modulation_route_and_task_named_fields_skip_serialization_when_unset() {
    use morphogen_core::{
        ModulationSampling as CoreModulationSampling, ModulationSource as CoreModulationSource,
        NamedModulatorMedia, RenderBackend, RenderJobModulationRoute, RenderJobTask,
    };

    // An unset `modulator` on a route is skipped, not serialized as `null`,
    // and round-trips exactly.
    let route = RenderJobModulationRoute {
        target: "shift_r_x".to_string(),
        source: CoreModulationSource::AudioRms,
        scale: 12.0,
        offset: 0.0,
        sampling: None,
        modulator: None,
    };
    let route_json = serde_json::to_string(&route).expect("serialize route");
    assert!(
        !route_json.contains("modulator"),
        "unset modulator must be skipped from the JSON: {route_json}"
    );
    let decoded_route: RenderJobModulationRoute =
        serde_json::from_str(&route_json).expect("deserialize route");
    assert_eq!(decoded_route, route);

    // Empty named-modulator vectors on a task are likewise skipped, so
    // pre-slice queue JSON/manifests stay byte-identical.
    let task = RenderJobTask::FrameSequenceChannelShift {
        carrier_frame_directory: "/tmp/car".to_string(),
        output_directory: "/tmp/out".to_string(),
        frames: 2,
        frame_rate: 24.0,
        shift_r_x: 0.0,
        shift_r_y: 0.0,
        shift_g_x: 0.0,
        shift_g_y: 0.0,
        shift_b_x: 0.0,
        shift_b_y: 0.0,
        flow_source_frame_directory: None,
        flow_gain: 0.0,
        flow_radius: 4,
        backend: RenderBackend::Cpu,
        modulation_routes: Vec::new(),
        modulator_audio_path: None,
        modulator_frames_directory: None,
        modulation_sampling: CoreModulationSampling::Hold,
        named_modulator_audio: Vec::new(),
        named_modulator_frames: Vec::new(),
        modulator_midi_path: None,
        named_modulator_midi: Vec::new(),
        matte_source: None,
        matte_frames: None,
        matte_gain: None,
        output_bit_depth: 8,
    };
    let task_json = serde_json::to_string(&task).expect("serialize task");
    assert!(
        !task_json.contains("named_modulator_audio")
            && !task_json.contains("named_modulator_frames")
            && !task_json.contains("named_modulator_midi"),
        "empty named-modulator vectors must be skipped from the JSON: {task_json}"
    );
    let decoded_task: RenderJobTask = serde_json::from_str(&task_json).expect("deserialize task");
    assert_eq!(decoded_task, task);

    // NamedModulatorMedia itself (used once a route names a modulator) round-trips.
    let media = NamedModulatorMedia {
        name: "rise".to_string(),
        path: "/tmp/rise.wav".to_string(),
    };
    let media_json = serde_json::to_string(&media).expect("serialize media");
    let decoded_media: NamedModulatorMedia =
        serde_json::from_str(&media_json).expect("deserialize media");
    assert_eq!(decoded_media, media);
}

#[test]
fn queue_palette_quantize_modulated_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let source_dir = temp_dir.path().join("source-b-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        let frame_arg = source_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }

    let source_arg = source_dir.to_string_lossy().to_string();
    let direct_dir = temp_dir.path().join("direct");
    let direct_arg = direct_dir.to_string_lossy().to_string();
    // The modulator frames are the carrier frames themselves, giving a
    // constant envelope of 1.0 (peak-relative luma) on the gradient fixture:
    // levels = round(1.0 * 6 + 2) = 8 (visible posterize), and the enum route
    // holds mode at variant index 0 (posterize).
    let levels_route = "levels=luma:6,2";
    let mode_route = "mode=luma:0,0";

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-palette-quantize-sequence",
            source_arg.as_str(),
            direct_arg.as_str(),
            "--frames",
            "2",
            "--modulate",
            levels_route,
            "--modulate",
            mode_route,
            "--modulator-frames",
            source_arg.as_str(),
            "--modulation-fps",
            "4",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-palette-quantize-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--frames",
            "2",
            "--frame-rate",
            "4",
            "--modulate",
            levels_route,
            "--modulate",
            mode_route,
            "--modulator-frames",
            source_arg.as_str(),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-palette-quantize-sequence", queue_arg.as_str()])
        .assert()
        .success();

    // Queue render is byte-identical to the direct render (path-independent).
    for frame_name in ["frame_000000.png", "frame_000001.png"] {
        assert_eq!(
            fs::read(output_root.join("job-0001/frames").join(frame_name)).expect("queued frame"),
            fs::read(direct_dir.join(frame_name)).expect("direct frame"),
            "queue render must be byte-identical to direct render ({frame_name})"
        );
    }

    // The routed levels actually posterized: output differs from the source
    // even though the static settings are the levels-256 passthrough.
    assert_ne!(
        fs::read(output_root.join("job-0001/frames/frame_000000.png")).expect("queued frame"),
        fs::read(source_dir.join("frame_000001.png")).expect("source frame"),
        "luma-routed levels must posterize the gradient"
    );

    // Manifest records the palette-quantize algorithm, knobs, and both routes.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "frame_sequence_palette_quantize");
    let effect = &manifest["palette_quantize"];
    assert_eq!(effect["algorithm"], "palette_quantize_posterize_cpu_v1");
    assert_eq!(effect["settings"]["mode"], "posterize");
    assert_eq!(effect["settings"]["levels"], 256);
    let modulation = &effect["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "levels");
    assert_eq!(modulation["routes"][0]["source"], "luma");
    assert_eq!(modulation["routes"][0]["scale"], 6.0);
    assert_eq!(modulation["routes"][0]["offset"], 2.0);
    assert_eq!(modulation["routes"][1]["target"], "mode");
    assert_eq!(modulation["fps"], 4.0);
    assert_eq!(modulation["modulator_frames"], source_arg.as_str());
}

#[test]
fn queue_rutt_etra_modulated_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let source_dir = temp_dir.path().join("source-b-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        write_horizontal_carrier(&source_dir.join(frame_name), 24, 16);
    }

    let source_arg = source_dir.to_string_lossy().to_string();
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    let depth_route = "displacement_depth=luma:6,2";

    // Unknown target fails at add time and persists nothing.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-rutt-etra-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--modulate",
            "mono=luma",
            "--modulator-frames",
            source_arg.as_str(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown rutt-etra modulation target",
        ));
    assert!(
        !queue_path.exists(),
        "rejected queue-add must not write a queue file"
    );

    // Direct render with the same knobs + route (fps 4 = the job frame rate).
    let direct_dir = temp_dir.path().join("direct");
    let direct_arg = direct_dir.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_arg.as_str(),
            direct_arg.as_str(),
            "--frames",
            "2",
            "--line-pitch",
            "4",
            "--modulate",
            depth_route,
            "--modulator-frames",
            source_arg.as_str(),
            "--modulation-fps",
            "4",
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-rutt-etra-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--frames",
            "2",
            "--frame-rate",
            "4",
            "--line-pitch",
            "4",
            "--modulate",
            depth_route,
            "--modulator-frames",
            source_arg.as_str(),
        ])
        .assert()
        .success();

    // Persisted job JSON shape: knobs + routes present, the named-modulator
    // vectors absent when empty (pre-slice queue JSON stays byte-identical).
    let queue_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queue"))
            .expect("parse queue");
    let task = &queue_json["jobs"][0]["task"];
    assert_eq!(task["type"], "frame_sequence_rutt_etra");
    assert_eq!(task["line_pitch"], 4);
    assert_eq!(task["displacement_depth"], 48.0);
    assert_eq!(task["line_thickness"], 1);
    assert_eq!(task["mono"], false);
    assert_eq!(task["modulation_routes"][0]["target"], "displacement_depth");
    assert_eq!(task["modulation_routes"][0]["source"], "luma");
    assert_eq!(task["modulation_routes"][0]["scale"], 6.0);
    assert_eq!(task["modulation_routes"][0]["offset"], 2.0);
    assert!(
        task.get("named_modulator_audio").is_none(),
        "empty named-modulator vectors must be skipped in the persisted JSON"
    );
    assert!(task.get("named_modulator_frames").is_none());

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-rutt-etra-sequence", queue_arg.as_str()])
        .assert()
        .success();

    // Queue render is byte-identical to the direct render (path-independent),
    // frames AND the render manifest.
    for file_name in ["frame_000000.png", "frame_000001.png", "manifest.json"] {
        assert_eq!(
            fs::read(output_root.join("job-0001/frames").join(file_name)).expect("queued file"),
            fs::read(direct_dir.join(file_name)).expect("direct file"),
            "queue render must be byte-identical to direct render ({file_name})"
        );
    }

    // Job manifest records the rutt-etra algorithm, knobs, and the route.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    assert_eq!(manifest["task"], "frame_sequence_rutt_etra");
    let effect = &manifest["rutt_etra"];
    assert_eq!(effect["algorithm"], "rutt_etra_scanline_cpu_v1");
    assert_eq!(effect["settings"]["line_pitch"], 4);
    assert_eq!(effect["settings"]["displacement_depth"], 48.0);
    assert_eq!(effect["settings"]["line_thickness"], 1);
    assert_eq!(effect["settings"]["mono"], false);
    let modulation = &effect["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "displacement_depth");
    assert_eq!(modulation["routes"][0]["source"], "luma");
    assert_eq!(modulation["routes"][0]["scale"], 6.0);
    assert_eq!(modulation["routes"][0]["offset"], 2.0);
    assert_eq!(modulation["fps"], 4.0);
    assert_eq!(modulation["modulator_frames"], source_arg.as_str());
}

#[test]
fn queue_rutt_etra_lfo_modulated_matches_direct_without_media() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let source_dir = temp_dir.path().join("source-b-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png"] {
        write_horizontal_carrier(&source_dir.join(frame_name), 24, 16);
    }

    let source_arg = source_dir.to_string_lossy().to_string();
    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    // saw at 0.5 Hz, phase 0.25, fps 4: p = 0.25, 0.375 — a distinct depth
    // per frame. All literals exactly representable in f32, so the queue's
    // spec_text reconstruction round-trips bit-for-bit.
    let depth_route = "displacement_depth=lfo(saw,0.5,0.25):64,-16";

    // An unknown target on an LFO route still fails at add time and
    // persists nothing.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-rutt-etra-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--modulate",
            "mono=lfo(sine)",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown rutt-etra modulation target",
        ));
    assert!(
        !queue_path.exists(),
        "rejected queue-add must not write a queue file"
    );

    // Direct render: the LFO route needs NO --modulator-* flags at all.
    let direct_dir = temp_dir.path().join("direct");
    let direct_arg = direct_dir.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            source_arg.as_str(),
            direct_arg.as_str(),
            "--frames",
            "2",
            "--line-pitch",
            "4",
            "--modulate",
            depth_route,
            "--modulation-fps",
            "4",
        ])
        .assert()
        .success();

    // Queue-add likewise accepts the LFO route without modulator media.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-rutt-etra-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--frames",
            "2",
            "--frame-rate",
            "4",
            "--line-pitch",
            "4",
            "--modulate",
            depth_route,
        ])
        .assert()
        .success();

    // Persisted job JSON: the LFO source is an object on the existing route
    // field (no new task fields); no modulator media paths were demanded.
    let queue_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queue"))
            .expect("parse queue");
    let task = &queue_json["jobs"][0]["task"];
    assert_eq!(task["type"], "frame_sequence_rutt_etra");
    let route = &task["modulation_routes"][0];
    assert_eq!(route["target"], "displacement_depth");
    assert_eq!(route["source"]["lfo"]["shape"], "saw");
    assert_eq!(route["source"]["lfo"]["rate_hz"], 0.5);
    assert_eq!(route["source"]["lfo"]["phase"], 0.25);
    assert_eq!(route["scale"], 64.0);
    assert_eq!(route["offset"], -16.0);
    assert!(task["modulator_audio_path"].is_null());
    assert!(task["modulator_frames_directory"].is_null());

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-rutt-etra-sequence", queue_arg.as_str()])
        .assert()
        .success();

    // Queue render is byte-identical to the direct render (the spec
    // reconstruction round-trips `lfo(...)` exactly), frames AND manifest.
    for file_name in ["frame_000000.png", "frame_000001.png", "manifest.json"] {
        assert_eq!(
            fs::read(output_root.join("job-0001/frames").join(file_name)).expect("queued file"),
            fs::read(direct_dir.join(file_name)).expect("direct file"),
            "queue render must be byte-identical to direct render ({file_name})"
        );
    }

    // Job manifest records the LFO route with no modulator media.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let modulation = &manifest["rutt_etra"]["modulation"];
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["shape"], "saw");
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["rate_hz"], 0.5);
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["phase"], 0.25);
    assert_eq!(modulation["fps"], 4.0);
    assert!(modulation["modulator_audio"].is_null());
    assert!(modulation["modulator_frames"].is_null());
}

#[test]
fn downscale_frames_two_runs_are_byte_identical() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    fs::create_dir_all(&source_dir).expect("create source frames");
    // 5x5 is not evenly divisible by scale 2, exercising the edge-clip path.
    for index in 0..2u32 {
        let frame = ImageBuffer::from_fn(5, 5, |x, y| {
            let value = (x as u8)
                .wrapping_mul(24)
                .wrapping_add(y as u8 * 7 + index as u8);
            Rgba([value, value.wrapping_add(index as u8 * 11), 40, u8::MAX])
        });
        frame
            .save(source_dir.join(format!("frame_{index:06}.png")))
            .expect("write source frame");
    }

    let source_arg = source_dir.to_string_lossy().to_string();
    let run = |output_dir: &Path| {
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args([
                "downscale-frames",
                source_arg.as_str(),
                output_dir.to_string_lossy().as_ref(),
                "--scale",
                "2",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("box_downscale_cpu_v1"));
    };

    let first_dir = temp_dir.path().join("first-run");
    let second_dir = temp_dir.path().join("second-run");
    run(&first_dir);
    run(&second_dir);

    assert_png_frames_identical(&first_dir, &second_dir, 2);
}

#[test]
fn downscale_frames_feeds_rutt_etra_sequence_at_reduced_dimensions() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let source_dir = temp_dir.path().join("source-frames");
    let downscaled_dir = temp_dir.path().join("downscaled-frames");
    let output_dir = temp_dir.path().join("rutt-etra-frames");
    fs::create_dir_all(&source_dir).expect("create source frames");

    for index in 0..2u32 {
        let frame = ImageBuffer::from_fn(16, 16, |x, _| {
            let value = (x as u8).wrapping_mul(16).wrapping_add(index as u8);
            Rgba([value, value, value, u8::MAX])
        });
        frame
            .save(source_dir.join(format!("frame_{index:06}.png")))
            .expect("write source frame");
    }

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "downscale-frames",
            source_dir.to_string_lossy().as_ref(),
            downscaled_dir.to_string_lossy().as_ref(),
            "--scale",
            "4",
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-rutt-etra-sequence",
            downscaled_dir.to_string_lossy().as_ref(),
            output_dir.to_string_lossy().as_ref(),
            "--frames",
            "2",
        ])
        .assert()
        .success();

    for index in 0..2 {
        let frame_path = output_dir.join(format!("frame_{index:06}.png"));
        let decoded = image::ImageReader::open(&frame_path)
            .expect("open rendered frame")
            .decode()
            .expect("decode rendered frame");
        assert_eq!(
            (decoded.width(), decoded.height()),
            (4, 4),
            "16x16 source at scale 4 must render at the downscaled 4x4 dimensions"
        );
    }
}

// ── Tier 5.3 S2: MIDI modulation on the queue path + checkpoint contract ────
// (`docs/MIDI_MODULATION_MILESTONE.md`, anchors 4-5). Mirrors the named-
// modulator-audio queue precedent (`queue_channel_shift_named_modulators_
// matches_direct_and_records_routes` et al.) with a `midi-cc(74)` route.

#[test]
fn queue_channel_shift_named_modulator_midi_matches_direct_and_records_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let source_dir = temp_dir.path().join("source-b-frames");
    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        let frame_arg = source_dir.join(frame_name).to_string_lossy().to_string();
        Command::cargo_bin("morphogen")
            .expect("morphogen binary")
            .args(["render-test", frame_arg.as_str()])
            .assert()
            .success();
    }

    // CC 74 ramps 0 -> 64 -> 127 at ticks 240/480 (120 BPM default: quarter
    // note = 0.5s, so the events land at 0.25s/0.5s — spanning the three
    // output frames at --frame-rate 4).
    let midi_path = temp_dir.path().join("ramp.mid");
    write_test_smf(
        &midi_path,
        &[
            (0, midi_control_change(0, 74, 0)),
            (240, midi_control_change(0, 74, 64)),
            (240, midi_control_change(0, 74, 127)),
        ],
    );

    let source_arg = source_dir.to_string_lossy().to_string();
    let midi_arg = midi_path.to_string_lossy().to_string();
    let route = "shift_r_x=ramp.midi-cc(74):12,0";

    let direct_dir = temp_dir.path().join("direct");
    let direct_arg = direct_dir.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-channel-shift-sequence",
            source_arg.as_str(),
            direct_arg.as_str(),
            "--frames",
            "3",
            "--modulate",
            route,
            "--named-modulator-midi",
            &format!("ramp={midi_arg}"),
            "--modulation-fps",
            "4",
        ])
        .assert()
        .success();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root = temp_dir.path().join("out");
    let output_root_arg = output_root.to_string_lossy().to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-channel-shift-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--frames",
            "3",
            "--frame-rate",
            "4",
            "--modulate",
            route,
            "--named-modulator-midi",
            &format!("ramp={midi_arg}"),
        ])
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["queue-run-channel-shift-sequence", queue_arg.as_str()])
        .assert()
        .success();

    // Byte-identical add→run vs the direct render (path-independent) —
    // anchor 4.
    assert_png_frames_identical(&direct_dir, &output_root.join("job-0001/frames"), 3);

    // Manifest records the route with its modulator name.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(output_root.join("job-0001/manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    let modulation = &manifest["channel_shift"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "shift_r_x");
    assert_eq!(modulation["routes"][0]["source"]["midi-cc"], 74);
    assert_eq!(modulation["routes"][0]["modulator"], "ramp");

    // The persisted job also records the named-modulator MIDI media itself.
    let queue_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&queue_path).expect("read queue"))
            .expect("parse queue json");
    let task = &queue_json["jobs"][0]["task"];
    assert_eq!(task["named_modulator_midi"][0]["name"], "ramp");
    assert_eq!(task["named_modulator_midi"][0]["path"], midi_arg.as_str());
}

#[test]
fn queue_channel_shift_named_modulator_midi_missing_media_rejects_at_add_and_persists_nothing() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let source_dir = temp_dir.path().join("source-b-frames");
    let frame_arg = source_dir
        .join("frame_000001.png")
        .to_string_lossy()
        .to_string();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(["render-test", frame_arg.as_str()])
        .assert()
        .success();
    let source_arg = source_dir.to_string_lossy().to_string();

    let queue_path = temp_dir.path().join("queue.json");
    let queue_arg = queue_path.to_string_lossy().to_string();
    let output_root_arg = temp_dir.path().join("out").to_string_lossy().to_string();
    // "ramp" is referenced by the route but never supplied via
    // --named-modulator-midi: add-time validation must reject this before
    // the job (or the queue file) is ever written — the S1 rejection this
    // slice reverses becomes a real media-presence check instead.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "queue-add-channel-shift-sequence",
            queue_arg.as_str(),
            source_arg.as_str(),
            output_root_arg.as_str(),
            "--frames",
            "1",
            "--frame-rate",
            "4",
            "--modulate",
            "shift_r_x=ramp.midi-cc(74)",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "requires --named-modulator-midi ramp=<path>",
        ));
    assert!(!queue_path.exists());
}

#[test]
fn render_feedback_sequence_midi_route_joins_checkpoint_contract_and_refuses_on_changed_content() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let modulator_dir = temp_dir.path().join("modulator-frames");
    let carrier_dir = temp_dir.path().join("carrier-frames");

    for frame_name in ["frame_000001.png", "frame_000002.png", "frame_000003.png"] {
        for dir in [&modulator_dir, &carrier_dir] {
            let frame_arg = dir.join(frame_name).to_string_lossy().to_string();
            Command::cargo_bin("morphogen")
                .expect("morphogen binary")
                .args(["render-test", frame_arg.as_str()])
                .assert()
                .success();
        }
    }

    // CC 74 ramps 0 -> 64 -> 127 at ticks 240/480 (120 BPM default: quarter
    // note = 0.5s, so the events land at 0.25s/0.5s), driving feedback_mix
    // across the three output frames at --frame-rate 4.
    let midi_path = temp_dir.path().join("ramp.mid");
    write_test_smf(
        &midi_path,
        &[
            (0, midi_control_change(0, 74, 0)),
            (240, midi_control_change(0, 74, 64)),
            (240, midi_control_change(0, 74, 127)),
        ],
    );

    let modulator_arg = modulator_dir.to_string_lossy().to_string();
    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let midi_arg = midi_path.to_string_lossy().to_string();
    let route = "feedback_mix=midi-cc(74):0.5,0.25";
    let base_args = |output_dir: &str| {
        vec![
            "render-feedback-sequence".to_string(),
            modulator_arg.clone(),
            carrier_arg.clone(),
            output_dir.to_string(),
            "--carrier-amount".to_string(),
            "8".to_string(),
            "--feedback-amount".to_string(),
            "12".to_string(),
            "--feedback-mix".to_string(),
            "0.7".to_string(),
            "--decay".to_string(),
            "0.95".to_string(),
            "--max-frames".to_string(),
            "3".to_string(),
            "--frame-rate".to_string(),
            "4".to_string(),
            "--flow-source".to_string(),
            "luminance".to_string(),
        ]
    };
    let modulated_args = |output_dir: &str| {
        let mut args = base_args(output_dir);
        args.extend([
            "--modulate".to_string(),
            route.to_string(),
            "--modulator-midi".to_string(),
            midi_arg.clone(),
        ]);
        args
    };

    let off_dir = temp_dir.path().join("off-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&off_dir.to_string_lossy()))
        .assert()
        .success();
    let uninterrupted_dir = temp_dir.path().join("uninterrupted-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(modulated_args(&uninterrupted_dir.to_string_lossy()))
        .assert()
        .success();

    // The route actually drives the state evolution.
    assert_ne!(
        fs::read(uninterrupted_dir.join("frames/frame_000002.png")).expect("modulated frame"),
        fs::read(off_dir.join("frames/frame_000002.png")).expect("unmodulated frame"),
        "routed midi-cc feedback_mix must change the rendered sequence"
    );

    let resumed_dir = temp_dir.path().join("resumed-output");
    let resumed_args = modulated_args(&resumed_dir.to_string_lossy());
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .arg("--stop-after-frame")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed flow-feedback sequence after frame 0",
        ));

    // The checkpoint's contract carries the MIDI file's content fingerprint
    // (anchor 5) alongside the route.
    let checkpoint_path = resumed_dir.join("checkpoint.json");
    let checkpoint: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&checkpoint_path).expect("read checkpoint"))
            .expect("parse checkpoint");
    let modulation = &checkpoint["contract"]["modulation"];
    assert_eq!(modulation["routes"][0]["source"]["midi-cc"], 74);
    assert_eq!(modulation["modulator_midi"]["path"], midi_arg.as_str());
    assert!(modulation["modulator_midi"]["checksum"]
        .as_str()
        .expect("midi checksum")
        .starts_with("fnv1a64:"));

    // Changing the MIDI file's content at the same path must refuse to
    // resume — the knob history would differ.
    write_test_smf(
        &midi_path,
        &[
            (0, midi_control_change(0, 74, 127)),
            (240, midi_control_change(0, 74, 64)),
            (240, midi_control_change(0, 74, 0)),
        ],
    );
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));

    // Restoring the original MIDI content lets the resume proceed, and the
    // result is byte-identical to the uninterrupted render.
    write_test_smf(
        &midi_path,
        &[
            (0, midi_control_change(0, 74, 0)),
            (240, midi_control_change(0, 74, 64)),
            (240, midi_control_change(0, 74, 127)),
        ],
    );
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered flow-feedback sequence with 3 frame(s)",
        ));
    for frame_name in ["frame_000000.png", "frame_000001.png", "frame_000002.png"] {
        assert_eq!(
            fs::read(resumed_dir.join("frames").join(frame_name)).expect("resumed frame"),
            fs::read(uninterrupted_dir.join("frames").join(frame_name))
                .expect("uninterrupted frame"),
            "resumed modulated render must be byte-identical ({frame_name})"
        );
    }
}

/// `render-morphogenesis-field` (Tier "Morphogenesis" S1): checkpoint/resume
/// mirrors `render-feedback-sequence` exactly — a partial checkpoint after
/// `--stop-after-frame`, a resumed run byte-identical to an uninterrupted one,
/// and a settings change refusing to resume the same output directory.
#[test]
fn render_morphogenesis_field_checkpoints_resumes_and_refuses_stale_settings() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let resumed_dir = temp_dir.path().join("resumed-output");
    let uninterrupted_dir = temp_dir.path().join("uninterrupted-output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "1",
            "--rate",
            "0",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let resumed_arg = resumed_dir.to_string_lossy().to_string();
    let uninterrupted_arg = uninterrupted_dir.to_string_lossy().to_string();
    let morphogenesis_args = [
        "render-morphogenesis-field",
        carrier_arg.as_str(),
        resumed_arg.as_str(),
        "--frames",
        "3",
        "--preset",
        "coral",
    ];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(morphogenesis_args)
        .arg("--stop-after-frame")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed morphogenesis field after frame 0",
        ));

    let partial_checkpoint = read_json(&resumed_dir.join("checkpoint.json"));
    assert_eq!(partial_checkpoint["task"], "render_morphogenesis_field");
    assert_eq!(partial_checkpoint["status"], "running");
    assert_eq!(partial_checkpoint["next_frame_index"], 1);
    assert!(resumed_dir
        .join("state/morphogenesis_frame_000000.rgba32f")
        .exists());
    assert!(resumed_dir.join("frames/frame_000000.png").exists());
    assert!(!resumed_dir.join("manifest.json").exists());

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(morphogenesis_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered morphogenesis field with 3 frame(s)",
        ));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-field",
            carrier_arg.as_str(),
            uninterrupted_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
        ])
        .assert()
        .success();

    let final_checkpoint = read_json(&resumed_dir.join("checkpoint.json"));
    assert_eq!(final_checkpoint["status"], "complete");
    assert_eq!(final_checkpoint["next_frame_index"], 3);
    let manifest = read_json(&resumed_dir.join("manifest.json"));
    assert_eq!(manifest["task"], "render_morphogenesis_field");
    assert_eq!(manifest["algorithm"], "morphogenesis_cpu_v1");

    for index in 0..3 {
        let frame_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(resumed_dir.join("frames").join(&frame_name)).expect("resumed frame"),
            fs::read(uninterrupted_dir.join("frames").join(&frame_name))
                .expect("uninterrupted frame"),
            "resumed morphogenesis field must be byte-identical to an uninterrupted run ({frame_name})"
        );
    }

    // Changed settings must refuse to resume the (now-complete) output dir.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-field",
            carrier_arg.as_str(),
            resumed_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "worms",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));
}

/// `render-morphogenesis-sequence` (Tier "Morphogenesis" S2): anchor A1 —
/// `--pattern-mix 0 --displace 0` must reproduce Source B byte-for-byte,
/// regardless of what the field is doing underneath.
#[test]
fn render_morphogenesis_sequence_anchor_a1_passthrough_matches_source_b() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let output_dir = temp_dir.path().join("output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--pattern-mix",
            "0",
            "--displace",
            "0",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered morphogenesis sequence with 3 frame(s)",
        ));

    for index in 0..3 {
        let frame_name = format!("frame_{index:06}.png");
        let rendered =
            fs::read(output_dir.join("frames").join(&frame_name)).expect("rendered frame");
        let source = fs::read(carrier_dir.join(&frame_name)).expect("source frame");
        assert_eq!(
            rendered, source,
            "A1: pattern-mix=0 && displace=0 must reproduce source B byte-for-byte ({frame_name})"
        );
    }

    let manifest = read_json(&output_dir.join("manifest.json"));
    assert_eq!(manifest["task"], "render_morphogenesis_sequence");
    assert_eq!(manifest["algorithm"], "morphogenesis_cpu_v1");
}

/// `render-morphogenesis-sequence`: checkpoint/resume mirrors
/// `render-morphogenesis-field` exactly — a partial checkpoint after
/// `--stop-after-frame`, a resumed run byte-identical to an uninterrupted
/// one, and a changed COMPOSITE knob (not just a field setting) refusing to
/// resume, since the composite consumes Source B every frame.
#[test]
fn render_morphogenesis_sequence_checkpoints_resumes_and_refuses_stale_composite_knob() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let resumed_dir = temp_dir.path().join("resumed-output");
    let uninterrupted_dir = temp_dir.path().join("uninterrupted-output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let resumed_arg = resumed_dir.to_string_lossy().to_string();
    let uninterrupted_arg = uninterrupted_dir.to_string_lossy().to_string();
    let morphogenesis_args = [
        "render-morphogenesis-sequence",
        carrier_arg.as_str(),
        resumed_arg.as_str(),
        "--frames",
        "3",
        "--preset",
        "coral",
        "--pattern-mix",
        "0.85",
        "--displace",
        "3",
    ];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(morphogenesis_args)
        .arg("--stop-after-frame")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed morphogenesis sequence after frame 0",
        ));

    let partial_checkpoint = read_json(&resumed_dir.join("checkpoint.json"));
    assert_eq!(partial_checkpoint["task"], "render_morphogenesis_sequence");
    assert_eq!(partial_checkpoint["status"], "running");
    assert_eq!(partial_checkpoint["next_frame_index"], 1);
    assert!(resumed_dir
        .join("state/morphogenesis_sequence_frame_000000.rgba32f")
        .exists());
    assert!(resumed_dir.join("frames/frame_000000.png").exists());
    assert!(!resumed_dir.join("manifest.json").exists());

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(morphogenesis_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered morphogenesis sequence with 3 frame(s)",
        ));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            uninterrupted_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--pattern-mix",
            "0.85",
            "--displace",
            "3",
        ])
        .assert()
        .success();

    let final_checkpoint = read_json(&resumed_dir.join("checkpoint.json"));
    assert_eq!(final_checkpoint["status"], "complete");
    assert_eq!(final_checkpoint["next_frame_index"], 3);
    let manifest = read_json(&resumed_dir.join("manifest.json"));
    assert_eq!(manifest["task"], "render_morphogenesis_sequence");
    assert_eq!(manifest["algorithm"], "morphogenesis_cpu_v1");

    for index in 0..3 {
        let frame_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(resumed_dir.join("frames").join(&frame_name)).expect("resumed frame"),
            fs::read(uninterrupted_dir.join("frames").join(&frame_name))
                .expect("uninterrupted frame"),
            "resumed morphogenesis sequence must be byte-identical to an uninterrupted run ({frame_name})"
        );
    }

    // A changed COMPOSITE knob (pattern-mix), not a field setting, must also
    // refuse to resume the (now-complete) output dir — the composite knobs
    // are part of the checkpoint contract.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            resumed_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--pattern-mix",
            "0.5",
            "--displace",
            "3",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));
}

/// `render-morphogenesis-sequence` (Tier "Morphogenesis" S3): a `feed`
/// LFO route joins the checkpoint contract exactly like flow-feedback's
/// (`FeedbackModulationContract` reused verbatim) — mirrors
/// `render_feedback_sequence_lfo_route_joins_checkpoint_contract`'s shape.
#[test]
fn render_morphogenesis_sequence_lfo_route_joins_checkpoint_contract() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    // sine at 1 Hz, fps 4: p = 0, 0.25, 0.5 across the three frames — a
    // distinct routed `feed` per frame (0.03, 0.04, 0.05), so frame N's field
    // state consumed frames 0..N's values (what resume must reproduce). All
    // literals are exactly representable in f32.
    let route = "feed=lfo(sine,1):0.02,0.03";
    let base_args = |output_dir: &str| {
        vec![
            "render-morphogenesis-sequence".to_string(),
            carrier_arg.clone(),
            output_dir.to_string(),
            "--frames".to_string(),
            "3".to_string(),
            "--preset".to_string(),
            "coral".to_string(),
            "--pattern-mix".to_string(),
            "0.85".to_string(),
            "--displace".to_string(),
            "0".to_string(),
            "--frame-rate".to_string(),
            "4".to_string(),
        ]
    };
    let modulated_args = |output_dir: &str| {
        let mut args = base_args(output_dir);
        args.extend(["--modulate".to_string(), route.to_string()]);
        args
    };

    // Unmodulated reference (the off case) and the uninterrupted modulated
    // render the resumed one must match.
    let off_dir = temp_dir.path().join("off-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&off_dir.to_string_lossy()))
        .assert()
        .success();
    let uninterrupted_dir = temp_dir.path().join("uninterrupted-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(modulated_args(&uninterrupted_dir.to_string_lossy()))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "modulation routes: feed=lfo(sine,1,0):0.02,0.03",
        ));

    // The route actually drives the field evolution.
    assert_ne!(
        fs::read(uninterrupted_dir.join("frames/frame_000002.png")).expect("modulated frame"),
        fs::read(off_dir.join("frames/frame_000002.png")).expect("unmodulated frame"),
        "routed LFO feed must change the rendered sequence"
    );

    // Interrupt after frame 0, then resume with identical arguments.
    let resumed_dir = temp_dir.path().join("resumed-output");
    let resumed_args = modulated_args(&resumed_dir.to_string_lossy());
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .arg("--stop-after-frame")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "checkpointed morphogenesis sequence after frame 0",
        ));

    // The LFO params ride the route inside the checkpoint's modulation block.
    let checkpoint: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(resumed_dir.join("checkpoint.json")).expect("read checkpoint"),
    )
    .expect("parse checkpoint");
    let modulation = &checkpoint["contract"]["modulation"];
    assert_eq!(modulation["routes"][0]["target"], "feed");
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["shape"], "sine");
    assert_eq!(modulation["routes"][0]["source"]["lfo"]["rate_hz"], 1.0);
    assert_eq!(modulation["routes"][0]["scale"], 0.02);
    assert_eq!(modulation["routes"][0]["offset"], 0.03);
    assert_eq!(modulation["fps"], 4.0);
    assert!(modulation["modulator_audio"].is_null());
    assert!(modulation["modulator_frames"].is_null());

    // A changed rate_hz must refuse to resume — the knob history would differ.
    let mut changed_rate_args = base_args(&resumed_dir.to_string_lossy());
    changed_rate_args.extend([
        "--modulate".to_string(),
        "feed=lfo(sine,2):0.02,0.03".to_string(),
    ]);
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&changed_rate_args)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));
    // Dropping the route entirely must refuse for the same reason.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&resumed_dir.to_string_lossy()))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));

    // Identical arguments resume to byte-identity with uninterrupted.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered morphogenesis sequence with 3 frame(s)",
        ));
    for frame_name in ["frame_000000.png", "frame_000001.png", "frame_000002.png"] {
        assert_eq!(
            fs::read(resumed_dir.join("frames").join(frame_name)).expect("resumed frame"),
            fs::read(uninterrupted_dir.join("frames").join(frame_name))
                .expect("uninterrupted frame"),
            "resumed LFO-modulated render must be byte-identical ({frame_name})"
        );
    }

    // A pre-S3 checkpoint (no modulation key at all) deserializes as
    // unmodulated and stays resumable by an unmodulated render.
    let legacy_dir = temp_dir.path().join("legacy-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&legacy_dir.to_string_lossy()))
        .arg("--stop-after-frame")
        .assert()
        .success();
    let checkpoint_path = legacy_dir.join("checkpoint.json");
    let mut legacy_checkpoint: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&checkpoint_path).expect("read legacy"))
            .expect("parse legacy");
    let contract = legacy_checkpoint["contract"]
        .as_object_mut()
        .expect("contract object");
    assert!(contract.remove("modulation").is_some());
    // A pre-S3 checkpoint's settings also predate `param_map_strength` — drop
    // it too so the legacy-shape simulation is complete.
    contract["settings"]
        .as_object_mut()
        .expect("settings object")
        .remove("param_map_strength");
    fs::write(
        &checkpoint_path,
        serde_json::to_string(&legacy_checkpoint).expect("serialize legacy"),
    )
    .expect("write legacy checkpoint");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&legacy_dir.to_string_lossy()))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered morphogenesis sequence with 3 frame(s)",
        ));
}

// ─── Morphogenesis Live Coupling (L-S1): inject/erode ──────────────────────
//
// `docs/MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md` L-S1: per-frame `--inject`/
// `--erode` so the field keeps responding to every carrier frame instead of
// freezing once it fills the domain. Anchors covered here: L1 (explicit
// zero-knob CLI invocation is byte-identical to omitting the flags entirely,
// on both commands), L5 (resume with inject+erode active is byte-identical
// to an uninterrupted run — the prev-luma-in-checkpoint proof — plus changed
// inject/erode refuses, plus a pre-milestone checkpoint missing the new keys
// still resumes with defaults). L2 (motion tracking) and L3 (no-freeze
// headline) are proven at the algorithm-unit level in
// `morphogen-render/src/morphogenesis.rs` and via the empirical tuning pass
// on the real cello showcase fixture (see the milestone doc's L-S1 entry).

#[test]
fn render_morphogenesis_field_inject_erode_zero_matches_the_pre_live_coupling_defaults() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let implicit_dir = temp_dir.path().join("implicit-output");
    let explicit_dir = temp_dir.path().join("explicit-output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.05",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();

    // No --inject/--erode/--inject-source flags at all (the pre-live-
    // coupling CLI surface).
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-field",
            carrier_arg.as_str(),
            implicit_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
        ])
        .assert()
        .success();

    // Explicit zero knobs (anchor L1).
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-field",
            carrier_arg.as_str(),
            explicit_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--inject",
            "0",
            "--erode",
            "0",
            "--inject-source",
            "motion",
        ])
        .assert()
        .success();

    for index in 0..3 {
        let frame_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(implicit_dir.join("frames").join(&frame_name)).expect("implicit frame"),
            fs::read(explicit_dir.join("frames").join(&frame_name)).expect("explicit frame"),
            "L1: explicit --inject 0 --erode 0 must be byte-identical to omitting the flags ({frame_name})"
        );
        let state_name = format!("state/morphogenesis_frame_{index:06}.rgba32f");
        assert_eq!(
            fs::read(implicit_dir.join(&state_name)).expect("implicit state"),
            fs::read(explicit_dir.join(&state_name)).expect("explicit state"),
            "L1: checkpoint state bytes must also match ({state_name})"
        );
    }
}

#[test]
fn render_morphogenesis_sequence_inject_erode_zero_matches_the_pre_live_coupling_defaults() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let implicit_dir = temp_dir.path().join("implicit-output");
    let explicit_dir = temp_dir.path().join("explicit-output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.05",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            implicit_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            explicit_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--inject",
            "0",
            "--erode",
            "0",
            "--inject-source",
            "motion",
        ])
        .assert()
        .success();

    for index in 0..3 {
        let frame_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(implicit_dir.join("frames").join(&frame_name)).expect("implicit frame"),
            fs::read(explicit_dir.join("frames").join(&frame_name)).expect("explicit frame"),
            "L1: explicit --inject 0 --erode 0 must be byte-identical to omitting the flags ({frame_name})"
        );
    }
}

#[test]
fn render_morphogenesis_sequence_resumes_byte_identically_with_inject_and_erode_active() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let resumed_dir = temp_dir.path().join("resumed-output");
    let uninterrupted_dir = temp_dir.path().join("uninterrupted-output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "4",
            "--rate",
            "0.08",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let resumed_arg = resumed_dir.to_string_lossy().to_string();
    let uninterrupted_arg = uninterrupted_dir.to_string_lossy().to_string();
    let morphogenesis_args = [
        "render-morphogenesis-sequence",
        carrier_arg.as_str(),
        resumed_arg.as_str(),
        "--frames",
        "4",
        "--preset",
        "coral",
        "--inject",
        "0.3",
        "--erode",
        "0.2",
        "--inject-source",
        "motion",
    ];

    // Interrupt after frame 0 (motion source's frame-0 all-zero-weight
    // case), then after frame 1 (the first frame where a real prev-luma
    // must have survived the checkpoint round-trip to compute motion `w`).
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(morphogenesis_args)
        .arg("--stop-after-frame")
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(morphogenesis_args)
        .arg("--stop-after-frame")
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(morphogenesis_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered morphogenesis sequence with 4 frame(s)",
        ));

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            uninterrupted_arg.as_str(),
            "--frames",
            "4",
            "--preset",
            "coral",
            "--inject",
            "0.3",
            "--erode",
            "0.2",
            "--inject-source",
            "motion",
        ])
        .assert()
        .success();

    for index in 0..4 {
        let frame_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(resumed_dir.join("frames").join(&frame_name)).expect("resumed frame"),
            fs::read(uninterrupted_dir.join("frames").join(&frame_name))
                .expect("uninterrupted frame"),
            "L5: resuming with inject/erode active must be byte-identical to an uninterrupted run ({frame_name}) — proves the prev-luma grid survived the checkpoint round-trip"
        );
    }
}

#[test]
fn render_morphogenesis_sequence_refuses_resume_on_changed_inject_or_erode() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let output_dir = temp_dir.path().join("output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.05",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--inject",
            "0.3",
            "--erode",
            "0.2",
        ])
        .arg("--stop-after-frame")
        .assert()
        .success();

    // Changed --inject refuses.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--inject",
            "0.5",
            "--erode",
            "0.2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));

    // Changed --erode refuses too.
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--inject",
            "0.3",
            "--erode",
            "0.6",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));
}

#[test]
fn render_morphogenesis_sequence_legacy_checkpoint_without_live_coupling_keys_resumes_with_defaults(
) {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let legacy_dir = temp_dir.path().join("legacy-output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.05",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let legacy_arg = legacy_dir.to_string_lossy().to_string();
    let base_args = [
        "render-morphogenesis-sequence",
        carrier_arg.as_str(),
        legacy_arg.as_str(),
        "--frames",
        "3",
        "--preset",
        "coral",
    ];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args)
        .arg("--stop-after-frame")
        .assert()
        .success();

    let checkpoint_path = legacy_dir.join("checkpoint.json");
    let mut legacy_checkpoint: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&checkpoint_path).expect("read legacy"))
            .expect("parse legacy");
    let settings = legacy_checkpoint["contract"]["settings"]
        .as_object_mut()
        .expect("settings object");
    assert!(settings.remove("inject").is_some());
    assert!(settings.remove("erode").is_some());
    assert!(settings.remove("inject_source").is_some());
    fs::write(
        &checkpoint_path,
        serde_json::to_string(&legacy_checkpoint).expect("serialize legacy"),
    )
    .expect("write legacy checkpoint");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered morphogenesis sequence with 3 frame(s)",
        ));
}

// ─── Morphogenesis Live Coupling (L-S2): homeostat + mod targets ───────────
//
// `docs/MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md` L-S2: `--coverage-target`
// homeostat (anchors L4/L5) plus `inject`/`erode`/`coverage_target` joining
// the modulation registry. L4 (the mean(V) settling band) is proven at the
// algorithm-unit level in `morphogen-render/src/morphogenesis.rs`
// (`anchor_l4_homeostat_settles_mean_v_within_band_of_coverage_target`); the
// tests here cover the CLI-level contract: off-identity, checkpoint-contract
// join/refusal, legacy-checkpoint compatibility, and the routed-prev-luma
// resume proof (L5) — a route can turn `inject`/`erode` on mid-render even
// when the render's own static `--inject`/`--erode` flags stay at 0.

#[test]
fn render_morphogenesis_field_coverage_target_zero_matches_the_pre_ls2_defaults() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let implicit_dir = temp_dir.path().join("implicit-output");
    let explicit_dir = temp_dir.path().join("explicit-output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.05",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-field",
            carrier_arg.as_str(),
            implicit_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
        ])
        .assert()
        .success();

    // Explicit zero (anchor: identity by construction, no mean(V) computed).
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-field",
            carrier_arg.as_str(),
            explicit_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--coverage-target",
            "0",
        ])
        .assert()
        .success();

    for index in 0..3 {
        let frame_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(implicit_dir.join("frames").join(&frame_name)).expect("implicit frame"),
            fs::read(explicit_dir.join("frames").join(&frame_name)).expect("explicit frame"),
            "explicit --coverage-target 0 must be byte-identical to omitting the flag ({frame_name})"
        );
        let state_name = format!("state/morphogenesis_frame_{index:06}.rgba32f");
        assert_eq!(
            fs::read(implicit_dir.join(&state_name)).expect("implicit state"),
            fs::read(explicit_dir.join(&state_name)).expect("explicit state"),
            "checkpoint state bytes must also match ({state_name})"
        );
    }
}

#[test]
fn render_morphogenesis_sequence_coverage_target_zero_matches_the_pre_ls2_defaults() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let implicit_dir = temp_dir.path().join("implicit-output");
    let explicit_dir = temp_dir.path().join("explicit-output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.05",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            implicit_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
        ])
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            explicit_dir.to_string_lossy().as_ref(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--coverage-target",
            "0",
        ])
        .assert()
        .success();

    for index in 0..3 {
        let frame_name = format!("frame_{index:06}.png");
        assert_eq!(
            fs::read(implicit_dir.join("frames").join(&frame_name)).expect("implicit frame"),
            fs::read(explicit_dir.join("frames").join(&frame_name)).expect("explicit frame"),
            "explicit --coverage-target 0 must be byte-identical to omitting the flag ({frame_name})"
        );
    }
}

#[test]
fn render_morphogenesis_sequence_refuses_resume_on_changed_coverage_target() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let output_dir = temp_dir.path().join("output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.05",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--coverage-target",
            "0.3",
        ])
        .arg("--stop-after-frame")
        .assert()
        .success();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--coverage-target",
            "0.5",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "settings changed; start with a new output directory",
        ));
}

#[test]
fn render_morphogenesis_sequence_legacy_checkpoint_without_coverage_target_resumes_with_default() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let legacy_dir = temp_dir.path().join("legacy-output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.05",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let legacy_arg = legacy_dir.to_string_lossy().to_string();
    let base_args = [
        "render-morphogenesis-sequence",
        carrier_arg.as_str(),
        legacy_arg.as_str(),
        "--frames",
        "3",
        "--preset",
        "coral",
    ];

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args)
        .arg("--stop-after-frame")
        .assert()
        .success();

    // Hand-strip `coverage_target` — a pre-L-S2 checkpoint's shape — and
    // confirm the render still resumes (default 0, off).
    let checkpoint_path = legacy_dir.join("checkpoint.json");
    let mut legacy_checkpoint: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&checkpoint_path).expect("read legacy"))
            .expect("parse legacy");
    let settings = legacy_checkpoint["contract"]["settings"]
        .as_object_mut()
        .expect("settings object");
    assert!(settings.remove("coverage_target").is_some());
    fs::write(
        &checkpoint_path,
        serde_json::to_string(&legacy_checkpoint).expect("serialize legacy"),
    )
    .expect("write legacy checkpoint");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered morphogenesis sequence with 3 frame(s)",
        ));
}

/// The knob-registry half of L-S2: `inject`, `erode`, `coverage_target` are
/// accepted CLI modulation targets end-to-end (parsing, clamping, checkpoint
/// contract), not just algorithm-level `apply_morphogenesis_modulation`
/// match arms.
#[test]
fn render_morphogenesis_sequence_accepts_inject_erode_coverage_target_routes() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");
    let output_dir = temp_dir.path().join("output");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "3",
            "--rate",
            "0.05",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let output_arg = output_dir.to_string_lossy().to_string();

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "render-morphogenesis-sequence",
            carrier_arg.as_str(),
            output_arg.as_str(),
            "--frames",
            "3",
            "--preset",
            "coral",
            "--frame-rate",
            "4",
            "--modulate",
            "inject=lfo(sine,1):0.5,0.1",
            "--modulate",
            "erode=lfo(sine,1):0.3,0.05",
            "--modulate",
            "coverage_target=lfo(sine,1):0.3,0.1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("modulation routes:"))
        .stdout(predicate::str::contains("inject=lfo(sine,1,0):0.5,0.1"))
        .stdout(predicate::str::contains("erode=lfo(sine,1,0):0.3,0.05"))
        .stdout(predicate::str::contains(
            "coverage_target=lfo(sine,1,0):0.3,0.1",
        ));
}

/// Anchor L5's teeth: a routed `inject` target with the render's own static
/// `--inject` left at its 0.0 default (a cold-start route, e.g.
/// `inject=audio-rms` or, here, an LFO for a media-free reproducible test) —
/// the prev-luma tracking decision (`track_prev_luma` in
/// `render_morphogenesis_sequence`) must key off the ROUTE, not just the
/// static knob, or the routed inject would silently never fire (its weight
/// field has no luma to read) and a resumed render would diverge from an
/// uninterrupted one.
#[test]
fn render_morphogenesis_sequence_resumes_byte_identically_with_a_routed_inject_active() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let carrier_dir = temp_dir.path().join("carrier-frames");

    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args([
            "generate-frames",
            "radial",
            carrier_dir.to_string_lossy().as_ref(),
            "--width",
            "24",
            "--height",
            "16",
            "--frames",
            "4",
            "--rate",
            "0.08",
        ])
        .assert()
        .success();

    let carrier_arg = carrier_dir.to_string_lossy().to_string();
    let route = "inject=lfo(sine,1):1,0";
    let base_args = |output_dir: &str| {
        vec![
            "render-morphogenesis-sequence".to_string(),
            carrier_arg.clone(),
            output_dir.to_string(),
            "--frames".to_string(),
            "4".to_string(),
            "--preset".to_string(),
            "coral".to_string(),
            "--pattern-mix".to_string(),
            "0.85".to_string(),
            "--displace".to_string(),
            "0".to_string(),
            "--frame-rate".to_string(),
            "4".to_string(),
        ]
    };
    let routed_args = |output_dir: &str| {
        let mut args = base_args(output_dir);
        args.extend(["--modulate".to_string(), route.to_string()]);
        args
    };

    // The route actually drives inject even though the static --inject flag
    // is never given (stays at its 0.0 default).
    let off_dir = temp_dir.path().join("off-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(base_args(&off_dir.to_string_lossy()))
        .assert()
        .success();
    let uninterrupted_dir = temp_dir.path().join("uninterrupted-output");
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(routed_args(&uninterrupted_dir.to_string_lossy()))
        .assert()
        .success();
    assert_ne!(
        fs::read(uninterrupted_dir.join("frames/frame_000003.png")).expect("routed frame"),
        fs::read(off_dir.join("frames/frame_000003.png")).expect("unrouted frame"),
        "a routed inject target (with the base --inject knob left at 0) must still change the rendered sequence"
    );

    // Interrupt after frame 0, then after frame 1 (the frame a real
    // prev-luma must have survived the checkpoint round-trip for), then
    // resume to completion.
    let resumed_dir = temp_dir.path().join("resumed-output");
    let resumed_args = routed_args(&resumed_dir.to_string_lossy());
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .arg("--stop-after-frame")
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .arg("--stop-after-frame")
        .assert()
        .success();
    Command::cargo_bin("morphogen")
        .expect("morphogen binary")
        .args(&resumed_args)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "rendered morphogenesis sequence with 4 frame(s)",
        ));

    for frame_name in [
        "frame_000000.png",
        "frame_000001.png",
        "frame_000002.png",
        "frame_000003.png",
    ] {
        assert_eq!(
            fs::read(resumed_dir.join("frames").join(frame_name)).expect("resumed frame"),
            fs::read(uninterrupted_dir.join("frames").join(frame_name))
                .expect("uninterrupted frame"),
            "L5: resuming with a ROUTED inject active (base --inject at 0) must be byte-identical to an uninterrupted run ({frame_name}) — the routed-prev-luma-tracking proof"
        );
    }
}
