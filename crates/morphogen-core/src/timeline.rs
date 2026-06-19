use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Timeline {
    pub frame_rate: f64,
    pub sample_rate: u32,
    pub range: TimeRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSampleRange {
    pub frame_index: u64,
    pub start_sample: u64,
    pub end_sample: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeRange {
    pub start_seconds: f64,
    pub duration_seconds: f64,
}

impl Timeline {
    pub fn validate(&self) -> Result<(), CoreError> {
        if !self.frame_rate.is_finite() || self.frame_rate <= 0.0 {
            return Err(CoreError::InvalidTimeline(
                "frame_rate must be a positive finite number".to_string(),
            ));
        }
        if self.sample_rate == 0 {
            return Err(CoreError::InvalidTimeline(
                "sample_rate must be greater than zero".to_string(),
            ));
        }
        if !self.range.start_seconds.is_finite() || self.range.start_seconds < 0.0 {
            return Err(CoreError::InvalidTimeline(
                "range.start_seconds must be a non-negative finite number".to_string(),
            ));
        }
        if !self.range.duration_seconds.is_finite() || self.range.duration_seconds < 0.0 {
            return Err(CoreError::InvalidTimeline(
                "range.duration_seconds must be a non-negative finite number".to_string(),
            ));
        }
        Ok(())
    }

    pub fn frame_count(&self) -> Result<u64, CoreError> {
        self.validate()?;
        checked_seconds_to_count(
            self.range.duration_seconds,
            self.frame_rate,
            CountMode::Ceil,
        )
    }

    pub fn sample_count(&self) -> Result<u64, CoreError> {
        self.validate()?;
        checked_seconds_to_count(
            self.range.duration_seconds,
            self.sample_rate as f64,
            CountMode::Round,
        )
    }

    pub fn time_for_frame(&self, frame_index: u64) -> Result<f64, CoreError> {
        self.validate()?;
        Ok(self.range.start_seconds + frame_index as f64 / self.frame_rate)
    }

    pub fn sample_range_for_frame(&self, frame_index: u64) -> Result<FrameSampleRange, CoreError> {
        self.validate()?;
        let start_time = self.time_for_frame(frame_index)?;
        let end_time = self.time_for_frame(frame_index.saturating_add(1))?;

        Ok(FrameSampleRange {
            frame_index,
            start_sample: checked_seconds_to_count(
                start_time,
                self.sample_rate as f64,
                CountMode::Round,
            )?,
            end_sample: checked_seconds_to_count(
                end_time,
                self.sample_rate as f64,
                CountMode::Round,
            )?,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum CountMode {
    Ceil,
    Round,
}

fn checked_seconds_to_count(
    seconds: f64,
    units_per_second: f64,
    mode: CountMode,
) -> Result<u64, CoreError> {
    let value = seconds * units_per_second;
    if !value.is_finite() || value < 0.0 || value > u64::MAX as f64 {
        return Err(CoreError::InvalidTimeline(
            "timeline conversion produced an out-of-range count".to_string(),
        ));
    }

    let count = match mode {
        CountMode::Ceil => value.ceil(),
        CountMode::Round => value.round(),
    };
    Ok(count as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_timeline() -> Timeline {
        Timeline {
            frame_rate: 24.0,
            sample_rate: 48_000,
            range: TimeRange {
                start_seconds: 0.0,
                duration_seconds: 4.0,
            },
        }
    }

    #[test]
    fn frame_and_sample_counts_align_for_integer_rate_timeline() {
        let timeline = test_timeline();

        assert_eq!(timeline.frame_count().expect("frame count"), 96);
        assert_eq!(timeline.sample_count().expect("sample count"), 192_000);
    }

    #[test]
    fn sample_range_for_frame_uses_frame_edge_times() {
        let timeline = test_timeline();
        let range = timeline.sample_range_for_frame(10).expect("sample range");

        assert_eq!(
            range,
            FrameSampleRange {
                frame_index: 10,
                start_sample: 20_000,
                end_sample: 22_000,
            }
        );
    }

    #[test]
    fn fractional_frame_rate_sample_ranges_remain_contiguous() {
        let timeline = Timeline {
            frame_rate: 29.97,
            sample_rate: 48_000,
            range: TimeRange {
                start_seconds: 0.0,
                duration_seconds: 1.0,
            },
        };

        let first = timeline.sample_range_for_frame(0).expect("first frame");
        let second = timeline.sample_range_for_frame(1).expect("second frame");

        assert_eq!(first.end_sample, second.start_sample);
        assert!(first.end_sample > first.start_sample);
        assert_eq!(timeline.frame_count().expect("frame count"), 30);
    }

    #[test]
    fn validation_rejects_invalid_timing_values() {
        let timeline = Timeline {
            frame_rate: 0.0,
            sample_rate: 48_000,
            range: TimeRange {
                start_seconds: 0.0,
                duration_seconds: 1.0,
            },
        };

        assert!(timeline.validate().is_err());
    }
}
