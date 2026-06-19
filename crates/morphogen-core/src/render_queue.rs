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
    use crate::{ExportFormat, RenderJobStatus, RenderQuality, RenderSettings};

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
            status: RenderJobStatus::Queued,
        });

        queue.save_json(&path).expect("save queue");
        let decoded = RenderQueue::load_json(&path).expect("load queue");

        assert_eq!(decoded, queue);
    }
}
