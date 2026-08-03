//! Eval engine — four-dimensional scoring, regression gate.
//!
//! Port of `PoC` `evals/harness/{score,trajectory,gate}.py` (behavioral
//! contract, tag `v1-spec-reference`). Deterministic 4D path is
//! authoritative; LLM judge is optional/calibrated (judge.py semantics,
//! phase 3+).
//!
//! Dimensions:
//! - D1 outcome: 0..1 — goal achieved + deterministic gate results.
//! - D2 trajectory: 0..1 — geomean of per-step scores (`score_trajectory`).
//! - D3 tool-use: 0..1 — golden parity + scope violations (0.85 penalty each).
//! - D4 cost: cost-normalized success = outcome / cost.
//!
//! Composite = D1 * D2 * D3 (any zero dimension kills the run).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

/// Penalty applied per tool mismatch or scope violation (`PoC` constant).
pub const TOOL_PARITY_PENALTY: f64 = 0.85;
/// Max allowed composite drop in the regression gate (`PoC` default).
pub const DEFAULT_TOLERANCE: f64 = 0.05;
/// Max allowed cost growth for equal-or-better outcome (`PoC`).
pub const MAX_COST_GROWTH: f64 = 1.25;
/// Cost floor when a run reports zero cost (`PoC`: `or 0.0001`).
const COST_FLOOR: f64 = 0.0001;

/// Eval engine errors (exit-code mapped by the CLI: 2 on validation).
#[derive(Debug, Error)]
pub enum EvalError {
    /// The run file could not be read.
    #[error("cannot read run file: {0}")]
    Read(#[from] std::io::Error),
    /// The run JSON failed to parse.
    #[error("invalid run json: {0}")]
    Json(#[from] serde_json::Error),
    /// A run field failed contract validation.
    #[error("invalid run field '{0}'")]
    InvalidField(String),
    /// Ticket metadata was malformed.
    #[error("{0}")]
    Metadata(String),
}

/// A single step of an agent run trajectory.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Step {
    /// 1-based step index (`PoC` contract).
    #[serde(default)]
    pub step: u32,
    /// Human/JSON action payload.
    #[serde(default)]
    pub action: String,
    /// Tool used for this step.
    pub tool: String,
    /// Whether the step's deterministic gates passed. `null` = not gated.
    #[serde(default)]
    pub ok: Option<bool>,
    /// Whether the step stayed on goal. `null` = not scored.
    #[serde(default)]
    pub goal_aligned: Option<bool>,
    /// Tokens spent on this step.
    #[serde(default)]
    pub tokens: u64,
    /// Output tokens produced by this step.
    #[serde(default)]
    pub output_tokens: u64,
    /// Whether an edit-commit cascade reverted this step.
    #[serde(default)]
    pub reverted: bool,
    /// Free-form note.
    #[serde(default)]
    pub note: String,
    /// File paths touched by write/edit steps.
    #[serde(default)]
    pub paths: Vec<String>,
}

/// Outcome block of a run.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Outcome {
    /// Whether the run achieved its goal.
    #[serde(default)]
    pub achieved: bool,
    /// Deterministic gate results; `null` = gate not run.
    #[serde(default)]
    pub tests_pass: Option<bool>,
    /// Typecheck gate result.
    #[serde(default)]
    pub typecheck_pass: Option<bool>,
    /// Lint gate result.
    #[serde(default)]
    pub lint_pass: Option<bool>,
}

