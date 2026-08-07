//! The intelligence layer (ADR-0005): runs compound into the world model.
//!
//! Sequoia thesis ("From Hierarchy to Intelligence", Dorsey & Botha,
//! 2026-03-31): an intelligence-organized company's world model is built
//! from recorded actions, and the honest signal is measured, not
//! reported. For an agent kernel the honest signal is the scored run:
//! tokens, cost, composite score, violations. This module closes the loop:
//!
//! - [`ingest_run`] turns one scored `run.json` (plus optional retro) into
//!   canonical facts with provenance — the model deepens per run without
//!   human writing;
//! - [`insights`] aggregates runs, memory, tickets and the journal into a
//!   compounding report — the failure signal (failing case, REWORK, budget
//!   overrun) IS the roadmap.

use std::fs;
use std::io;
use std::path::Path;

use crate::eval::{self, ScoreReport};
use crate::memory;

/// Result of ingesting one run into the world model.
#[derive(Debug)]
pub struct IngestReport {
    /// Case name (parent directory of the run file).
    pub case: String,
    /// Composite score of the ingested run.
    pub composite: f64,
    /// Total tokens of the run.
    pub tokens: u64,
    /// Total cost in USD.
    pub cost_usd: f64,
    /// Canonical facts written by this ingest.
    pub new_facts: usize,
    /// Facts already known (dedup by content hash).
    pub skipped: usize,
}

/// Ingest a scored run (plus optional retro) into canonical memory.
///
/// Facts carry `domain: eval` and provenance (`source: run ingest`). The
/// run is scored against `evals/golden`; re-ingesting the same run adds
/// nothing (content-hash dedup) — idempotent by construction.
///
/// # Errors
///
/// Returns the underlying I/O, scoring, or memory error.
pub fn ingest_run(root: &Path, run: &Path, retro: Option<&Path>) -> Result<IngestReport, String> {
    // P0-1 (hardening audit): cost cap enforced at ingest — a run that
    // exceeds the repo's configured max_cost_usd is refused, so a
    // self-reported cost cannot slip into the trusted corpus silently.
    if let Some(max) = crate::config::Config::load(root).max_cost_usd
        && let Some(r) = std::fs::read_to_string(run)
            .ok()
            .and_then(|text| serde_json::from_str::<crate::eval::Run>(&text).ok())
        && r.cost_usd > max
    {
        return Err(format!(
            "refusing to ingest {}: cost ${:.4} exceeds the configured \
             max_cost_usd ${max:.4} (P0-1 cost cap)",
            run.display(),
            r.cost_usd
        ));
    }
    let report = eval::score_run(run, root, &root.join("evals/golden"))
        .map_err(|e| format!("cannot score run: {e}"))?;
    let case = run
        .parent()
        .and_then(|p| p.file_name())
        .map_or_else(|| "run".to_string(), |n| n.to_string_lossy().into_owned());
    let mut lines = Vec::new();
    lines.push(format!(
        "FACT: run {case} scored composite {:.4} on {} tokens ({:.4} USD) with {} scope violations and {} tool mismatches.",
        report.composite,
        report.tokens_total,
        report.cost_usd,
        report.scope_violations.len(),
        report.tool_mismatches_vs_golden
    ));
    if let Some(flag) = run_flag(&report) {
        lines.push(format!("FACT: run {case} {flag}."));
    }
    if let Some(retro_path) = retro
        && let Ok(text) = fs::read_to_string(retro_path)
    {
        for bullet in text
            .lines()
            .filter(|l| l.trim_start().starts_with("- "))
            .take(3)
        {
            let line = bullet.trim().trim_start_matches("- ").trim();
            if !line.is_empty() {
                lines.push(format!("FACT: retro ({case}): {line}"));
            }
        }
    }
    let buffer = lines.join("\n");
    let opts = memory::ConsolidateOptions {
        domain: "eval".to_string(),
        require_signoff: false,
        dry_run: false,
    };
    let outcome = memory::consolidate(root, &buffer, "run ingest", &opts)
        .map_err(|e| format!("cannot consolidate run facts: {e}"))?;
    // Comprehensive action log (production-readiness D.1).
    let _ = crate::audit::append_action(root, "run-ingest", "kernel", &case);
    Ok(IngestReport {
        case,
        composite: report.composite,
        tokens: report.tokens_total,
        cost_usd: report.cost_usd,
        new_facts: outcome.new_facts,
        skipped: outcome.skipped,
    })
}

