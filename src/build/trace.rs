// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::Result;

use super::{BuildTrace, BuildTraceStage, BuildTraceStageStatus};

pub(super) struct BuildTraceRecorder {
    pub(super) started_at: Instant,
    pub(super) stages: Vec<BuildTraceStage>,
}

impl BuildTraceRecorder {
    pub(super) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            stages: Vec::new(),
        }
    }

    pub(super) fn record<T, F>(
        &mut self,
        name: &str,
        detail: Option<String>,
        operation: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let started_at_ms = self.started_at.elapsed().as_millis();
        let stage_started = Instant::now();
        let result = operation();
        let duration_ms = stage_started.elapsed().as_millis();
        let status = if result.is_ok() {
            BuildTraceStageStatus::Succeeded
        } else {
            BuildTraceStageStatus::Failed
        };
        self.stages.push(BuildTraceStage {
            name: name.to_owned(),
            started_at_ms,
            duration_ms,
            status,
            detail,
        });
        result
    }

    pub(super) fn finish(self, mut trace: BuildTrace) -> BuildTrace {
        trace.total_duration_ms = self.started_at.elapsed().as_millis();
        trace.stages = self.stages;
        trace
    }
}

pub(super) fn generated_at() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_unix_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = seconds_of_day % 3_600 / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

pub(super) fn civil_from_unix_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_piece = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_piece + 2) / 5 + 1;
    let month = month_piece + if month_piece < 10 { 3 } else { -9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}
