#![forbid(unsafe_code)]

pub mod analysis;
pub mod cache;
pub mod error;
pub mod export;
pub mod graph;
pub mod media;
pub mod node;
pub mod project;
pub mod render_job;
pub mod render_queue;
pub mod timeline;

pub use analysis::AnalysisKind;
pub use cache::{AnalysisCacheEntry, CacheManifest};
pub use error::CoreError;
pub use graph::{ModulationRoute, NodeGraph};
pub use media::{MediaProxy, MediaSource, SourceRole};
pub use node::{Node, NodeId, NodeKind, SignalType};
pub use project::{Project, ProjectMetadata};
pub use render_job::{
    ConvolutionMethod, CrossSynthFilterType, CrossSynthMode, CrossSynthWindow, ExportFormat,
    FlowSource,
    GrainSelectionMode, GranularAudioModulation, KernelMode, RenderBackend, RenderJob,
    RenderJobAnalysisCacheProvenance, RenderJobFailure, RenderJobOutputMetadata,
    RenderJobProvenance, RenderJobSourceProvenance, RenderJobStatus, RenderJobTask, RenderQuality,
    RenderSettings, RenderTimingMetadata, VideoVocoderMode,
};
pub use render_queue::RenderQueue;
pub use timeline::{FrameSampleRange, TimeRange, Timeline};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_project_serializes_and_deserializes() {
        let project = Project::example_two_source_flow_displace();
        let json = serde_json::to_string_pretty(&project).expect("serialize example project");
        let decoded: Project = serde_json::from_str(&json).expect("deserialize example project");

        assert_eq!(decoded.metadata.name, "Two Source Flow Displace");
        decoded.validate().expect("example project validates");
    }

    #[test]
    fn checked_in_example_project_is_valid() {
        let json =
            include_str!("../../../examples/projects/two_source_flow_displace.morphogen.json");
        let project: Project = serde_json::from_str(json).expect("parse checked-in example");

        project.validate().expect("checked-in example validates");
        assert_eq!(project.sources.len(), 2);
    }

    #[test]
    fn modulation_route_serializes_as_stable_json() {
        let route = ModulationRoute {
            from_node: NodeId::new("analysis_optical_flow_a"),
            from_output: "flow_xy".to_string(),
            to_node: NodeId::new("render_flow_displace"),
            to_parameter: "displacement_vector_field".to_string(),
            amount: 1.0,
        };

        let json = serde_json::to_string(&route).expect("serialize route");
        assert!(json.contains("analysis_optical_flow_a"));
        assert!(json.contains("displacement_vector_field"));

        let decoded: ModulationRoute = serde_json::from_str(&json).expect("deserialize route");
        assert_eq!(decoded, route);
    }

    #[test]
    fn validation_rejects_unknown_route_output_port() {
        let mut project = Project::example_two_source_flow_displace();
        project.graph.routes[0].from_output = "not_a_flow_output".to_string();

        let error = project
            .validate()
            .expect_err("unknown route output rejected");

        assert!(error.to_string().contains("has no output port"));
    }

    #[test]
    fn validation_rejects_incompatible_route_signal_types() {
        let mut project = Project::example_two_source_flow_displace();
        project.graph.routes[0].to_parameter = "displacement_amount".to_string();

        let error = project
            .validate()
            .expect_err("incompatible route signal rejected");

        assert!(error.to_string().contains("incompatible signal types"));
        assert!(error
            .to_string()
            .contains("vector_field_2d -> scalar_control"));
    }

    #[test]
    fn validation_requires_analysis_nodes_to_target_source_nodes() {
        let mut project = Project::example_two_source_flow_displace();
        let analysis_node = project
            .graph
            .nodes
            .iter_mut()
            .find(|node| node.id == NodeId::new("analysis_optical_flow_a"))
            .expect("analysis node exists");
        analysis_node.kind = NodeKind::Analysis {
            source_node: NodeId::new("render_flow_displace"),
            analysis: AnalysisKind::OpticalFlow,
        };

        let error = project
            .validate()
            .expect_err("non-source analysis input rejected");

        assert!(error.to_string().contains("is not a source node"));
    }
}