/// A scored run: the input contract of the eval engine.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Run {
    /// The goal given to the agent (may reference a ticket).
    pub goal: String,
    /// Declared write scope (files/dirs, `fnmatch` entries).
    pub scope: Vec<String>,
    /// Outcome block.
    pub outcome: Outcome,
    /// Total tokens spent on the run.
    #[serde(default)]
    pub tokens_total: u64,
    /// Run cost in USD.
    #[serde(default)]
    pub cost_usd: f64,
    /// Golden trajectory file name (relative to evals/golden/), if any.
    #[serde(default)]
    pub golden: Option<String>,
    /// Steps of the run.
    pub trajectory: Vec<Step>,
    /// Extra metadata (ignored by scoring).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Run {
    /// Validate the scorer inputs; returns the run on success.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::InvalidField`] when a required field has the
    /// wrong type. (Mirrors `PoC` `validate_run`.)
    pub fn validate(run: &Self) -> Result<&Self, EvalError> {
        if run.trajectory.is_empty() {
            return Err(EvalError::InvalidField("trajectory".into()));
        }
        Ok(run)
    }
}

/// Per-step trajectory score in `[0, 1]` (`PoC` `step_score`).
#[must_use]
pub fn step_score(step: &Step) -> f64 {
    let mut s = 1.0;
    if step.ok == Some(false) {
        s *= 0.0;
    }
    if step.ok == Some(true) {
        s *= 1.0;
    }
    if step.goal_aligned == Some(false) {
        s *= 0.2;
    }
    if step.reverted {
        s *= 0.1;
    }
    if step.ok.is_none() {
        s *= 0.5;
    }
    s
}

/// Geometric mean; zero if any score is non-positive (`PoC` `geomean`).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "exact port of PoC float math; lens are bounded by run sizes"
)]
pub fn geomean(scores: &[f64]) -> f64 {
    if scores.is_empty() || scores.iter().any(|s| *s <= 0.0) {
        return 0.0;
    }
    let sum: f64 = scores.iter().map(|s| s.ln()).sum();
    (sum / scores.len() as f64).exp()
}

/// Full trajectory report (`PoC` `score_trajectory`).
#[derive(Debug, Serialize)]
pub struct TrajectoryReport {
    /// Number of steps.
    pub steps: usize,
    /// Per-step scores.
    pub per_step: Vec<f64>,
    /// Geomean of step scores, rounded to 4 decimals.
    pub geomean: f64,
    /// Sum of step tokens.
    pub total_tokens: u64,
    /// Sum of step output tokens.
    pub output_tokens: u64,
    /// Steps flagged as goal drift.
    pub goal_drift_steps: usize,
    /// Steps reverted by the checkpoint cascade.
    pub reverted_steps: usize,
    /// Tokens per step (`PoC`: total / max(steps, 1), rounded).
    pub efficiency_tokens_per_step: u64,
}

/// Score a trajectory (`PoC` `score_trajectory`).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "exact port of PoC float math; efficiency = round(total/steps)"
)]
pub fn score_trajectory(steps: &[Step]) -> TrajectoryReport {
    let per_step: Vec<f64> = steps.iter().map(step_score).collect();
    let total_tokens: u64 = steps.iter().map(|s| s.tokens).sum();
    let output_tokens: u64 = steps.iter().map(|s| s.output_tokens).sum();
    let goal_drift_steps = steps
        .iter()
        .filter(|s| s.goal_aligned == Some(false))
        .count();
    let reverted_steps = steps.iter().filter(|s| s.reverted).count();
    let geomean_value = round4(geomean(&per_step));
    let efficiency = (total_tokens as f64 / per_step.len().max(1) as f64).round() as u64;
    TrajectoryReport {
        steps: per_step.len(),
        per_step,
        geomean: geomean_value,
        total_tokens,
        output_tokens,
        goal_drift_steps,
        reverted_steps,
        efficiency_tokens_per_step: efficiency,
    }
}

/// D1: outcome score (`PoC` `outcome_score`).
#[must_use]
pub fn outcome_score(run: &Run) -> f64 {
    let mut s = 1.0;
    if !run.outcome.achieved {
        return 0.0;
    }
    for gate in [
        run.outcome.tests_pass,
        run.outcome.typecheck_pass,
        run.outcome.lint_pass,
    ] {
        if gate == Some(false) {
            s *= 0.25;
        }
    }
    s
}

