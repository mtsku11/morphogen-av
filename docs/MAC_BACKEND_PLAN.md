# Mac Backend Plan

Morphogen AV is Mac-first and optimized for Apple Silicon. The initial repository keeps native platform integration explicit without binding the project to a premature bridge.

## SwiftUI and AppKit

SwiftUI is the app shell and primary UI layer. AppKit escape hatches are expected for advanced file panels, timeline controls, custom node graph interactions, and media preview surfaces.

## Rust Core Bridge Options

The app starts independent from Rust. Future bridge options are:

- C ABI/staticlib for a narrow stable engine boundary.
- UniFFI for generated Swift bindings when the schema and API settle.
- Swift invoking the local CLI during early development for low-friction validation.
- Later direct engine binding for long-running render jobs and interactive preview.

## Metal Rendering

Metal is the only initial GPU target. The Rust and Swift layers should treat Metal as the production backend for image processing, feedback, flow displacement, pyramids, and eventual optical flow.

## AVFoundation

AVFoundation is the intended user-facing media layer for asset inspection, permissions, metadata, track selection, and decode coordination. FFmpeg remains an optional external helper for early CLI workflows.

The SwiftUI shell now uses AVFoundation for source probing, reading asset duration plus basic video and audio track metadata. FFprobe remains a fallback through the dev CLI bridge for files AVFoundation cannot inspect.

## CoreMedia

CoreMedia should own accurate timing concepts in the Mac backend: time ranges, frame durations, sample timing, and synchronization between audio and video.

## CoreVideo

CoreVideo pixel buffers are the likely bridge between decoded media, preview surfaces, and Metal textures. The SwiftUI target now has a compile-checked `CoreVideoMetalTextureBridge` helper that creates a `CVMetalTextureCache`, validates pixel-buffer planes, and returns an `MTLTexture` for a selected pixel format. `AVFoundationFrameTextureBridge` decodes the first video frame of an asset as a BGRA `CVPixelBuffer` and can pass it through the texture bridge as an `.bgra8Unorm` `MTLTexture`. This is not connected to a preview surface yet.

## VideoToolbox

VideoToolbox is the long-term path for hardware decode/encode and ProRes-oriented export workflows. The SwiftUI target now has a compile-checked ProRes planning helper that registers professional workflow encoders, lists available ProRes encoders, validates configurable 1920x1080 ProRes offline export plans, and probes `VTCopySupportedPropertyDictionaryForEncoder`.

The first real exporter feeds deterministic PNG image-sequence frames into an `AVAssetWriterInputPixelBufferAdaptor` and writes a configurable ProRes `.mov` using VideoToolbox encoder specification keys. It can also mux the first WAV stem from a render queue bundle. The early source-pixel-buffer format is `kCVPixelFormatType_32BGRA` for compatibility; later high-quality paths should evaluate direct `VTCompressionSession` control, 10-bit 4:2:2, and half-float RGBA handoff from the Metal/offline render pipeline.

## Accelerate/vDSP

Accelerate and vDSP should back high-performance audio analysis once portable Rust implementations define expected behavior. Target areas include FFT, STFT, convolution, spectral centroid, onset detection, and future phase-vocoder or cross-synthesis stages.
