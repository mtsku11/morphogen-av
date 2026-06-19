use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{CoreError, RenderJob};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RenderQueue {
    pub jobs: Vec<RenderJob>,
}

impl RenderQueue {
    pub fn enqueue(&mut self, job: RenderJob) {
        self.jobs.push(job);
    }

    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let json = fs::read_to_string(path)?;
        let queue = serde_json::from_str(&json)?;
        Ok(queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AnalysisKind, ExportFormat, RenderJobAnalysisCacheProvenance, RenderJobProvenance,
        RenderJobSourceProvenance, RenderJobStatus, RenderJobTask, RenderQuality, RenderSettings,
        SourceRole,
    };

    #[test]
    fn render_queue_persists_to_json() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("queue.json");
        let mut queue = RenderQueue::default();
        queue.enqueue(RenderJob {
            id: "job-0001".to_string(),
            project_path: Some(
                "examples/projects/two_source_flow_displace.morphogen.json".to_string(),
            ),
            settings: RenderSettings {
                width: 1920,
                height: 1080,
                quality: RenderQuality::HighQualityOffline,
                export_format: ExportFormat::Png { bit_depth: 16 },
                temporal_supersampling: 1,
                deterministic: true,
            },
            task: Default::default(),
            provenance: None,
            status: RenderJobStatus::Queued,
            output: None,
        });

        queue.save_json(&path).expect("save queue");
        let decoded = RenderQueue::load_json(&path).expect("load queue");

        assert_eq!(decoded, queue);
    }

    #[test]
    fn render_queue_loads_jobs_written_before_output_metadata_existed() {
        let json = r#"
        {
          "jobs": [
            {
              "id": "job-0001",
              "project_path": null,
              "settings": {
                "width": 1920,
                "height": 1080,
                "quality": "high_quality_offline",
                "export_format": {
                  "type": "image_sequence",
                  "extension": "png",
                  "bit_depth": 16
                },
                "temporal_supersampling": 1,
                "deterministic": true
              },
              "status": "queued"
            }
          ]
        }
        "#;

        let queue: RenderQueue = serde_json::from_str(json).expect("deserialize old queue");

        assert_eq!(queue.jobs.len(), 1);
        assert!(queue.jobs[0].output.is_none());
        assert_eq!(queue.jobs[0].task, RenderJobTask::TestRender);
        assert!(queue.jobs[0].provenance.is_none());
    }

    #[test]
    fn frame_sequence_job_persists_source_and_cache_provenance() {
        let job = RenderJob {
            id: "job-0002".to_string(),
            project_path: None,
            settings: RenderSettings {
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
            task: RenderJobTask::FrameSequenceFlowDisplace {
                modulator_frame_directory: "/tmp/modulator-frames".to_string(),
                carrier_frame_directory: "/tmp/carrier-frames".to_string(),
                output_directory: "/tmp/output/job-0002".to_string(),
                flow_cache_directory: Some("/tmp/output/job-0002/cache/flow".to_string()),
                amount: 12.0,
                max_frames: Some(48),
                frame_rate: 24.0,
            },
            provenance: Some(RenderJobProvenance {
                sources: vec![
                    RenderJobSourceProvenance {
                        source_id: "source-a-frames".to_string(),
                        role: SourceRole::Modulator,
                        path: "/tmp/modulator-frames".to_string(),
                    },
                    RenderJobSourceProvenance {
                        source_id: "source-b-frames".to_string(),
                        role: SourceRole::Carrier,
                        path: "/tmp/carrier-frames".to_string(),
                    },
                ],
                analysis_caches: vec![RenderJobAnalysisCacheProvenance {
                    kind: AnalysisKind::OpticalFlow,
                    path: "/tmp/output/job-0002/cache/flow".to_string(),
                    producer: "luminance_gradient_cpu_v1".to_string(),
                }],
            }),
            status: RenderJobStatus::Queued,
            output: None,
        };

        let json = serde_json::to_string_pretty(&job).expect("serialize queue job");
        let decoded: RenderJob = serde_json::from_str(&json).expect("deserialize queue job");

        assert_eq!(decoded, job);
    }
}
