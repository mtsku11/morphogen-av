use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{CoreError, RenderJob, RenderJobStatus};

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

    /// Cancel a queued or running job by id. Returns an error if the job is
    /// unknown or already in a terminal state (complete, failed, or cancelled).
    pub fn cancel_job(&mut self, job_id: &str) -> Result<(), CoreError> {
        let job = self
            .jobs
            .iter_mut()
            .find(|job| job.id == job_id)
            .ok_or_else(|| CoreError::InvalidRenderQueue(format!("no job with id '{job_id}'")))?;

        if job.status.is_terminal() {
            return Err(CoreError::InvalidRenderQueue(format!(
                "job '{job_id}' is already {:?} and cannot be cancelled",
                job.status
            )));
        }

        job.status = RenderJobStatus::Cancelled;
        Ok(())
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), CoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(self)?;
        // Write-then-rename so a crash mid-write can never leave a truncated
        // queue file behind; the rename is atomic on the same filesystem.
        let temp_path = path.with_extension("json.tmp");
        fs::write(&temp_path, json)?;
        fs::rename(&temp_path, path)?;
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
        AnalysisKind, ExportFormat, ModulationSampling, RenderBackend,
        RenderJobAnalysisCacheProvenance, RenderJobProvenance, RenderJobSourceProvenance,
        RenderJobStatus, RenderJobTask, RenderQuality, RenderSettings, SourceRole,
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
            failure: None,
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
    fn retro_static_job_written_before_modulation_routes_deserializes_unmodulated() {
        let json = r#"
        {
          "type": "frame_sequence_retro_static",
          "source_frame_directory": "/tmp/source-frames",
          "output_directory": "/tmp/out/job-0001",
          "frames": 24,
          "frame_rate": 12.0,
          "real_bpp": 4,
          "assumed_bpp": 3,
          "filter": "paeth",
          "strength": 1.0,
          "backend": "cpu"
        }
        "#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize pre-slice job");

        let RenderJobTask::FrameSequenceRetroStatic {
            modulation_routes,
            modulator_audio_path,
            modulator_frames_directory,
            modulation_sampling,
            ..
        } = task
        else {
            panic!("expected a retro-static task");
        };
        assert!(modulation_routes.is_empty());
        assert!(modulator_audio_path.is_none());
        assert!(modulator_frames_directory.is_none());
        assert_eq!(modulation_sampling, ModulationSampling::Hold);
    }

    #[test]
    fn stateful_jobs_written_before_modulation_routes_deserialize_unmodulated() {
        // Flow feedback and datamosh cover the two stateful task shapes; the
        // fluid-advect variants share the identical serde-defaulted block.
        let feedback_json = r#"
        {
          "type": "frame_sequence_flow_feedback",
          "modulator_frame_directory": "/tmp/mod",
          "carrier_frame_directory": "/tmp/car",
          "output_directory": "/tmp/out/job-0001",
          "flow_cache_directory": null,
          "carrier_amount": 8.0,
          "feedback_amount": 12.0,
          "feedback_mix": 0.7,
          "decay": 0.95,
          "iterations": 1,
          "max_frames": null,
          "frame_rate": 24.0
        }
        "#;
        let task: RenderJobTask =
            serde_json::from_str(feedback_json).expect("deserialize pre-slice feedback job");
        let RenderJobTask::FrameSequenceFlowFeedback {
            modulation_routes,
            modulator_audio_path,
            modulator_frames_directory,
            modulation_sampling,
            ..
        } = task
        else {
            panic!("expected a flow-feedback task");
        };
        assert!(modulation_routes.is_empty());
        assert!(modulator_audio_path.is_none());
        assert!(modulator_frames_directory.is_none());
        assert_eq!(modulation_sampling, ModulationSampling::Hold);

        let datamosh_json = r#"
        {
          "type": "frame_sequence_datamosh",
          "modulator_frame_directory": "/tmp/mod",
          "carrier_frame_directory": "/tmp/car",
          "output_directory": "/tmp/out/job-0002",
          "keyframe_interval": 0,
          "amount": 1.0,
          "max_frames": null
        }
        "#;
        let task: RenderJobTask =
            serde_json::from_str(datamosh_json).expect("deserialize pre-slice datamosh job");
        let RenderJobTask::FrameSequenceDatamosh {
            modulation_routes,
            modulator_audio_path,
            modulator_frames_directory,
            modulation_sampling,
            ..
        } = task
        else {
            panic!("expected a datamosh task");
        };
        assert!(modulation_routes.is_empty());
        assert!(modulator_audio_path.is_none());
        assert!(modulator_frames_directory.is_none());
        assert_eq!(modulation_sampling, ModulationSampling::Hold);
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
                backend: RenderBackend::Cpu,
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
            failure: None,
        };

        let json = serde_json::to_string_pretty(&job).expect("serialize queue job");
        let decoded: RenderJob = serde_json::from_str(&json).expect("deserialize queue job");

        assert_eq!(decoded, job);
    }

    #[test]
    fn cancel_job_marks_queued_job_cancelled() {
        let mut queue = RenderQueue::default();
        queue.enqueue(test_job("job-0001", RenderJobStatus::Queued));

        queue.cancel_job("job-0001").expect("cancel queued job");

        assert_eq!(queue.jobs[0].status, RenderJobStatus::Cancelled);
    }

    #[test]
    fn cancel_job_rejects_unknown_and_terminal_jobs() {
        let mut queue = RenderQueue::default();
        queue.enqueue(test_job("job-0001", RenderJobStatus::Complete));

        assert!(queue.cancel_job("job-9999").is_err());
        assert!(queue.cancel_job("job-0001").is_err());
        assert_eq!(queue.jobs[0].status, RenderJobStatus::Complete);
    }

    fn test_job(id: &str, status: RenderJobStatus) -> RenderJob {
        RenderJob {
            id: id.to_string(),
            project_path: None,
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
            status,
            output: None,
            failure: None,
        }
    }
}
