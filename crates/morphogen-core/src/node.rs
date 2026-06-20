use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{AnalysisKind, ExportFormat, SourceRole};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalType {
    MediaSource,
    RgbaImage,
    ScalarField2D,
    VectorField2D,
    ScalarControl,
    AudioSpectrum,
    GrainIndex,
}

impl fmt::Display for SignalType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            SignalType::MediaSource => "media_source",
            SignalType::RgbaImage => "rgba_image",
            SignalType::ScalarField2D => "scalar_field_2d",
            SignalType::VectorField2D => "vector_field_2d",
            SignalType::ScalarControl => "scalar_control",
            SignalType::AudioSpectrum => "audio_spectrum",
            SignalType::GrainIndex => "grain_index",
        };
        formatter.write_str(name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub label: String,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeKind {
    Source {
        source_id: String,
        role: SourceRole,
    },
    Analysis {
        source_node: NodeId,
        analysis: AnalysisKind,
    },
    Render {
        processor: String,
    },
    Export {
        format: ExportFormat,
    },
}

impl NodeKind {
    pub fn output_signal(&self, output: &str) -> Option<SignalType> {
        match self {
            NodeKind::Source { .. } => match output {
                "media" => Some(SignalType::MediaSource),
                _ => None,
            },
            NodeKind::Analysis { analysis, .. } => analysis_output_signal(*analysis, output),
            NodeKind::Render { processor } => render_output_signal(processor, output),
            NodeKind::Export { .. } => None,
        }
    }

    pub fn parameter_signal(&self, parameter: &str) -> Option<SignalType> {
        match self {
            NodeKind::Render { processor } => render_parameter_signal(processor, parameter),
            NodeKind::Export { .. } => match parameter {
                "image" => Some(SignalType::RgbaImage),
                _ => None,
            },
            NodeKind::Source { .. } | NodeKind::Analysis { .. } => None,
        }
    }
}

fn analysis_output_signal(analysis: AnalysisKind, output: &str) -> Option<SignalType> {
    match (analysis, output) {
        (AnalysisKind::Luminance, "luminance") => Some(SignalType::ScalarField2D),
        (AnalysisKind::EdgeMap, "edge_map") => Some(SignalType::ScalarField2D),
        (AnalysisKind::OpticalFlow, "flow_xy") => Some(SignalType::VectorField2D),
        (AnalysisKind::DepthMap, "depth") => Some(SignalType::ScalarField2D),
        (AnalysisKind::AudioRms, "rms") => Some(SignalType::ScalarControl),
        (AnalysisKind::SpectralCentroid, "spectral_centroid") => Some(SignalType::ScalarControl),
        (AnalysisKind::OnsetStrength, "onset_strength") => Some(SignalType::ScalarControl),
        (AnalysisKind::Stft, "stft") => Some(SignalType::AudioSpectrum),
        (AnalysisKind::GrainDescriptors, "grain_descriptors") => Some(SignalType::GrainIndex),
        _ => None,
    }
}

fn render_output_signal(processor: &str, output: &str) -> Option<SignalType> {
    match (processor, output) {
        ("flow_displace_cpu_reference", "image") => Some(SignalType::RgbaImage),
        ("flow_displace", "image") => Some(SignalType::RgbaImage),
        ("flow_displace_metal", "image") => Some(SignalType::RgbaImage),
        ("flow_feedback", "image") => Some(SignalType::RgbaImage),
        _ => None,
    }
}

fn render_parameter_signal(processor: &str, parameter: &str) -> Option<SignalType> {
    match (processor, parameter) {
        ("flow_displace_cpu_reference", "displacement_vector_field") => {
            Some(SignalType::VectorField2D)
        }
        ("flow_displace", "displacement_vector_field") => Some(SignalType::VectorField2D),
        ("flow_displace_metal", "displacement_vector_field") => Some(SignalType::VectorField2D),
        ("flow_displace_cpu_reference", "displacement_amount") => Some(SignalType::ScalarControl),
        ("flow_displace", "displacement_amount") => Some(SignalType::ScalarControl),
        ("flow_displace_metal", "displacement_amount") => Some(SignalType::ScalarControl),
        ("flow_feedback", "displacement_vector_field") => Some(SignalType::VectorField2D),
        ("flow_feedback", "carrier_amount") => Some(SignalType::ScalarControl),
        ("flow_feedback", "feedback_amount") => Some(SignalType::ScalarControl),
        ("flow_feedback", "feedback_mix") => Some(SignalType::ScalarControl),
        ("flow_feedback", "decay") => Some(SignalType::ScalarControl),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_feedback_declares_its_typed_ports() {
        let node = NodeKind::Render {
            processor: "flow_feedback".to_string(),
        };

        assert_eq!(node.output_signal("image"), Some(SignalType::RgbaImage));
        assert_eq!(
            node.parameter_signal("displacement_vector_field"),
            Some(SignalType::VectorField2D)
        );
        assert_eq!(
            node.parameter_signal("feedback_mix"),
            Some(SignalType::ScalarControl)
        );
    }
}
