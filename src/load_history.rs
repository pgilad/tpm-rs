use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

pub(crate) const SCHEMA_VERSION: u8 = 1;
const LOAD_HISTORY_FILE: &str = "load-history.jsonl";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LoadHistoryRecord {
    pub schema: u8,
    pub tpm_version: String,
    pub started_at: u64,
    pub total_ms: u64,
    pub success: bool,
    pub plugins: Vec<LoadHistoryPlugin>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LoadHistoryPlugin {
    pub name: String,
    pub ms: u64,
    pub success: bool,
}

pub(crate) fn history_path(state_dir: &Path) -> PathBuf {
    state_dir.join(LOAD_HISTORY_FILE)
}

pub(crate) fn append(state_dir: &Path, record: &LoadHistoryRecord) {
    if fs::create_dir_all(state_dir).is_err() {
        return;
    }

    let Ok(line) = serde_json::to_string(record) else {
        return;
    };

    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path(state_dir))
    else {
        return;
    };

    let _ = writeln!(file, "{line}");
}

pub(crate) fn read(state_dir: &Path) -> Result<Vec<LoadHistoryRecord>> {
    let path = history_path(state_dir);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(AppError::InspectPath { path, source }),
    };

    let records = contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }

            let record = serde_json::from_str::<LoadHistoryRecord>(line).ok()?;
            if record.schema == SCHEMA_VERSION {
                Some(record)
            } else {
                None
            }
        })
        .collect();

    Ok(records)
}

pub(crate) fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}
