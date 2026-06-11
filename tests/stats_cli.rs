use std::time::{SystemTime, UNIX_EPOCH};

mod support;

use support::{run_tpm_with_env, unique_temp_dir, write_file};

#[test]
fn stats_summarizes_recent_load_history() {
    let workspace = unique_temp_dir("stats-human");
    let state_dir = workspace.join("state");
    let now = unix_timestamp();
    let recent_one = now.saturating_sub(10 * 24 * 60 * 60);
    let recent_two = now.saturating_sub(60);
    let old = now.saturating_sub(31 * 24 * 60 * 60);

    write_file(
        &state_dir.join("load-history.jsonl"),
        &format!(
            concat!(
                "not json\n",
                "{{\"schema\":2,\"tpm_version\":\"ignored\",\"started_at\":{recent_two},\"total_ms\":999,\"success\":true,\"plugins\":[]}}\n",
                "{{\"schema\":1,\"tpm_version\":\"1.0.0\",\"started_at\":{recent_one},\"total_ms\":100,\"success\":true,\"plugins\":[{{\"name\":\"slow\",\"ms\":80,\"success\":true}},{{\"name\":\"fast\",\"ms\":20,\"success\":true}}]}}\n",
                "{{\"schema\":1,\"tpm_version\":\"1.1.0\",\"started_at\":{recent_two},\"total_ms\":300,\"success\":false,\"plugins\":[{{\"name\":\"slow\",\"ms\":200,\"success\":false}},{{\"name\":\"new\",\"ms\":50,\"success\":true}}]}}\n",
                "{{\"schema\":1,\"tpm_version\":\"1.0.0\",\"started_at\":{old},\"total_ms\":1000,\"success\":true,\"plugins\":[{{\"name\":\"slow\",\"ms\":900,\"success\":true}}]}}\n",
            ),
            recent_one = recent_one,
            recent_two = recent_two,
            old = old,
        ),
    );

    let output = run_tpm_with_env(
        &workspace,
        ["stats", "--days", "30"],
        vec![("TPM_STATE_DIR".to_string(), state_dir.display().to_string())],
    );

    assert!(output.status.success(), "stats should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf-8");
    assert!(stdout.contains("Load stats for last 30 days"));
    assert!(stdout.contains("Runs: 2"));
    assert!(stdout.contains("Successful: 1"));
    assert!(stdout.contains("Failed: 1"));
    assert!(stdout.contains("TPM versions: 1.0.0, 1.1.0"));
    assert!(stdout.contains("avg: 200ms  max: 300ms"));
    assert!(stdout.contains("slow  avg 140ms"));
    assert!(stdout.contains("max 200ms"));
    assert!(stdout.contains("runs 2"));
    assert!(stdout.contains("failed 1"));
    assert!(stdout.contains("new"));
    assert!(stdout.contains("fast"));
    assert!(!stdout.contains("1000ms"));
    assert!(!stdout.contains("ignored"));
}

#[test]
fn stats_json_emits_machine_readable_summary() {
    let workspace = unique_temp_dir("stats-json");
    let state_dir = workspace.join("state");
    let now = unix_timestamp();

    write_file(
        &state_dir.join("load-history.jsonl"),
        &format!(
            "{{\"schema\":1,\"tpm_version\":\"1.0.0\",\"started_at\":{now},\"total_ms\":101,\"success\":true,\"plugins\":[{{\"name\":\"plugin-a\",\"ms\":41,\"success\":true}}]}}\n"
        ),
    );

    let output = run_tpm_with_env(
        &workspace,
        ["stats", "--json"],
        vec![("TPM_STATE_DIR".to_string(), state_dir.display().to_string())],
    );

    assert!(output.status.success(), "stats should succeed: {output:?}");

    let report = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .expect("stats json should parse");
    assert_eq!(report["days"].as_u64(), Some(30));
    assert_eq!(report["runs"].as_u64(), Some(1));
    assert_eq!(report["successful_runs"].as_u64(), Some(1));
    assert_eq!(report["failed_runs"].as_u64(), Some(0));
    assert_eq!(report["tpm_versions"][0].as_str(), Some("1.0.0"));
    assert_eq!(report["total_ms"]["avg"].as_u64(), Some(101));
    assert_eq!(report["total_ms"]["max"].as_u64(), Some(101));
    assert_eq!(report["plugins"][0]["name"].as_str(), Some("plugin-a"));
    assert_eq!(report["plugins"][0]["avg_ms"].as_u64(), Some(41));
    assert_eq!(report["plugins"][0]["max_ms"].as_u64(), Some(41));
}

#[test]
fn stats_reports_empty_history_without_error() {
    let workspace = unique_temp_dir("stats-empty");
    let state_dir = workspace.join("state");

    let output = run_tpm_with_env(
        &workspace,
        ["stats"],
        vec![("TPM_STATE_DIR".to_string(), state_dir.display().to_string())],
    );

    assert!(output.status.success(), "stats should succeed: {output:?}");
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should be utf-8"),
        ""
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be utf-8"),
        "No load stats recorded for last 30 days. Run `tpm load` to collect stats.\n"
    );
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be monotonic")
        .as_secs()
}