/// One-line run flag for the world model (measured signals, ADR-0005).
fn run_flag(report: &ScoreReport) -> Option<String> {
    if report.composite >= 0.9 {
        Some("is a strong run (composite >= 0.9)".to_string())
    } else if report.composite <= 0.0 {
        Some("is a failed run (composite 0.0)".to_string())
    } else {
        None
    }
}

/// One case's aggregated run data.
#[derive(Debug)]
pub struct CaseInsight {
    /// Case name.
    pub case: String,
    /// Latest composite score.
    pub composite: f64,
}

/// The compounding report.
#[derive(Debug)]
pub struct InsightsReport {
    /// Runs found under `evals/cases/`.
    pub runs: usize,
    /// Mean composite across runs (history, plain).
    pub composite_avg: f64,
    /// Capability mean (ADR-0010): original cases only, a passing rerun
    /// overrides its original; each case counted once.
    pub composite_avg_effective: f64,
    /// Total tokens across runs.
    pub tokens_total: u64,
    /// Total cost across runs.
    pub cost_total: f64,
    /// Per-case latest composite, sorted by case.
    pub cases: Vec<CaseInsight>,
    /// Canonical entries count.
    pub entries: usize,
    /// Canonical facts count.
    pub facts: usize,
    /// Tickets discovered.
    pub tickets: usize,
    /// Journal events by kind (begin/pass/fail/status).
    pub journal: [usize; 4],
    /// Capability gaps: cases scoring below the gate tolerance are the
    /// roadmap (Sequoia failure-signal loop).
    pub gaps: Vec<String>,
}

