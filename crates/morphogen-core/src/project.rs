use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    AnalysisCacheEntry, AnalysisKind, CacheManifest, CoreError, ExportFormat, MediaProxy,
    MediaSource, ModulationRoute, Node, NodeGraph, NodeId, NodeKind, RenderQuality, RenderSettings,
    SourceRole, TimeRange, Timeline,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub schema_version: u32,
    pub metadata: ProjectMetadata,
    pub sources: Vec<MediaSource>,
    pub timeline: Timeline,
    pub graph: NodeGraph,
    pub render_settings: RenderSettings,
    pub cache_manifest: CacheManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectMetadata {
    pub name: String,
    pub created_by: String,
    pub notes: String,
}

impl Project {
    pub fn example_two_source_flow_displace() -> Self {
        let source_a = MediaSource {
            id: "source-a".to_string(),
            label: "Source A synthetic modulator".to_string(),
            role: SourceRole::Modulator,
            uri: "file://replace-with-modulator.mov".to_string(),
            proxy: None,
        };
        let source_b = MediaSource {
            id: "source-b".to_string(),
            label: "Source B synthetic carrier".to_string(),
            role: SourceRole::Carrier,
            uri: "file://replace-with-carrier.mov".to_string(),
            proxy: None,
        };

        let graph = NodeGraph {
            nodes: vec![
                Node {
                    id: NodeId::new("source_a"),
                    label: "Source A".to_string(),
                    kind: NodeKind::Source {
                        source_id: source_a.id.clone(),
                        role: SourceRole::Modulator,
                    },
                },
                Node {
                    id: NodeId::new("source_b"),
                    label: "Source B".to_string(),
                    kind: NodeKind::Source {
                        source_id: source_b.id.clone(),
                        role: SourceRole::Carrier,
                    },
                },
                Node {
                    id: NodeId::new("analysis_optical_flow_a"),
                    label: "A optical flow analysis".to_string(),
                    kind: NodeKind::Analysis {
                        source_node: NodeId::new("source_a"),
                        analysis: AnalysisKind::OpticalFlow,
                    },
                },
                Node {
                    id: NodeId::new("render_flow_displace"),
                    label: "Flow-displace B".to_string(),
                    kind: NodeKind::Render {
                        processor: "flow_displace_cpu_reference".to_string(),
                    },
                },
                Node {
                    id: NodeId::new("export_image_sequence"),
                    label: "Export image sequence".to_string(),
                    kind: NodeKind::Export {
                        format: ExportFormat::ImageSequence {
                            extension: "png".to_string(),
                            bit_depth: 16,
                        },
                    },
                },
            ],
            routes: vec![ModulationRoute {
                from_node: NodeId::new("analysis_optical_flow_a"),
                from_output: "flow_xy".to_string(),
                to_node: NodeId::new("render_flow_displace"),
                to_parameter: "displacement_vector_field".to_string(),
                amount: 1.0,
            }],
        };

        Self {
            schema_version: 1,
            metadata: ProjectMetadata {
                name: "Two Source Flow Displace".to_string(),
                created_by: "morphogen-cli".to_string(),
                notes: "Source A optical flow modulates Source B displacement.".to_string(),
            },
            sources: vec![source_a, source_b],
            timeline: Timeline {
                frame_rate: 24.0,
                sample_rate: 48_000,
                range: TimeRange {
                    start_seconds: 0.0,
                    duration_seconds: 4.0,
                },
            },
            graph,
            render_settings: RenderSettings {
                width: 1920,
                height: 1080,
                quality: RenderQuality::HighQualityOffline,
                export_format: ExportFormat::ImageSequence {
                    extension: "png".to_string(),
                    bit_depth: 16,
                },
                temporal_supersampling: 1,
                deterministic: true,
            },
            cache_manifest: CacheManifest {
                version: 1,
                entries: vec![AnalysisCacheEntry {
                    id: "cache-optical-flow-source-a".to_string(),
                    source_id: "source-a".to_string(),
                    kind: AnalysisKind::OpticalFlow,
                    path: "cache/source-a/optical-flow".to_string(),
                    frame_count: None,
                    sample_count: None,
                }],
            },
        }
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.schema_version == 0 {
            return Err(CoreError::InvalidProject(
                "schema_version must be greater than zero".to_string(),
            ));
        }

        self.timeline.validate()?;

        if self.sources.is_empty() {
            return Err(CoreError::InvalidProject(
                "project must declare at least one source".to_string(),
            ));
        }

        let mut source_ids = HashSet::new();
        for source in &self.sources {
            if !source_ids.insert(source.id.as_str()) {
                return Err(CoreError::InvalidProject(format!(
                    "duplicate media source id '{}'",
                    source.id
                )));
            }
        }

        let mut node_ids = HashSet::new();
        for node in &self.graph.nodes {
            if !node_ids.insert(node.id.clone()) {
                return Err(CoreError::InvalidProject(format!(
                    "duplicate node id '{}'",
                    node.id
                )));
            }
        }

        let nodes_by_id: HashMap<NodeId, &Node> = self
            .graph
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node))
            .collect();

        for node in &self.graph.nodes {
            match &node.kind {
                NodeKind::Source { source_id, .. } => {
                    if !source_ids.contains(source_id.as_str()) {
                        return Err(CoreError::InvalidProject(format!(
                            "node '{}' references missing source '{}'",
                            node.id, source_id
                        )));
                    }
                }
                NodeKind::Analysis { source_node, .. } => match nodes_by_id.get(source_node) {
                    Some(source) if matches!(&source.kind, NodeKind::Source { .. }) => {}
                    Some(_) => {
                        return Err(CoreError::InvalidProject(format!(
                            "analysis node '{}' source_node '{}' is not a source node",
                            node.id, source_node
                        )));
                    }
                    None => {
                        return Err(CoreError::InvalidProject(format!(
                            "analysis node '{}' references missing source node '{}'",
                            node.id, source_node
                        )));
                    }
                },
                NodeKind::Render { .. } | NodeKind::Export { .. } => {}
            }
        }

        for route in &self.graph.routes {
            validate_route(route, &nodes_by_id)?;
        }

        Ok(())
    }

    /// Record ingested proxy media for a source and merge in any analysis-cache
    /// references it produced. Cache entries with an id that already exists are
    /// replaced so repeated ingests stay idempotent. Errors if the source is unknown.
    pub fn register_source_proxy(
        &mut self,
        source_id: &str,
        proxy: MediaProxy,
        caches: Vec<AnalysisCacheEntry>,
    ) -> Result<(), CoreError> {
        if proxy.frame_directory.trim().is_empty() {
            return Err(CoreError::InvalidProject(
                "proxy frame_directory must not be empty".to_string(),
            ));
        }

        let source = self
            .sources
            .iter_mut()
            .find(|source| source.id == source_id)
            .ok_or_else(|| {
                CoreError::InvalidProject(format!(
                    "cannot register proxy: no source with id '{source_id}'"
                ))
            })?;
        source.proxy = Some(proxy);

        for entry in caches {
            match self
                .cache_manifest
                .entries
                .iter_mut()
                .find(|existing| existing.id == entry.id)
            {
                Some(existing) => *existing = entry,
                None => self.cache_manifest.entries.push(entry),
            }
        }

        Ok(())
    }

    pub fn summary(&self) -> String {
        format!(
            "{}: {} sources, {} nodes, {} routes, {}x{} {:?}",
            self.metadata.name,
            self.sources.len(),
            self.graph.nodes.len(),
            self.graph.routes.len(),
            self.render_settings.width,
            self.render_settings.height,
            self.render_settings.quality
        )
    }
}

