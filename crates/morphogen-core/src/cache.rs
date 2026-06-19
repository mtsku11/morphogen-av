use serde::{Deserialize, Serialize};

use crate::AnalysisKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheManifest {
    pub version: u32,
    pub entries: Vec<AnalysisCacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisCacheEntry {
    pub id: String,
    pub source_id: String,
    pub kind: AnalysisKind,
    pub path: String,
    pub frame_count: Option<u64>,
    pub sample_count: Option<u64>,
}
