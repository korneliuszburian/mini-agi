//! Eval contract tests — ported from `PoC` `tests/test_score.py` plus
//! reproduction of the committed baseline over the 11 real eval cases.

use std::path::PathBuf;

use mini_agi_core::eval::{
    Step, TicketMetadata, ToolMismatch, find_scope_violations, load_ticket_metadata,
    path_is_in_scope, score_run, score_trajectory, step_score, ticket_metadata_for_run, tool_score,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn step(tool: &str, ok: Option<bool>, goal_aligned: Option<bool>) -> Step {
    Step {
        step: 1,
        action: String::new(),
        tool: tool.to_string(),
        ok,
        goal_aligned,
        tokens: 0,
        output_tokens: 0,
        reverted: false,
        note: String::new(),
        paths: Vec::new(),
    }
}

#[test]
fn scores_reproduce_poc_baseline_on_all_cases() {
    let root = repo_root();
    let cases = root.join("evals/cases");
    let golden = root.join("evals/golden");
    let baseline: Vec<serde_json::Value> = serde_json::from_str(
        &std::fs::read_to_string(root.join("evals/results/baseline.json")).unwrap(),
    )
    .unwrap();

    let mut scored = Vec::new();
    for case in &[
        "codex-exp-002",
        "codex-exp-002-rerun",
        "codex-exp-003",
        "codex-exp-003-rerun",
        "flailing",
        "flailing-rerun",
        "harnessed",
        "reactive-loop",
        "reactive-loop-rerun",
        "real-ticket-001-v2",
        "real-ticket-001-v2-rerun",
        "real-ticket-002-v2",
        "real-ticket-002-v2-rerun",
        "real-ticket-003-v2",
        "real-ticket-003-v2-rerun",
        "real-ticket-004-v2",
        "real-ticket-004-v2-rerun",
        "real-ticket-005-v2",
        "real-ticket-005-v2-rerun",
        "real-ticket-006-v2",
        "real-ticket-006-v2-rerun",
        "real-ticket-007-v2",
        "real-ticket-007-v2-rerun",
        "real-ticket-008-v2",
        "afk-max-idle",
    ] {
        let report = score_run(&cases.join(case).join("run.json"), &root, &golden).unwrap();
        scored.push((case.to_string(), report));
    }

    for entry in &baseline {
        let name = entry["case"].as_str().unwrap();
        let (_, report) = scored.iter().find(|(n, _)| n == name).unwrap();
        let want = entry["composite"].as_f64().unwrap();
        let got = report.composite;
        assert!(
            (got - want).abs() < 0.0005,
            "{name}: composite {got} != PoC baseline {want}"
        );
        assert_eq!(
            i64::try_from(report.tokens_total).unwrap_or(-1),
            entry["tokens"].as_i64().unwrap(),
            "{name}: tokens"
        );
        assert!(
            (report.cost_usd - entry["cost_usd"].as_f64().unwrap()).abs() < 0.0005,
            "{name}: cost"
        );
    }
}

#[test]
fn real_ticket_008_has_zero_scope_violations() {
    let root = repo_root();
    let report = score_run(
        &root.join("evals/cases/real-ticket-008-v2/run.json"),
        &root,
        &root.join("evals/golden"),
    )
    .unwrap();
    assert!((report.composite - 0.9774).abs() < 0.0005);
    assert!(report.scope_violations.is_empty());
    assert_eq!(report.tool_mismatches_vs_golden, 0);
}

#[test]
fn reactive_loop_has_zero_composite() {
    let root = repo_root();
    let report = score_run(
        &root.join("evals/cases/reactive-loop/run.json"),
        &root,
        &root.join("evals/golden"),
    )
    .unwrap();
    assert!((report.composite - 0.0).abs() < 1e-12);
    assert!((report.dims.outcome - 0.0).abs() < 1e-12);
}

#[test]
fn tool_mismatch_detail_lists_exact_steps_vs_golden() {
    let root = repo_root();
    let report = score_run(
        &root.join("evals/cases/real-ticket-001-v2/run.json"),
        &root,
        &root.join("evals/golden"),
    )
    .unwrap();
    assert_eq!(report.tool_mismatches_vs_golden, 4);
    assert_eq!(
        report.tool_mismatches_detail,
        vec![
            ToolMismatch {
                step: 2,
                run_tool: "exec".into(),
                golden_tool: "write".into()
            },
            ToolMismatch {
                step: 3,
                run_tool: "exec".into(),
                golden_tool: "write".into()
            },
            ToolMismatch {
                step: 4,
                run_tool: "exec".into(),
                golden_tool: "write".into()
            },
            ToolMismatch {
                step: 6,
                run_tool: "read".into(),
                golden_tool: "write".into()
            },
        ]
    );
}

#[test]
fn directory_scope_entries_match_nested_paths() {
    assert!(path_is_in_scope(
        "artifacts/TICKET-004-v2/spec.md",
        &["artifacts/TICKET-004-v2/".to_string()]
    ));
    assert!(path_is_in_scope(
        "artifacts/TICKET-004-v2/spec.md",
        &["artifacts/TICKET-004-v2".to_string()]
    ));
    assert!(path_is_in_scope(
        "scripts/schemas/handoff.json",
        &["scripts/schemas/*.json".to_string()]
    ));
    assert!(path_is_in_scope(
        "tests/test_score.py",
        &["tests/test_score.py".to_string()]
    ));
    assert!(path_is_in_scope(
        "Makefile",
        &["Makefile (add target)".to_string()]
    ));
    assert!(!path_is_in_scope(
        "outside/unowned.py",
        &["artifacts/TICKET-004-v2".to_string()]
    ));
}

#[test]
fn ticket_metadata_parses_exceptions_and_orchestrator_artifacts() {
    let tmp = std::env::temp_dir().join(format!("mag-eval-meta-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let ticket = tmp.join("TICKET-999.md");
    std::fs::write(
        &ticket,
        "- expected orchestrator post-run artifacts (NOT implementer edits): memory/episodic/checkpoints.log (via checkpoint.sh), evals/cases/real-ticket-999-v2/run.json, git commits\n\
         <!-- machine-readable -->\n\
         scope-exceptions:\n\
         - docs/authorized.md\n",
    )
    .unwrap();
    let meta = load_ticket_metadata(&ticket).unwrap();
    assert_eq!(meta.scope_exceptions, vec!["docs/authorized.md"]);
    assert_eq!(
        meta.orchestrator_artifacts,
        vec![
            "memory/episodic/checkpoints.log",
            "evals/cases/real-ticket-999-v2/run.json",
            "git commits",
        ]
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn ticket_metadata_rejects_malformed_scope_exceptions() {
    let tmp = std::env::temp_dir().join(format!("mag-eval-meta2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let ticket = tmp.join("TICKET-999.md");
    std::fs::write(&ticket, "scope-exceptions\n- docs/authorized.md\n").unwrap();
    assert!(load_ticket_metadata(&ticket).is_err());
    for entry in ["", "**", "docs/*.md", "/etc/passwd", "docs/../secret.md"] {
        std::fs::write(&ticket, format!("scope-exceptions:\n- {entry}\n")).unwrap();
        assert!(load_ticket_metadata(&ticket).is_err(), "entry {entry:?}");
    }
    std::fs::write(&ticket, "scope-exceptions:\nnot-a-list\n").unwrap();
    assert!(load_ticket_metadata(&ticket).is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn exceptions_and_orchestrator_artifacts_are_not_scope_violations() {
    let steps = [
        step("write", Some(true), Some(true)),
        step("write", Some(true), Some(true)),
        step("write", Some(true), Some(true)),
    ];
    let mut steps = steps;
    steps[0].paths = vec!["docs/authorized.md".to_string()];
    steps[1].paths = vec!["memory/episodic/checkpoints.log".to_string()];
    steps[2].paths = vec!["outside/unowned.py".to_string()];
    let meta = TicketMetadata {
        scope_exceptions: vec!["docs/authorized.md".to_string()],
        orchestrator_artifacts: vec!["memory/episodic/checkpoints.log".to_string()],
    };
    assert_eq!(
        find_scope_violations(&steps, &["tests/test_score.py".to_string()], &meta),
        vec!["outside/unowned.py".to_string()]
    );
}

#[test]
fn scope_violation_penalizes_tool_score_but_allows_carve_outs() {
    let steps = [
        step("write", Some(true), Some(true)),
        step("edit", Some(true), Some(true)),
        step("write", Some(true), Some(true)),
        step("edit", Some(true), Some(true)),
    ];
    let mut steps = steps;
    steps[0].paths = vec!["scripts/owned.py".to_string()];
    steps[1].paths = vec!["outside/unowned.py".to_string()];
    steps[2].paths = vec!["memory/derived/context-brief.md".to_string()];
    steps[3].paths = vec!["AGENTS.md".to_string()];
    let meta = TicketMetadata::default();
    let (value, mismatches, _detail) =
        tool_score(&steps, &[], &["scripts/owned.py".to_string()], &meta);
    assert_eq!(mismatches, 0);
    assert!((value - 0.85f64.powi(3)).abs() < 1e-9);
}

#[test]
fn missing_write_paths_fail_closed_as_scope_violation() {
    let steps = [step("write", Some(true), Some(true))];
    let meta = TicketMetadata::default();
    assert_eq!(
        find_scope_violations(&steps, &[], &meta),
        vec!["<unknown write target>".to_string()]
    );
}

#[test]
fn validate_run_rejects_wrong_required_field_types() {
    let text =
        r#"{"trajectory":"not-a-list","outcome":{"achieved":true},"scope":["x"],"metadata":{}}"#;
    let run: Result<mini_agi_core::eval::Run, _> = serde_json::from_str(text);
    assert!(run.is_err());
}

#[test]
fn score_cli_contract_validation_errors_are_clean() {
    let tmp = std::env::temp_dir().join(format!("mag-eval-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("run.json");
    std::fs::write(
        &path,
        r#"{"trajectory":"not-a-list","outcome":{"achieved":true},"scope":[]}"#,
    )
    .unwrap();
    assert!(score_run(&path, &tmp, &tmp.join("golden")).is_err());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn trajectory_scoring_matches_poc_semantics() {
    let good = step("read", Some(true), Some(true));
    let drift = step("exec", Some(true), Some(false));
    let fail = step("write", Some(false), Some(true));
    let report = score_trajectory(&[good, drift, fail]);
    for (got, want) in report.per_step.iter().zip([1.0, 0.2, 0.0]) {
        assert!((got - want).abs() < 1e-12);
    }
    assert!((report.geomean - 0.0).abs() < 1e-12);
    assert_eq!(report.goal_drift_steps, 1);
}

#[test]
fn ticket_metadata_for_run_resolves_from_goal() {
    let root = repo_root();
    let meta = ticket_metadata_for_run(
        "You are implementing TICKET-008-v2 in this repo.",
        &root.join("tickets"),
    );
    assert!(!meta.orchestrator_artifacts.is_empty());
    let none = ticket_metadata_for_run("no ticket here", &root.join("tickets"));
    assert!(none.orchestrator_artifacts.is_empty());
}

#[test]
fn step_scores_port_exactly() {
    let mut s = step("exec", None, None);
    assert!((step_score(&s) - 0.5).abs() < 1e-12);
    s.reverted = true;
    assert!((step_score(&s) - 0.05).abs() < 1e-12);
}