/// Ticket metadata parsed from a TICKET file: scope exceptions and
/// orchestrator-owned artifacts (`PoC` `load_ticket_metadata`).
#[derive(Debug, Default, Clone)]
pub struct TicketMetadata {
    /// Paths the implementer may write outside the declared scope.
    pub scope_exceptions: Vec<String>,
    /// Artifacts owned by the orchestrator (not implementer edits).
    pub orchestrator_artifacts: Vec<String>,
}

/// Parse scope-exceptions and orchestrator artifacts from a ticket file
/// (`PoC` `load_ticket_metadata`; validation 1:1).
///
/// # Errors
///
/// Returns [`EvalError::Metadata`] on malformed entries (mirrors `PoC`
/// `ValueError` — the gate fails loudly instead of scoring garbage).
pub fn load_ticket_metadata(path: &Path) -> Result<TicketMetadata, EvalError> {
    let mut meta = TicketMetadata::default();
    let text = fs::read_to_string(path).map_err(EvalError::Read)?;
    let lines: Vec<&str> = text.lines().collect();

    for line in &lines {
        if line.starts_with("- expected orchestrator post-run artifacts") {
            let Some((_, rest)) = line.split_once(':') else {
                return Err(EvalError::Metadata(format!(
                    "malformed orchestrator artifacts in {}: missing colon",
                    path.display()
                )));
            };
            meta.orchestrator_artifacts = rest
                .split(',')
                .map(|item| item.split(" (").next().unwrap_or("").trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
        }
    }

    for (index, line) in lines.iter().enumerate() {
        if !line.starts_with("scope-exceptions") {
            continue;
        }
        if *line != "scope-exceptions:" {
            return Err(EvalError::Metadata(format!(
                "malformed scope-exceptions in {}: expected 'scope-exceptions:'",
                path.display()
            )));
        }
        for item in &lines[index + 1..] {
            if !item.starts_with('-') {
                if !item.trim().is_empty() && !item.starts_with("<!--") {
                    return Err(EvalError::Metadata(format!(
                        "malformed scope-exceptions in {}: expected '- path'",
                        path.display()
                    )));
                }
                break;
            }
            let value = item[1..].trim().to_string();
            if value.is_empty()
                || value.contains(['*', '?', '[', ']'])
                || value.starts_with('/')
                || value.split('/').any(|part| part == "..")
            {
                return Err(EvalError::Metadata(format!(
                    "invalid scope-exception in {}: {value:?}",
                    path.display()
                )));
            }
            meta.scope_exceptions.push(value);
        }
        break;
    }
    Ok(meta)
}

/// Resolve ticket metadata for a run by scanning its goal for
/// `TICKET-<n>` (`PoC` `ticket_metadata_for_run`; missing ticket = empty).
#[must_use]
pub fn ticket_metadata_for_run(goal: &str, tickets_root: &Path) -> TicketMetadata {
    let Some(pos) = goal.find("TICKET-") else {
        return TicketMetadata::default();
    };
    let rest = &goal[pos + "TICKET-".len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let Ok(_number) = digits.parse::<u32>() else {
        return TicketMetadata::default();
    };
    let ticket = tickets_root.join(format!("TICKET-{digits}.md"));
    if !ticket.exists() {
        return TicketMetadata::default();
    }
    load_ticket_metadata(&ticket).unwrap_or_default()
}

/// True when `path` matches a scope entry (`PoC` `path_is_in_scope`;
/// `fnmatch` `*`/`?` + directory-prefix semantics).
#[must_use]
pub fn path_is_in_scope(path: &str, scope: &[String]) -> bool {
    scope.iter().any(|entry| {
        let entry = entry.split(" (").next().unwrap_or("").trim();
        if path == entry || glob_match(entry, path) {
            return true;
        }
        let directory = entry.trim_end_matches('/');
        !directory.is_empty()
            && !entry.contains(['*', '?', '[', ']'])
            && (entry.ends_with('/') || Path::new(directory).extension().is_none())
            && path.starts_with(&format!("{directory}/"))
    })
}

/// `fnmatch`-style glob: `*` matches any run, `?` exactly one char.
#[must_use]
fn glob_match(pattern: &str, text: &str) -> bool {
    let (Some(p), Some(t)) = (pattern.chars().next(), text.chars().next()) else {
        return pattern.is_empty() && text.is_empty();
    };
    match p {
        '*' => {
            let rest = &pattern[1..];
            glob_match(rest, text) || glob_match(pattern, &text[1..])
        }
        '?' => glob_match(&pattern[1..], &text[1..]),
        _ => p == t && glob_match(&pattern[1..], &text[1..]),
    }
}

/// Paths written outside the declared scope (`PoC` `find_scope_violations`;
/// missing paths fail closed).
#[must_use]
pub fn find_scope_violations(
    steps: &[Step],
    scope: &[String],
    metadata: &TicketMetadata,
) -> Vec<String> {
    let mut authorized: Vec<String> = scope.to_vec();
    authorized.extend(metadata.scope_exceptions.iter().cloned());
    authorized.extend(metadata.orchestrator_artifacts.iter().cloned());
    let mut violations = Vec::new();
    for step in steps {
        if step.tool != "write" && step.tool != "edit" {
            continue;
        }
        if step.paths.is_empty() {
            violations.push("<unknown write target>".to_string());
            continue;
        }
        violations.extend(
            step.paths
                .iter()
                .filter(|path| !path_is_in_scope(path, &authorized))
                .cloned(),
        );
    }
    violations
}

/// One tool-use mismatch between a run step and the golden trajectory
/// (additive diagnostic; the mismatch count and D3 score are unchanged).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ToolMismatch {
    /// 1-based step number in the run trajectory.
    pub step: usize,
    /// Tool the run used at this step.
    pub run_tool: String,
    /// Tool the golden trajectory expects at this step.
    pub golden_tool: String,
}

/// Tool family for parity comparison (ADR-0006): `write` and `edit` both
/// mean "modify a file" and normalize to one family; everything else
/// normalizes to itself.
#[must_use]
pub fn tool_family(tool: &str) -> &str {
    match tool {
        "write" | "edit" => "file-modify",
        other => other,
    }
}

/// `D3`: tool-use score (golden parity + scope violations) and mismatch
/// count (`PoC` `tool_score`; families per ADR-0006).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "exact port of PoC powi over a small mismatch count"
)]
pub fn tool_score(
    steps: &[Step],
    golden: &[Step],
    scope: &[String],
    metadata: &TicketMetadata,
) -> (f64, usize, Vec<ToolMismatch>) {
    let mut mismatches = 0;
    let mut detail = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        if golden
            .get(i)
            .is_some_and(|g| tool_family(&step.tool) != tool_family(&g.tool))
        {
            mismatches += 1;
            detail.push(ToolMismatch {
                step: i + 1,
                run_tool: step.tool.clone(),
                golden_tool: golden[i].tool.clone(),
            });
        }
    }
    let violations = find_scope_violations(steps, scope, metadata).len();
    (
        TOOL_PARITY_PENALTY.powi((mismatches + violations) as i32),
        mismatches,
        detail,
    )
}