fn validate_route(
    route: &ModulationRoute,
    nodes_by_id: &HashMap<NodeId, &Node>,
) -> Result<(), CoreError> {
    if route.from_output.trim().is_empty() || route.to_parameter.trim().is_empty() {
        return Err(CoreError::InvalidProject(
            "route endpoints must be named".to_string(),
        ));
    }

    if !route.amount.is_finite() {
        return Err(CoreError::InvalidProject(format!(
            "route '{}.{} -> {}.{}' amount must be finite",
            route.from_node, route.from_output, route.to_node, route.to_parameter
        )));
    }

    let from_node = nodes_by_id.get(&route.from_node).ok_or_else(|| {
        CoreError::InvalidProject(format!(
            "route references missing from_node '{}'",
            route.from_node
        ))
    })?;
    let to_node = nodes_by_id.get(&route.to_node).ok_or_else(|| {
        CoreError::InvalidProject(format!(
            "route references missing to_node '{}'",
            route.to_node
        ))
    })?;

    let from_signal = from_node
        .kind
        .output_signal(&route.from_output)
        .ok_or_else(|| {
            CoreError::InvalidProject(format!(
                "node '{}' has no output port '{}'",
                route.from_node, route.from_output
            ))
        })?;
    let to_signal = to_node
        .kind
        .parameter_signal(&route.to_parameter)
        .ok_or_else(|| {
            CoreError::InvalidProject(format!(
                "node '{}' has no parameter port '{}'",
                route.to_node, route.to_parameter
            ))
        })?;

    if from_signal != to_signal {
        return Err(CoreError::InvalidProject(format!(
            "route '{}.{} -> {}.{}' connects incompatible signal types {} -> {}",
            route.from_node,
            route.from_output,
            route.to_node,
            route.to_parameter,
            from_signal,
            to_signal
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_source_proxy_records_proxy_and_merges_caches() {
        let mut project = Project::example_two_source_flow_displace();
        let cache_count_before = project.cache_manifest.entries.len();

        project
            .register_source_proxy(
                "source-a",
                MediaProxy {
                    frame_directory: "proxy/source-a/frames".to_string(),
                    audio_path: Some("proxy/source-a/audio.wav".to_string()),
                },
                vec![AnalysisCacheEntry {
                    id: "cache-audio-rms-source-a".to_string(),
                    source_id: "source-a".to_string(),
                    kind: AnalysisKind::AudioRms,
                    path: "proxy/source-a/rms.json".to_string(),
                    frame_count: None,
                    sample_count: None,
                }],
            )
            .expect("register proxy");

        let source = project
            .sources
            .iter()
            .find(|source| source.id == "source-a")
            .expect("source-a present");
        assert_eq!(
            source
                .proxy
                .as_ref()
                .map(|proxy| proxy.frame_directory.as_str()),
            Some("proxy/source-a/frames")
        );
        assert_eq!(project.cache_manifest.entries.len(), cache_count_before + 1);
        assert!(project.validate().is_ok());
    }

    #[test]
    fn register_source_proxy_replaces_cache_entry_with_same_id() {
        let mut project = Project::example_two_source_flow_displace();
        let proxy = MediaProxy {
            frame_directory: "proxy/source-a/frames".to_string(),
            audio_path: None,
        };
        let entry = AnalysisCacheEntry {
            id: "cache-stft-source-a".to_string(),
            source_id: "source-a".to_string(),
            kind: AnalysisKind::Stft,
            path: "first.json".to_string(),
            frame_count: None,
            sample_count: None,
        };
        project
            .register_source_proxy("source-a", proxy.clone(), vec![entry.clone()])
            .expect("first register");

        let updated = AnalysisCacheEntry {
            path: "second.json".to_string(),
            ..entry
        };
        project
            .register_source_proxy("source-a", proxy, vec![updated])
            .expect("second register");

        let matching: Vec<_> = project
            .cache_manifest
            .entries
            .iter()
            .filter(|entry| entry.id == "cache-stft-source-a")
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].path, "second.json");
    }

    #[test]
    fn register_source_proxy_rejects_unknown_source() {
        let mut project = Project::example_two_source_flow_displace();
        let result = project.register_source_proxy(
            "source-zzz",
            MediaProxy {
                frame_directory: "proxy/frames".to_string(),
                audio_path: None,
            },
            Vec::new(),
        );
        assert!(result.is_err());
    }
}