/// Aggregate runs, memory, tickets and the journal into the report.
///
/// # Errors
///
/// Returns the underlying filesystem error.
#[allow(
    clippy::cast_precision_loss,
    reason = "exact port of PoC float math; run counts are bounded by case list size"
)]
pub fn insights(root: &Path) -> Result<InsightsReport, io::Error> {
    let cases_dir = root.join("evals/cases");
    let golden_dir = root.join("evals/golden");
    let mut cases: Vec<CaseInsight> = Vec::new();
    let mut tokens_total = 0u64;
    let mut cost_total = 0.0f64;
    let mut gaps = Vec::new();
    if cases_dir.is_dir() {
        for entry in fs::read_dir(&cases_dir)? {
            let entry = entry?;
            let run = entry.path().join("run.json");
            if !run.is_file() {
                continue;
            }
            let case = entry.file_name().to_string_lossy().into_owned();
            if let Ok(report) = eval::score_run(&run, root, &golden_dir) {
                tokens_total += report.tokens_total;
                cost_total += report.cost_usd;
                if report.composite <= 0.05 && !crate::loopcmd::case_closed_by_rerun(root, &case) {
                    gaps.push(format!("{case} (composite {:.4})", report.composite));
                }
                cases.push(CaseInsight {
                    case,
                    composite: report.composite,
                });
            }
        }
    }
    cases.sort_by(|a, b| a.case.cmp(&b.case));
    let runs = cases.len();
    let composite_avg = if runs == 0 {
        0.0
    } else {
        cases.iter().map(|c| c.composite).sum::<f64>() / runs as f64
    };
    // Effective capability mean (ADR-0010): original cases only; a
    // passing rerun (loop target 0.5) overrides its original.
    let mut effective_sum = 0.0f64;
    let mut effective_count = 0usize;
    for case in &cases {
        if case.case.ends_with("-rerun") {
            continue;
        }
        let effective = crate::loopcmd::rerun_composite(root, &case.case)
            .filter(|c| *c >= crate::loopcmd::TARGET_COMPOSITE)
            .unwrap_or(case.composite);
        effective_sum += effective;
        effective_count += 1;
    }
    let composite_avg_effective = if effective_count == 0 {
        0.0
    } else {
        effective_sum / effective_count as f64
    };

    let stats = crate::metrics::stats(root).unwrap_or_default();
    let tickets = crate::ticket::list_tickets(root).unwrap_or_default().len();
    let mut journal = [0usize; 4];
    let journal_path = root.join("memory/episodic/checkpoints.log");
    if let Ok(text) = fs::read_to_string(&journal_path) {
        for event in crate::journal::parse_journal(&text) {
            match event.kind {
                crate::journal::JournalKind::Begin => journal[0] += 1,
                crate::journal::JournalKind::VerifyPass => journal[1] += 1,
                crate::journal::JournalKind::VerifyFail => journal[2] += 1,
                crate::journal::JournalKind::Status => journal[3] += 1,
                crate::journal::JournalKind::End => {}
            }
        }
    }

    Ok(InsightsReport {
        runs,
        composite_avg,
        composite_avg_effective,
        tokens_total,
        cost_total,
        cases,
        entries: stats.entries,
        facts: stats.facts,
        tickets,
        journal,
        gaps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("mag-insights-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn seed_case(root: &std::path::Path, case: &str) {
        let dir = root.join("evals/cases").join(case);
        fs::create_dir_all(&dir).unwrap();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/cases/real-ticket-008-v2/run.json");
        fs::copy(&src, dir.join("run.json")).unwrap();
        let golden = root.join("evals/golden");
        fs::create_dir_all(&golden).unwrap();
        let gsrc = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/golden/real-ticket-compact.json");
        fs::copy(&gsrc, golden.join("real-ticket-compact.json")).unwrap();
    }

    fn first_entry_text(root: &std::path::Path) -> String {
        let entries = root.join("memory/canonical/entries");
        let mut days: Vec<std::path::PathBuf> = fs::read_dir(&entries)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        days.sort();
        days.into_iter()
            .find_map(|day| {
                let mut files: Vec<std::path::PathBuf> = fs::read_dir(&day)
                    .unwrap()
                    .flatten()
                    .map(|e| e.path())
                    .collect();
                files.sort();
                files.first().and_then(|f| fs::read_to_string(f).ok())
            })
            .unwrap_or_default()
    }

    #[test]
    fn ingest_run_writes_world_model_facts_and_is_idempotent() {
        let root = tmp_root("ingest");
        seed_case(&root, "real-ticket-008-v2");
        let run = root.join("evals/cases/real-ticket-008-v2/run.json");
        let first = ingest_run(&root, &run, None).unwrap();
        assert_eq!(first.case, "real-ticket-008-v2");
        assert!((first.composite - 0.9774).abs() < 0.001);
        assert!(first.new_facts >= 1);
        let second = ingest_run(&root, &run, None).unwrap();
        assert_eq!(second.new_facts, 0);
        assert!(second.skipped >= 1);
        let text = first_entry_text(&root);
        assert!(text.contains("composite"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ingest_refuses_run_over_cost_cap() {
        // P0-1 (hardening audit): the cost cap is enforced at ingest.
        let root = tmp_root("cost-cap");
        seed_case(&root, "real-ticket-008-v2");
        fs::write(root.join(".miniagi.json"), r#"{"max_cost_usd": 0.001}"#).unwrap();
        let run = root.join("evals/cases/real-ticket-008-v2/run.json");
        let err =
            ingest_run(&root, &run, None).expect_err("ingest must refuse a run over the cost cap");
        assert!(err.contains("cost"), "unexpected error: {err}");
        assert!(err.contains("max_cost_usd"), "unexpected error: {err}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ingest_retro_bullets_become_facts() {
        let root = tmp_root("ingest-retro");
        seed_case(&root, "real-ticket-008-v2");
        fs::write(
            root.join("retro.md"),
            "# Retro\n\n- batching amortized the fixed overhead\n- checkpoint discipline held\n",
        )
        .unwrap();
        let run = root.join("evals/cases/real-ticket-008-v2/run.json");
        let report = ingest_run(&root, &run, Some(&root.join("retro.md"))).unwrap();
        assert!(report.new_facts >= 2);
        let text = first_entry_text(&root);
        assert!(text.contains("batching amortized"));
        assert!(text.contains("checkpoint discipline held"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn insights_effective_avg_uses_passing_rerun_override() {
        let root = tmp_root("insights-eff");
        // A custom failing run (composite 0) for the original case...
        let dir = root.join("evals/cases/weak");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("run.json"),
            r#"{"goal":"weak","scope":["x"],"outcome":{"achieved":false},"tokens_total":100,"cost_usd":0.01,"golden":null,"trajectory":[{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        // ...and a passing rerun seeded from the 0.9774 fixture.
        seed_case(&root, "weak-rerun");
        let report = insights(&root).unwrap();
        assert_eq!(report.runs, 2);
        assert!(
            (report.composite_avg - 0.4887).abs() < 0.001,
            "plain {:.4}",
            report.composite_avg
        );
        assert!(
            (report.composite_avg_effective - 0.9774).abs() < 0.001,
            "effective {:.4}",
            report.composite_avg_effective
        );
        assert!(report.composite_avg_effective > report.composite_avg);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn insights_aggregate_runs_and_memory() {
        let root = tmp_root("insights");
        seed_case(&root, "real-ticket-008-v2");
        let report = insights(&root).unwrap();
        assert_eq!(report.runs, 1);
        assert!((report.composite_avg - 0.9774).abs() < 0.001);
        assert!((report.composite_avg_effective - 0.9774).abs() < 0.001);
        assert!(report.tokens_total > 100_000);
        assert!(report.cost_total > 0.0);
        assert_eq!(report.cases.len(), 1);
        assert!(report.gaps.is_empty());
        assert_eq!(report.entries, 0);
        assert_eq!(report.facts, 0);
        assert_eq!(report.tickets, 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn insights_flags_failing_runs_as_capability_gaps() {
        let root = tmp_root("gaps");
        let dir = root.join("evals/cases/reactive-loop");
        fs::create_dir_all(&dir).unwrap();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/cases/reactive-loop/run.json");
        fs::copy(&src, dir.join("run.json")).unwrap();
        fs::create_dir_all(root.join("evals/golden")).unwrap();
        let report = insights(&root).unwrap();
        assert_eq!(report.runs, 1);
        assert_eq!(report.gaps.len(), 1);
        assert!(report.gaps[0].contains("reactive-loop"));
        let _ = fs::remove_dir_all(&root);
    }
}

/// A backlog item generated from a capability gap (ADR-0005).
#[derive(Debug)]
pub struct BacklogTicket {
    /// Generated ticket id.
    pub id: String,
    /// Case behind the gap.
    pub case: String,
    /// Whether the ticket already existed (dedup) or was written.
    pub created: bool,
}

/// Turn capability gaps into tickets — the Sequoia failure-signal loop:
/// a failing run IS a roadmap item. Idempotent: a gap already referenced
/// by an existing ticket is skipped.
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn backlog(root: &Path) -> Result<Vec<BacklogTicket>, io::Error> {
    let report = insights(root)?;
    let existing = crate::ticket::list_tickets(root).unwrap_or_default();
    let mut next_number = existing
        .iter()
        .filter_map(|t| {
            t.id.strip_prefix("TICKET-")
                .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|d| d.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1;
    let mut created = Vec::new();
    for gap in &report.gaps {
        let case = gap.split(" (composite").next().unwrap_or(gap).to_string();
        let existing_match = existing
            .iter()
            .find(|t| t.goal.contains(&case) || t.title.contains(&case));
        if let Some(matched) = existing_match {
            created.push(BacklogTicket {
                id: matched.id.clone(),
                case: case.clone(),
                created: false,
            });
            continue;
        }
        let id = format!("TICKET-{next_number}");
        next_number += 1;
        let body = format!(
            "# Ticket\n\n- id: {id}\n- title: Fix capability gap: {case} scores below gate\n- goal (one sentence): Bring {case} composite above the gate tolerance by fixing the failing run.\n- scope: evals/cases\n- domain: eval\n"
        );
        fs::write(root.join("tickets").join(format!("{id}.md")), body)?;
        created.push(BacklogTicket {
            id,
            case,
            created: true,
        });
    }
    Ok(created)
}

/// The resume block: what a fresh session needs to pick up state
/// (brief summary, journal tail, in-flight checkpoint).
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn resume(root: &Path) -> Result<String, io::Error> {
    let stats = crate::metrics::stats(root).unwrap_or_default();
    let mut out = format!(
        "resume: {} canonical facts across {} entries\n",
        stats.facts, stats.entries
    );
    let journal = root.join("memory/episodic/checkpoints.log");
    if let Ok(text) = fs::read_to_string(&journal) {
        let lines: Vec<&str> = text.lines().collect();
        let tail = lines.iter().rev().take(5).rev();
        out.push_str("journal tail:\n");
        for line in tail {
            use std::fmt::Write as _;
            let _ = writeln!(out, "  {line}");
        }
        if let Some(last) = lines.last()
            && last.contains("BEGIN")
        {
            out.push_str("in-flight checkpoint: yes (last line is a BEGIN)\n");
        }
    }
    let brief = root.join("memory/derived/context-brief.md");
    if let Ok(text) = fs::read_to_string(&brief) {
        let head: Vec<&str> = text.lines().take(12).collect();
        out.push_str("brief head:\n");
        for line in head {
            use std::fmt::Write as _;
            let _ = writeln!(out, "  {line}");
        }
    }
    if let Ok(failures) = crate::failure::read_register(root)
        && !failures.is_empty()
    {
        use std::fmt::Write as _;
        let _ = writeln!(
            out,
            "failure register: {} recorded failures — do not repeat ({}):",
            failures.len(),
            crate::failure::register_path(root).display()
        );
        for entry in failures.iter().rev().take(5).rev() {
            let _ = writeln!(
                out,
                "  `{}` tool={} action=\"{}\" count={} case={}",
                entry.hash, entry.tool, entry.action, entry.count, entry.case
            );
        }
    }
    if let Ok(mismatches) = crate::mismatch::read_register(root)
        && !mismatches.is_empty()
    {
        use std::fmt::Write as _;
        let cases: std::collections::BTreeSet<&str> =
            mismatches.iter().map(|e| e.case.as_str()).collect();
        let _ = writeln!(
            out,
            "tool mismatch register: {} divergences in {} cases — golden expects ({}) — match the golden step shape:",
            mismatches.len(),
            cases.len(),
            crate::mismatch::register_path(root).display()
        );
        for entry in mismatches.iter().rev().take(5).rev() {
            let _ = writeln!(
                out,
                "  {} step {}: golden expects {}, used {}",
                entry.case, entry.step, entry.golden_tool, entry.run_tool
            );
        }
    }
    Ok(out)
}

#[cfg(test)]
mod backlog_tests {
    use super::*;
    use std::fs;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("mag-backlog-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tickets")).unwrap();
        root
    }

    #[test]
    fn backlog_writes_gap_ticket_and_dedups() {
        let root = tmp_root("a");
        let dir = root.join("evals/cases/reactive-loop");
        fs::create_dir_all(&dir).unwrap();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/cases/reactive-loop/run.json");
        fs::copy(&src, dir.join("run.json")).unwrap();
        let first = backlog(&root).unwrap();
        assert_eq!(first.len(), 1);
        assert!(first[0].created);
        assert_eq!(first[0].id, "TICKET-1");
        assert!(root.join("tickets/TICKET-1.md").is_file());
        let second = backlog(&root).unwrap();
        assert_eq!(second.len(), 1);
        assert!(!second[0].created);
        assert_eq!(
            second[0].id, "TICKET-1",
            "dedup must surface the existing id"
        );
        let text = fs::read_to_string(root.join("tickets/TICKET-1.md")).unwrap();
        assert!(text.contains("reactive-loop"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn backlog_numbers_continue_after_existing_tickets() {
        let root = tmp_root("b");
        fs::write(
            root.join("tickets/TICKET-7.md"),
            "- id: TICKET-7\n- title: old\n- goal: old ticket\n- scope: evals/cases\n",
        )
        .unwrap();
        let dir = root.join("evals/cases/reactive-loop");
        fs::create_dir_all(&dir).unwrap();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/cases/reactive-loop/run.json");
        fs::copy(&src, dir.join("run.json")).unwrap();
        let first = backlog(&root).unwrap();
        assert_eq!(first[0].id, "TICKET-8");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resume_block_includes_brief_and_journal() {
        let root = tmp_root("c");
        fs::create_dir_all(root.join("memory/episodic")).unwrap();
        fs::write(
            root.join("memory/episodic/checkpoints.log"),
            "2026-08-02T19:00:00Z BEGIN step -> abc\n2026-08-02T19:01:00Z VERIFY-PASS step @ abc\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("memory/derived")).unwrap();
        fs::write(
            root.join("memory/derived/context-brief.md"),
            "# CONTEXT BRIEF\n\n- fact one\n",
        )
        .unwrap();
        let block = resume(&root).unwrap();
        assert!(block.contains("resume:"));
        assert!(block.contains("VERIFY-PASS step"));
        assert!(block.contains("fact one"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resume_block_includes_failure_register() {
        let root = tmp_root("resume-register");
        fs::create_dir_all(root.join("memory/derived")).unwrap();
        let entry = crate::failure::FailureEntry {
            hash: crate::hash::fact_id("edit|edit same line"),
            tool: "edit".into(),
            action: "edit same line".into(),
            count: 2,
            steps: vec![4, 6],
            case: "reactive-loop".into(),
            reflection: None,
            mast: None,
            verifier: None,
        };
        crate::failure::update_register(&root, std::slice::from_ref(&entry)).unwrap();
        let block = resume(&root).unwrap();
        assert!(block.contains("failure register: 1 recorded failures"));
        assert!(block.contains("edit same line"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resume_block_includes_mismatch_register() {
        // resume's tool-mismatch block had no test (journal/brief/
        // failure were covered); a regression where the block stops
        // being rendered would pass silently.
        let root = tmp_root("resume-mismatch");
        fs::create_dir_all(root.join("memory/derived")).unwrap();
        let entry = crate::mismatch::MismatchEntry {
            hash: crate::hash::fact_id("case|1|edit|read"),
            case: "reactive-loop".into(),
            step: 1,
            run_tool: "edit".into(),
            golden_tool: "read".into(),
        };
        crate::mismatch::update_register(&root, std::slice::from_ref(&entry)).unwrap();
        let block = resume(&root).unwrap();
        assert!(
            block.contains("tool mismatch register: 1 divergences"),
            "{block}"
        );
        assert!(block.contains("golden expects read"), "{block}");
        assert!(block.contains("used edit"), "{block}");
        let _ = fs::remove_dir_all(&root);
    }
}