/// Full scoring report for one run (`PoC` `score.py` report shape).
#[derive(Debug, Serialize)]
pub struct ScoreReport {
    /// Case name = parent directory of the run file.
    pub case: String,
    /// 4D dimension scores.
    pub dims: Dims,
    /// Trajectory detail block.
    pub trajectory_detail: TrajectoryReport,
    /// Tool mismatches vs the golden trajectory.
    pub tool_mismatches_vs_golden: usize,
    /// Per-step tool mismatches vs the golden trajectory (additive detail).
    pub tool_mismatches_detail: Vec<ToolMismatch>,
    /// Scope violations found.
    pub scope_violations: Vec<String>,
    /// Run cost in USD (floored at 0.0001).
    pub cost_usd: f64,
    /// Total tokens.
    pub tokens_total: u64,
    /// Outcome per dollar.
    pub cost_normalized_success: f64,
    /// Composite D1*D2*D3.
    pub composite: f64,
}

/// Dimension scores block of a report.
#[derive(Debug, Serialize)]
pub struct Dims {
    /// D1: outcome.
    pub outcome: f64,
    /// D2: trajectory geomean.
    pub trajectory: f64,
    /// D3: tool-use.
    pub tool: f64,
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// Load a golden trajectory from the golden dir (`PoC` `load_golden`).
///
/// # Errors
///
/// Returns [`EvalError`] when the file is missing or malformed.
pub fn load_golden(golden_dir: &Path, name: &str) -> Result<Vec<Step>, EvalError> {
    let text = fs::read_to_string(golden_dir.join(name))?;
    Ok(serde_json::from_str(&text)?)
}

/// Score a single run file (`PoC` `score.py` main).
///
/// # Errors
///
/// Returns [`EvalError`] on read/parse/validation failures (CLI maps to
/// exit 2 with `error: ...` on stderr, no traceback).
pub fn score_run(
    run_path: &Path,
    root: &Path,
    golden_dir: &Path,
) -> Result<ScoreReport, EvalError> {
    let text = fs::read_to_string(run_path)?;
    let run: Run = serde_json::from_str(&text)?;
    Run::validate(&run)?;
    let traj = score_trajectory(&run.trajectory);
    let out = outcome_score(&run);
    let golden = match &run.golden {
        Some(name) => load_golden(golden_dir, name)?,
        None => Vec::new(),
    };
    let metadata = ticket_metadata_for_run(&run.goal, &root.join("tickets"));
    let (tscore, mism, mism_detail) = tool_score(&run.trajectory, &golden, &run.scope, &metadata);
    let violations = find_scope_violations(&run.trajectory, &run.scope, &metadata);
    let cost = if run.cost_usd <= 0.0 {
        COST_FLOOR
    } else {
        run.cost_usd
    };
    let tokens = if run.tokens_total > 0 {
        run.tokens_total
    } else {
        traj.total_tokens
    };
    let composite = round4(out * traj.geomean * tscore);
    let cost_norm = round2(out / cost);
    Ok(ScoreReport {
        case: run_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("case")
            .to_string(),
        dims: Dims {
            outcome: out,
            trajectory: traj.geomean,
            tool: round4(tscore),
        },
        trajectory_detail: traj,
        tool_mismatches_vs_golden: mism,
        tool_mismatches_detail: mism_detail,
        scope_violations: violations,
        cost_usd: round4(cost),
        tokens_total: tokens,
        cost_normalized_success: cost_norm,
        composite,
    })
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Gate entry for one case (`PoC` `gate.py` `score_run` shape).
#[derive(Debug, Serialize, serde::Deserialize)]
pub struct GateEntry {
    /// Case name.
    pub case: String,
    /// Composite score.
    pub composite: f64,
    /// D1 outcome.
    pub outcome: f64,
    /// Run cost.
    pub cost_usd: f64,
    /// Tokens.
    pub tokens: u64,
}

/// Score every case under `cases_dir` for the gate (sorted by name).
///
/// # Errors
///
/// Returns [`EvalError`] when any case fails to read/parse/validate.
pub fn score_all_cases(
    cases_dir: &Path,
    root: &Path,
    golden_dir: &Path,
) -> Result<Vec<GateEntry>, EvalError> {
    let mut entries = Vec::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(cases_dir)? {
        let entry = entry?;
        let run = entry.path().join("run.json");
        if run.exists() {
            paths.push(run);
        }
    }
    paths.sort();
    for run in paths {
        let report = score_run(&run, root, golden_dir)?;
        entries.push(GateEntry {
            case: report.case.clone(),
            composite: report.composite,
            outcome: report.dims.outcome,
            cost_usd: report.cost_usd,
            tokens: report.tokens_total,
        });
    }
    Ok(entries)
}

/// Result of a gate run.
#[derive(Debug)]
pub struct GateResult {
    /// One message per case: NEW CASE, REGRESSION, or COST REGRESSION.
    pub messages: Vec<String>,
    /// Number of regressions found.
    pub failures: usize,
    /// Number of cases evaluated.
    pub case_count: usize,
}

/// Run the regression gate against a committed baseline (`PoC` `gate.py`).
#[must_use]
pub fn run_gate(entries: &[GateEntry], baseline: &[GateEntry], tolerance: f64) -> GateResult {
    let mut messages = Vec::new();
    let mut failures = 0;
    let base: std::collections::HashMap<&str, &GateEntry> =
        baseline.iter().map(|e| (e.case.as_str(), e)).collect();
    for entry in entries {
        match base.get(entry.case.as_str()) {
            None => messages.push(format!("NEW CASE: {} (no baseline) — ok", entry.case)),
            Some(b) => {
                let dcomp = b.composite - entry.composite;
                if dcomp > tolerance {
                    messages.push(format!(
                        "REGRESSION {}: composite {} -> {}",
                        entry.case, b.composite, entry.composite
                    ));
                    failures += 1;
                }
                if entry.outcome >= b.outcome && entry.cost_usd > b.cost_usd * MAX_COST_GROWTH {
                    messages.push(format!(
                        "COST REGRESSION {}: ${} -> ${} (outcome not improved)",
                        entry.case, b.cost_usd, entry.cost_usd
                    ));
                    failures += 1;
                }
            }
        }
    }
    GateResult {
        messages,
        failures,
        case_count: entries.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(tool: &str) -> Step {
        Step {
            step: 1,
            action: String::new(),
            tool: tool.to_string(),
            ok: Some(true),
            goal_aligned: Some(true),
            tokens: 0,
            output_tokens: 0,
            reverted: false,
            note: String::new(),
            paths: Vec::new(),
        }
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn geomean_of_ones_is_one() {
        assert!(approx(geomean(&[1.0, 1.0, 1.0]), 1.0));
    }

    #[test]
    fn geomean_zero_on_any_zero() {
        assert!(approx(geomean(&[1.0, 0.0]), 0.0));
    }

    #[test]
    fn geomean_empty_is_zero() {
        assert!(approx(geomean(&[]), 0.0));
    }

    #[test]
    fn step_failure_kills_score() {
        let mut s = step("exec");
        s.ok = Some(false);
        assert!(approx(step_score(&s), 0.0));
    }

    #[test]
    fn step_goal_drift_penalty() {
        let mut s = step("exec");
        s.goal_aligned = Some(false);
        assert!(approx(step_score(&s), 0.2));
    }

    #[test]
    fn step_reverted_penalty() {
        let mut s = step("exec");
        s.reverted = true;
        assert!(approx(step_score(&s), 0.1));
    }

    #[test]
    fn step_ungated_is_half() {
        let mut s = step("exec");
        s.ok = None;
        assert!(approx(step_score(&s), 0.5));
    }

    #[test]
    fn glob_star_matches_any_run() {
        assert!(glob_match(
            "scripts/schemas/*.json",
            "scripts/schemas/handoff.json"
        ));
        assert!(!glob_match(
            "scripts/schemas/*.py",
            "scripts/schemas/evil.json"
        ));
    }

    #[test]
    fn path_in_scope_directory_and_exact_and_glob() {
        assert!(path_is_in_scope(
            "artifacts/TICKET-004-v2/spec.md",
            &["artifacts/TICKET-004-v2/".to_string()]
        ));
        assert!(path_is_in_scope(
            "artifacts/TICKET-004-v2/spec.md",
            &["artifacts/TICKET-004-v2".to_string()]
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
}
