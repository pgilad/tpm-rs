use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::{
    commands::base_paths,
    error::Result,
    load_history::{self, LoadHistoryRecord},
};

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

#[derive(Debug, Serialize)]
struct StatsReport {
    days: u64,
    runs: usize,
    successful_runs: usize,
    failed_runs: usize,
    tpm_versions: Vec<String>,
    total_ms: DurationStats,
    plugins: Vec<PluginStats>,
}

#[derive(Debug, Default, Serialize)]
struct DurationStats {
    avg: u64,
    max: u64,
}

#[derive(Debug, Serialize)]
struct PluginStats {
    name: String,
    runs: usize,
    failed_runs: usize,
    avg_ms: u64,
    max_ms: u64,
}

#[derive(Debug, Default)]
struct Accumulator {
    runs: usize,
    failed_runs: usize,
    sum_ms: u128,
    max_ms: u64,
}

pub fn run(
    config_override: Option<&Path>,
    plugins_override: Option<&Path>,
    days: u64,
    json: bool,
) -> Result<()> {
    let paths = base_paths(config_override, plugins_override)?;
    let records = load_history::read(&paths.state_dir)?;
    let report = build_report(&records, days, unix_timestamp());

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }

    Ok(())
}

fn build_report(records: &[LoadHistoryRecord], days: u64, now: u64) -> StatsReport {
    let cutoff = now.saturating_sub(days.saturating_mul(SECONDS_PER_DAY));
    let mut total = Accumulator::default();
    let mut successful_runs = 0;
    let mut versions = BTreeSet::new();
    let mut plugin_accumulators = HashMap::<String, Accumulator>::new();

    for record in records.iter().filter(|record| {
        record.started_at >= cutoff && record.schema == load_history::SCHEMA_VERSION
    }) {
        total.record(record.total_ms, record.success);
        if record.success {
            successful_runs += 1;
        }
        versions.insert(record.tpm_version.clone());

        for plugin in &record.plugins {
            plugin_accumulators
                .entry(plugin.name.clone())
                .or_default()
                .record(plugin.ms, plugin.success);
        }
    }

    let mut plugins = plugin_accumulators
        .into_iter()
        .map(|(name, accumulator)| PluginStats {
            name,
            runs: accumulator.runs,
            failed_runs: accumulator.failed_runs,
            avg_ms: accumulator.avg_ms(),
            max_ms: accumulator.max_ms,
        })
        .collect::<Vec<_>>();
    plugins.sort_by(|left, right| {
        right
            .max_ms
            .cmp(&left.max_ms)
            .then_with(|| right.avg_ms.cmp(&left.avg_ms))
            .then_with(|| left.name.cmp(&right.name))
    });

    StatsReport {
        days,
        runs: total.runs,
        successful_runs,
        failed_runs: total.failed_runs,
        tpm_versions: versions.into_iter().collect(),
        total_ms: DurationStats {
            avg: total.avg_ms(),
            max: total.max_ms,
        },
        plugins,
    }
}

fn print_human(report: &StatsReport) {
    if report.runs == 0 {
        println!(
            "No load stats recorded for last {}. Run `tpm load` to collect stats.",
            days_label(report.days)
        );
        return;
    }

    println!("Load stats for last {}", days_label(report.days));
    println!();
    println!("Runs: {}", report.runs);
    println!("Successful: {}", report.successful_runs);
    println!("Failed: {}", report.failed_runs);
    if !report.tpm_versions.is_empty() {
        println!("TPM versions: {}", report.tpm_versions.join(", "));
    }
    println!();
    println!("Total load time:");
    println!(
        "  avg: {}  max: {}",
        format_millis(report.total_ms.avg),
        format_millis(report.total_ms.max)
    );
    println!();
    println!("Plugins:");

    if report.plugins.is_empty() {
        println!("  none");
        return;
    }

    let name_width = report
        .plugins
        .iter()
        .map(|plugin| plugin.name.len())
        .max()
        .unwrap_or(0);
    for plugin in &report.plugins {
        println!(
            "  {name:<name_width$}  avg {avg:<8}  max {max:<8}  runs {runs:<3}  failed {failed}",
            name = plugin.name,
            avg = format_millis(plugin.avg_ms),
            max = format_millis(plugin.max_ms),
            runs = plugin.runs,
            failed = plugin.failed_runs,
        );
    }
}

impl Accumulator {
    fn record(&mut self, ms: u64, success: bool) {
        self.runs += 1;
        if !success {
            self.failed_runs += 1;
        }
        self.sum_ms += u128::from(ms);
        self.max_ms = self.max_ms.max(ms);
    }

    fn avg_ms(&self) -> u64 {
        if self.runs == 0 {
            return 0;
        }

        let runs = self.runs as u128;
        ((self.sum_ms + (runs / 2)) / runs)
            .try_into()
            .unwrap_or(u64::MAX)
    }
}

fn days_label(days: u64) -> String {
    match days {
        1 => "1 day".to_string(),
        days => format!("{days} days"),
    }
}

fn format_millis(ms: u64) -> String {
    format!("{ms}ms")
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::build_report;
    use crate::load_history::{self, LoadHistoryPlugin, LoadHistoryRecord};

    #[test]
    fn aggregates_recent_records_only() {
        let now = 4_000_000;
        let records = vec![
            LoadHistoryRecord {
                schema: load_history::SCHEMA_VERSION,
                tpm_version: "1.0.0".to_string(),
                started_at: now - 10,
                total_ms: 100,
                success: true,
                plugins: vec![LoadHistoryPlugin {
                    name: "slow".to_string(),
                    ms: 80,
                    success: true,
                }],
            },
            LoadHistoryRecord {
                schema: load_history::SCHEMA_VERSION,
                tpm_version: "1.1.0".to_string(),
                started_at: now - 20,
                total_ms: 300,
                success: false,
                plugins: vec![LoadHistoryPlugin {
                    name: "slow".to_string(),
                    ms: 200,
                    success: false,
                }],
            },
            LoadHistoryRecord {
                schema: load_history::SCHEMA_VERSION,
                tpm_version: "1.0.0".to_string(),
                started_at: now - (31 * 24 * 60 * 60),
                total_ms: 900,
                success: true,
                plugins: Vec::new(),
            },
        ];

        let report = build_report(&records, 30, now);

        assert_eq!(report.runs, 2);
        assert_eq!(report.successful_runs, 1);
        assert_eq!(report.failed_runs, 1);
        assert_eq!(report.tpm_versions, vec!["1.0.0", "1.1.0"]);
        assert_eq!(report.total_ms.avg, 200);
        assert_eq!(report.total_ms.max, 300);
        assert_eq!(report.plugins.len(), 1);
        assert_eq!(report.plugins[0].name, "slow");
        assert_eq!(report.plugins[0].avg_ms, 140);
        assert_eq!(report.plugins[0].max_ms, 200);
        assert_eq!(report.plugins[0].runs, 2);
        assert_eq!(report.plugins[0].failed_runs, 1);
    }
}
