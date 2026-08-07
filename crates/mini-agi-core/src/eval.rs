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
    /// The golden trajectory file could not be read.
    #[error("cannot read golden file: {0}")]
    GoldenRead(std::io::Error),
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
    /// Deterministic verification command (verifiable reward layer,
    /// ADR-0011): executed by `run verify` IN `verify_target` to prove
    /// the outcome. Absent = the outcome is unverified by the kernel.
    #[serde(default)]
    pub verify_command: Option<String>,
    /// Directory (absolute or relative to the kernel root) where
    /// `verify_command` runs — the target repo of the work.
    #[serde(default)]
    pub verify_target: Option<String>,
    /// Verbal self-reflection on the run's failures (Reflexion, Phase 8
    /// slice 2): injected into rerun context via the failure register.
    #[serde(default)]
    pub reflection: Option<String>,
    /// MAST failure classification (one of the 14 modes, arXiv
    /// 2503.13657): what kind of failure this run exemplifies.
    #[serde(default)]
    pub mast: Option<String>,
    /// Steps of the run.
    pub trajectory: Vec<Step>,
    /// Kernel version that produced the run (versioned trace header,
    /// production-readiness C.2/F.3). `None` for legacy runs.
    #[serde(default)]
    pub kernel_version: Option<String>,
    /// Trajectory step count (trace aggregate). `None` for legacy runs.
    #[serde(default)]
    pub n_steps: Option<usize>,
    /// Exec/tool-call count (trace aggregate). `None` for legacy runs.
    #[serde(default)]
    pub n_toolcalls: Option<usize>,
    /// Wall-clock latency in seconds. `None` for legacy/reparse runs.
    #[serde(default)]
    pub latency_seconds: Option<u64>,
    /// Eval mode (production-readiness C.1): `capability` (hill-climbing,
    /// monitored) or `regression` (frozen, ~100% gated). `None`/absent =
    /// `regression`.
    #[serde(default)]
    pub mode: Option<String>,
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

/// Gate/test/verify commands whose failure is a REAL gate failure, not
/// a probe (ADR-0013).
const GATE_COMMANDS: &[&str] = &[
    "make verify",
    "make test",
    "cargo test",
    "cargo clippy",
    "cargo build",
    "cargo check",
    "pytest",
    "python -m unittest",
    "npm test",
    "npm run test",
    "node --test",
    "npx tsc",
    "mini-agi verify",
    "checkpoint.sh verify",
    "mvn test",
    "go test",
];

/// Probe-vs-gate classification (ADR-0013).
///
/// Is this failing step a real gate failure (score 0) rather than a
/// probe (ungated 0.5)? A step is a gate failure when it touches a path
/// in the run's declared `scope` OR its action is a gate/test/verify
/// command. Any other failing step is a probe — a diagnostic whose
/// failure says nothing about whether the work succeeded.
#[must_use]
pub fn is_gate_failure(run: &Run, step: &Step) -> bool {
    step.paths.iter().any(|p| path_is_in_scope(p, &run.scope))
        || GATE_COMMANDS.iter().any(|g| step.action.contains(g))
}

/// Per-step score honoring probe-vs-gate (ADR-0013).
///
/// A failing step that is NOT a gate failure is downgraded from 0 to
/// the ungated 0.5 (it is a probe, not a failed gate); everything else
/// uses `step_score`.
#[must_use]
pub fn step_score_gated(run: &Run, step: &Step) -> f64 {
    if step.ok == Some(false) && !is_gate_failure(run, step) {
        return 0.5;
    }
    step_score(step)
}

/// Repetition watchdog signal (hardening audit P1-5): the longest run of
/// consecutive identical `(tool, action)` steps in a run's trajectory.
///
/// A worker that repeats the same action verbatim is likely spinning, not
/// progressing — `max_repeated_steps` in the config converts this into a
/// warning at `loop verify` time.
#[must_use]
pub fn max_consecutive_repeat(run: &Run) -> usize {
    let mut best = 0;
    let mut cur = 0;
    let mut prev: Option<(&str, &str)> = None;
    for step in &run.trajectory {
        let key = (step.tool.as_str(), step.action.as_str());
        if prev == Some(key) {
            cur += 1;
        } else {
            cur = 1;
        }
        best = best.max(cur);
        prev = Some(key);
    }
    best
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

/// One step's process-supervision verdict (Phase 8 slice 5).
///
/// `suspicious` = the step-level signal contradicts the outcome-level
/// claim — where a judge should spend budget (2305.20050).
#[derive(Debug, Clone, PartialEq)]
pub struct StepVerdict {
    /// 1-based step index.
    pub step: u32,
    /// Tool used.
    pub tool: String,
    /// `step_score` of this step (0.0-1.0).
    pub score: f64,
    /// Whether this step is flagged for judge attention.
    pub suspicious: bool,
    /// ADR-0013: true when the step failed as a PROBE (ok:false but not
    /// a scope-touching or gate command) and was scored as ungated.
    pub probe_failure: bool,
}

/// Per-step process supervision for a run.
///
/// Heuristic (testable, deterministic): a step is suspicious when
/// (a) the run claims success but the step explicitly admits
/// misalignment (`goal_aligned == Some(false)`), or (b) the run claims
/// failure but every step is clean — the failure is unexplained at the
/// step level.
#[must_use]
pub fn score_steps(run: &Run) -> Vec<StepVerdict> {
    // A run counts as a clean success only when the outcome score is
    // full (achieved AND no failed outcome gates) — a gated failure is
    // not "success" for supervision purposes (codex review finding).
    let outcome_ok = outcome_score(run) >= 1.0;
    let all_clean = run
        .trajectory
        .iter()
        .all(|s| s.ok != Some(false) && s.goal_aligned != Some(false));
    run.trajectory
        .iter()
        .map(|s| {
            let score = step_score_gated(run, s);
            let probe_failure = s.ok == Some(false) && !is_gate_failure(run, s);
            let suspicious = if outcome_ok {
                s.goal_aligned == Some(false) || s.ok == Some(false) || s.reverted
            } else {
                all_clean
            };
            StepVerdict {
                step: s.step,
                tool: s.tool.clone(),
                score,
                suspicious,
                probe_failure,
            }
        })
        .collect()
}

/// Per-channel error audit of a run (cycle-33 finding, Flat Score #98).
///
/// The composite/D1 score is an end-of-trajectory outcome; per-step
/// failures can be hidden behind the run's "budget" (the number of
/// failed steps a run tolerates before it still counts as success).
/// This audit surfaces the per-channel error counts and an optimistic
/// `success_at_budget` bound — the run would *claim* success at budget k
/// only if its outcome is achieved and it had at most k failed steps.
/// Deterministic: derived only from the run's own steps.
///
/// The bound is optimistic, not a true counterfactual: the run was not
/// re-executed under budget k, so it cannot capture damage where the
/// run's own recovery absorbed failures it would have spent budget on
/// (the Flat Score study *re-ran* under tighter budgets to expose that).
/// Read it as "how tight a budget this run's recorded failures fit
/// under", not "how the run would behave under a tighter budget".
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ErrorBudgetAudit {
    /// Total steps in the trajectory.
    pub total_steps: usize,
    /// Steps whose deterministic gate failed (`ok == Some(false)`).
    pub failed_gate_steps: usize,
    /// Steps flagged as goal drift (`goal_aligned == Some(false)`).
    pub goal_drift_steps: usize,
    /// Steps reverted by the checkpoint cascade.
    pub reverted_steps: usize,
    /// Steps failing any of gate/goal/revert, deduplicated — a step that
    /// fails its gate AND drifts from the goal counts once. This is the
    /// budget denominator (channels overlap; the total does not).
    pub failed_steps: usize,
    /// Per-step failed count, by tool name (channel view).
    pub failed_by_tool: Vec<(String, usize)>,
    /// `success_at_budget[k]` is true when the run's *declared* outcome
    /// is achieved AND the number of failed steps is at most `k`. Index 0
    /// is the strictest budget (zero tolerance); the last entry is the
    /// run's actual tolerance. A run whose outcome is not achieved has
    /// all-false entries (there is no budget at which an unachieved run
    /// counts as success).
    pub success_at_budget: Vec<bool>,
}

/// Build the per-channel error audit for a run.
#[must_use]
pub fn error_budget_audit(run: &Run) -> ErrorBudgetAudit {
    let total_steps = run.trajectory.len();
    let failed_gate_steps = run
        .trajectory
        .iter()
        .filter(|s| s.ok == Some(false))
        .count();
    let goal_drift_steps = run
        .trajectory
        .iter()
        .filter(|s| s.goal_aligned == Some(false))
        .count();
    let reverted_steps = run.trajectory.iter().filter(|s| s.reverted).count();
    let mut by_tool: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for s in &run.trajectory {
        if s.ok == Some(false) || s.goal_aligned == Some(false) || s.reverted {
            *by_tool.entry(s.tool.clone()).or_insert(0) += 1;
        }
    }
    let failed_by_tool: Vec<(String, usize)> = by_tool.into_iter().collect();
    let achieved = run.outcome.achieved;
    // Budget k = number of failed steps tolerated. A step "counts" as a
    // failure when it fails a gate, drifts from the goal, or was
    // reverted (double counting is avoided: a step is a failure once).
    let failed_steps = run
        .trajectory
        .iter()
        .filter(|s| s.ok == Some(false) || s.goal_aligned == Some(false) || s.reverted)
        .count();
    let success_at_budget = (0..=failed_steps)
        .map(|k| achieved && failed_steps <= k)
        .collect();
    ErrorBudgetAudit {
        total_steps,
        failed_gate_steps,
        goal_drift_steps,
        reverted_steps,
        failed_steps,
        failed_by_tool,
        success_at_budget,
    }
}

/// Deterministic run-failure classifier for the repair gate (GGC #60):
/// mechanical failures a rerun can target vs semantic ones a blind retry
/// would reproduce.
///
/// GGC measured ~78% of generated-query errors as *semantic*
/// (executable-but-wrong) and that a gate deciding *when* to repair
/// beats correcting everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairSignal {
    /// Clean success — no repair needed.
    Clean,
    /// At least one step failed a deterministic gate, drifted from the
    /// goal, or was reverted: a mechanical failure a rerun can target.
    Mechanical,
    /// No failed/reverted steps but the run is not a clean success: a
    /// semantic failure — the output is executable-but-wrong, so a blind
    /// retry is likely to reproduce it (GGC: ~78% of failures).
    Semantic,
    /// Repetition watchdog tripped: the trajectory repeats the same
    /// (tool, action) consecutively — a spinning worker that a rerun
    /// must not blindly repeat.
    Spinning,
}

impl std::fmt::Display for RepairSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clean => write!(f, "clean"),
            Self::Mechanical => write!(f, "mechanical"),
            Self::Semantic => write!(f, "semantic"),
            Self::Spinning => write!(f, "spinning"),
        }
    }
}

/// Classify a run's failure mode for the repair gate.
#[must_use]
pub fn repair_signal(run: &Run, max_repeated_steps: Option<usize>) -> RepairSignal {
    // Spinning takes precedence over Clean: a trajectory that repeats
    // the same (tool, action) consecutively is a warning even when the
    // run claims success (the repeat can be a legit probe, but a rerun
    // must not blindly resubmit the loop).
    let repeated = max_repeated_steps.is_some_and(|m| max_consecutive_repeat(run) > m);
    if repeated {
        return RepairSignal::Spinning;
    }
    if run.outcome.achieved
        && run
            .trajectory
            .iter()
            .all(|s| s.ok != Some(false) && s.goal_aligned != Some(false) && !s.reverted)
    {
        return RepairSignal::Clean;
    }
    let mechanical = run
        .trajectory
        .iter()
        .any(|s| s.ok == Some(false) || s.goal_aligned == Some(false) || s.reverted);
    if mechanical {
        RepairSignal::Mechanical
    } else {
        RepairSignal::Semantic
    }
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
    trajectory_report(per_step, steps)
}

/// Score a trajectory with the probe-vs-gate rule (ADR-0013): used by
/// `score_run` so a failed probe does not zero the geomean. A step that
/// fails AND touches the scope or runs a gate command still zeroes.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "same PoC float math; ADR-0013 per-step adjustment"
)]
pub fn score_trajectory_gated(run: &Run) -> TrajectoryReport {
    let per_step: Vec<f64> = run
        .trajectory
        .iter()
        .map(|s| step_score_gated(run, s))
        .collect();
    trajectory_report(per_step, &run.trajectory)
}

/// Shared trajectory report builder from per-step scores.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "exact port of PoC float math; efficiency = round(total/steps)"
)]
fn trajectory_report(per_step: Vec<f64>, steps: &[Step]) -> TrajectoryReport {
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

/// Case family for capability telemetry (Phase 9 slice 4): the case
/// name minus trailing `-<digits><suffix>` and `-rerun` parts —
/// `real-ticket-001-v2(-rerun)` groups under `real-ticket`.
#[must_use]
pub fn family_of(case: &str) -> String {
    let base = case.strip_suffix("-rerun").unwrap_or(case);
    let digit_pos = base
        .char_indices()
        .find(|(_, c)| c.is_ascii_digit())
        .map(|(i, _)| i);
    match digit_pos {
        Some(i) if i > 0 => base[..i].trim_end_matches('-').to_string(),
        _ => base.to_string(),
    }
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
    /// Per-channel error audit (cycle-33 Flat Score pattern): failed
    /// steps by channel and the success-at-budget projection.
    pub error_budget: ErrorBudgetAudit,
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
    let text = fs::read_to_string(golden_dir.join(name)).map_err(EvalError::GoldenRead)?;
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
    let traj = score_trajectory_gated(&run);
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
        error_budget: error_budget_audit(&run),
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
    /// Tool mismatches vs the golden trajectory (absent in baselines
    /// written before ADR-0006's gate wiring; defaults to 0).
    #[serde(default)]
    pub tool_mismatches: usize,
    /// Eval mode (production-readiness C.1): capability cases are
    /// monitored, regression cases are hard-gated. Defaults to
    /// "regression" for legacy baselines.
    #[serde(default = "default_regression_mode")]
    pub mode: String,
}

fn default_regression_mode() -> String {
    "regression".into()
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
        let mode = serde_json::from_str::<Run>(&fs::read_to_string(&run).unwrap_or_default())
            .ok()
            .and_then(|r| r.mode)
            .unwrap_or_else(default_regression_mode);
        entries.push(GateEntry {
            case: report.case.clone(),
            composite: report.composite,
            outcome: report.dims.outcome,
            cost_usd: report.cost_usd,
            tokens: report.tokens_total,
            tool_mismatches: report.tool_mismatches_vs_golden,
            mode,
        });
    }
    Ok(entries)
}

/// Result of a gate run.
#[derive(Debug)]
pub struct GateResult {
    /// One message per case: NEW CASE, REGRESSION, TOOL REGRESSION, or
    /// COST REGRESSION.
    pub messages: Vec<String>,
    /// Number of regressions found.
    pub failures: usize,
    /// Number of cases evaluated.
    pub case_count: usize,
}

/// Run the regression gate against a committed baseline (`PoC` `gate.py`).
///
/// `mismatch_tolerance` allows tool mismatches to grow by up to that many
/// before the gate fails (ADR-0006 wiring; Phase 6.2: mismatch regression
/// is a hard signal).
#[must_use]
pub fn run_gate(
    entries: &[GateEntry],
    baseline: &[GateEntry],
    tolerance: f64,
    mismatch_tolerance: usize,
) -> GateResult {
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
                    if entry.mode == "capability" {
                        // Production-readiness C.1: a capability-case drop
                        // is a monitoring signal, not a hard gate failure.
                        messages.push(format!(
                            "CAPABILITY DROP {}: composite {} -> {} (monitored — not a hard fail)",
                            entry.case, b.composite, entry.composite
                        ));
                    } else {
                        messages.push(format!(
                            "REGRESSION {}: composite {} -> {}",
                            entry.case, b.composite, entry.composite
                        ));
                        failures += 1;
                    }
                }
                if entry.tool_mismatches > b.tool_mismatches + mismatch_tolerance {
                    messages.push(format!(
                        "TOOL REGRESSION {}: mismatches {} -> {}",
                        entry.case, b.tool_mismatches, entry.tool_mismatches
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
    // Best-state bound (Phase 8 slice 3, codex review): a baseline case
    // that VANISHED from evals/cases is a regression — silently removing
    // a case must not shrink the frozen suite to green.
    for baseline_entry in baseline {
        if !entries.iter().any(|e| e.case == baseline_entry.case) {
            messages.push(format!(
                "REGRESSION {}: case missing from evals/cases (frozen suite shrank)",
                baseline_entry.case
            ));
            failures += 1;
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

    fn step_with(tool: &str, action: &str, ok: Option<bool>, paths: &[&str]) -> Step {
        let mut s = step(tool);
        s.action = action.to_string();
        s.ok = ok;
        s.paths = paths.iter().map(ToString::to_string).collect();
        s
    }

    fn run_with(steps: Vec<Step>) -> Run {
        Run {
            goal: "g".into(),
            scope: vec!["src/".to_string()],
            outcome: Outcome {
                achieved: true,
                tests_pass: Some(true),
                typecheck_pass: Some(true),
                lint_pass: Some(true),
            },
            tokens_total: 0,
            cost_usd: 0.0,
            golden: None,
            verify_command: None,
            verify_target: None,
            reflection: None,
            mast: None,
            trajectory: steps,
            kernel_version: None,
            n_steps: None,
            n_toolcalls: None,
            latency_seconds: None,
            mode: None,
            extra: serde_json::Map::new(),
        }
    }

    fn score_steps_composite(steps: Vec<Step>) -> f64 {
        let run = run_with(steps);
        score_trajectory_gated(&run).geomean
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
    fn watchdog_counts_only_consecutive_identical_steps() {
        let run = Run {
            goal: "g".into(),
            scope: vec![],
            outcome: Outcome {
                achieved: true,
                tests_pass: Some(true),
                typecheck_pass: Some(true),
                lint_pass: Some(true),
            },
            tokens_total: 0,
            cost_usd: 0.0,
            golden: None,
            verify_command: None,
            verify_target: None,
            reflection: None,
            mast: None,
            trajectory: vec![
                step("exec"),
                step("exec"),
                step("exec"),
                step("read"),
                step("exec"),
            ],
            kernel_version: None,
            n_steps: None,
            n_toolcalls: None,
            latency_seconds: None,
            mode: None,
            extra: serde_json::Map::new(),
        };
        // The three identical execs are consecutive; the read and final
        // exec break the run, so the max repeat is 3, not 4.
        assert_eq!(max_consecutive_repeat(&run), 3);
    }

    #[test]
    fn trace_header_parses_and_is_reported() {
        // Production-readiness C.2/F.3: a run with the versioned trace
        // header parses, validates, and survives scoring.
        let mut run = run_with(vec![step("exec"), step("write")]);
        run.kernel_version = Some("0.3.0".into());
        run.n_steps = Some(2);
        run.n_toolcalls = Some(1);
        run.latency_seconds = Some(7);
        Run::validate(&run).expect("trace header must not break validation");
        assert_eq!(run.n_toolcalls, Some(1));
    }

    #[test]
    fn legacy_runs_without_header_still_parse() {
        // Old run.json files (no kernel_version/n_steps/...) must keep
        // parsing via serde defaults — the committed 24-case corpus.
        let run = run_with(vec![step("exec")]);
        assert_eq!(run.kernel_version, None);
        assert_eq!(run.n_steps, None);
        assert_eq!(run.n_toolcalls, None);
        assert_eq!(run.latency_seconds, None);
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

    fn gate_entry(case: &str, composite: f64, mismatches: usize) -> GateEntry {
        GateEntry {
            case: case.to_string(),
            composite,
            outcome: 1.0,
            cost_usd: 0.1,
            tokens: 1000,
            tool_mismatches: mismatches,
            mode: "regression".into(),
        }
    }

    #[test]
    fn tool_family_merges_write_and_edit() {
        let meta = TicketMetadata::default();
        let steps = [step("write"), step("edit")];
        let golden = [step("edit"), step("write")];
        let (_, mismatches, detail) = tool_score(&steps, &golden, &[], &meta);
        assert_eq!(mismatches, 0, "write vs edit must not mismatch (ADR-0006)");
        assert!(detail.is_empty());
        let steps = [step("exec"), step("write")];
        let golden = [step("write"), step("exec")];
        let (_, mismatches, _) = tool_score(&steps, &golden, &[], &meta);
        assert_eq!(mismatches, 2, "exec vs write stays a real mismatch");
    }

    #[test]
    fn gate_flags_tool_mismatch_growth_beyond_tolerance() {
        let entries = [gate_entry("case-a", 0.9, 4)];
        let baseline = [gate_entry("case-a", 0.9, 1)];
        let bad = run_gate(&entries, &baseline, 0.05, 1);
        assert_eq!(bad.failures, 1);
        assert!(bad.messages[0].starts_with("TOOL REGRESSION case-a: mismatches 1 -> 4"));
        let zero_tolerance = run_gate(&entries, &baseline, 0.05, 0);
        assert_eq!(zero_tolerance.failures, 1);
    }

    #[test]
    fn gate_tolerates_grown_mismatches_within_tolerance() {
        let entries = [gate_entry("case-a", 0.9, 2)];
        let baseline = [gate_entry("case-a", 0.9, 1)];
        let result = run_gate(&entries, &baseline, 0.05, 1);
        assert_eq!(result.failures, 0);
    }

    #[test]
    fn capability_drop_is_monitored_not_failed() {
        // Production-readiness C.1: a capability-case composite drop is
        // reported (CAPABILITY DROP) but does not fail the gate.
        let mut entry = gate_entry("cap-a", 0.4, 0);
        entry.mode = "capability".into();
        let baseline = [gate_entry("cap-a", 0.9, 0)];
        let result = run_gate(&[entry], &baseline, 0.05, 1);
        assert_eq!(result.failures, 0, "{:?}", result.messages);
        assert!(
            result
                .messages
                .iter()
                .any(|m| m.starts_with("CAPABILITY DROP")),
            "{:?}",
            result.messages
        );
    }

    #[test]
    fn regression_drop_still_fails() {
        // A regression-case drop is unchanged: hard failure.
        let entry = gate_entry("reg-a", 0.4, 0);
        let baseline = [gate_entry("reg-a", 0.9, 0)];
        let result = run_gate(&[entry], &baseline, 0.05, 1);
        assert_eq!(result.failures, 1, "{:?}", result.messages);
        assert!(
            result.messages.iter().any(|m| m.starts_with("REGRESSION")),
            "{:?}",
            result.messages
        );
    }

    #[test]
    fn probe_failure_does_not_zero_trajectory() {
        // ADR-0013: a failed diagnostic (ok:false, no scope paths, not a
        // gate command) is a probe — scored as ungated, not 0.
        let steps = vec![
            step_with("exec", "sed -n '1,20p' missing.txt", Some(false), &[]),
            step_with("write", "Wrote src/main.rs", Some(true), &["src/main.rs"]),
            step_with("exec", "make verify", Some(true), &[]),
        ];
        let g = score_steps_composite(steps);
        // 0.5 (probe) * 1 * 1 -> geomean (0.5)^(1/3) ~ 0.7937, rounded to 4.
        assert!(g > 0.0, "probe must not zero the trajectory, got {g}");
        assert!((g - 0.7937).abs() < 1e-4, "got {g}");
    }

    #[test]
    fn scope_touching_failure_still_zeroes() {
        // ADR-0013: a failing step that touches a scope path is a real
        // gate failure — the trajectory still zeroes.
        let steps = vec![
            step_with("write", "Wrote src/main.rs", Some(true), &["src/main.rs"]),
            step_with(
                "edit",
                "Wrote src/broken.rs",
                Some(false),
                &["src/broken.rs"],
            ),
        ];
        assert!(approx(score_steps_composite(steps), 0.0));
    }

    #[test]
    fn gate_command_failure_still_zeroes() {
        // ADR-0013: a failing gate/test command is a real gate failure.
        let steps = vec![
            step_with("exec", "make verify", Some(true), &[]),
            step_with("exec", "make verify", Some(false), &[]),
        ];
        assert!(approx(score_steps_composite(steps), 0.0));
    }

    #[test]
    fn probe_failure_is_flagged_in_step_verdicts() {
        let run = run_with(vec![
            step_with("exec", "which nonexistent", Some(false), &[]),
            step_with("exec", "make verify", Some(true), &[]),
        ]);
        let verdicts = score_steps(&run);
        assert!(verdicts[0].probe_failure, "probe must be flagged");
        assert!(!verdicts[1].probe_failure, "gate pass is not a probe");
        assert!(approx(verdicts[0].score, 0.5));
    }

    #[test]
    fn error_budget_audit_counts_channels_and_projects_success() {
        let a = step("tool-a");
        let mut b = step("tool-b");
        let mut c = step("tool-c");
        b.ok = Some(false);
        b.goal_aligned = Some(false);
        c.reverted = true;
        let run = run_with(vec![a, b, c]);
        let audit = error_budget_audit(&run);
        assert_eq!(audit.total_steps, 3);
        assert_eq!(audit.failed_gate_steps, 1, "only b failed its gate");
        assert_eq!(audit.goal_drift_steps, 1, "only b drifted from goal");
        assert_eq!(audit.reverted_steps, 1, "only c was reverted");
        // b fails gate AND drifts from goal but counts once (dedup):
        // failed_steps = b + c = 2, not 3 (channels overlap, total does not).
        assert_eq!(audit.failed_steps, 2);
        assert_eq!(audit.failed_by_tool.len(), 2, "b and c, not a");
        assert!(
            audit
                .failed_by_tool
                .iter()
                .any(|(t, n)| t == "tool-b" && *n == 1)
        );
        assert!(
            audit
                .failed_by_tool
                .iter()
                .any(|(t, n)| t == "tool-c" && *n == 1)
        );
        // Two distinct failing steps: success only at budget >= 2.
        assert_eq!(audit.success_at_budget, vec![false, false, true]);
    }

    #[test]
    fn error_budget_audit_clean_run_is_strict_success() {
        let run = run_with(vec![step("tool-a"), step("tool-b")]);
        let audit = error_budget_audit(&run);
        assert_eq!(audit.failed_gate_steps, 0);
        assert_eq!(audit.goal_drift_steps, 0);
        assert_eq!(audit.reverted_steps, 0);
        assert_eq!(audit.success_at_budget, vec![true]);
    }

    #[test]
    fn error_budget_audit_unachieved_run_has_no_success_budget() {
        let mut run = run_with(vec![step("tool-a")]);
        run.outcome.achieved = false;
        let audit = error_budget_audit(&run);
        assert_eq!(audit.failed_gate_steps, 0);
        assert!(
            audit.success_at_budget.iter().all(|s| !s),
            "an unachieved run is not a success at any budget"
        );
    }

    #[test]
    fn repair_signal_classifies_clean_mechanical_semantic_spinning() {
        // Clean.
        assert_eq!(
            repair_signal(&run_with(vec![step("tool-a")]), None),
            RepairSignal::Clean
        );
        // Mechanical: a step failed its gate.
        let mut bad = step("tool-b");
        bad.ok = Some(false);
        assert_eq!(
            repair_signal(&run_with(vec![step("tool-a"), bad]), None),
            RepairSignal::Mechanical
        );
        // Semantic: clean steps but unachieved outcome.
        let mut sem = run_with(vec![step("tool-a")]);
        sem.outcome.achieved = false;
        assert_eq!(repair_signal(&sem, None), RepairSignal::Semantic);
        // Spinning: repeated (tool, action) beyond the threshold.
        let mut spinning = step("exec");
        spinning.action = "make verify".into();
        let spin_run = run_with(vec![spinning.clone(), spinning.clone(), spinning]);
        assert_eq!(repair_signal(&spin_run, Some(2)), RepairSignal::Spinning);
        // A clean run ignores the repeat threshold.
        assert_eq!(repair_signal(&spin_run, Some(10)), RepairSignal::Clean);
    }
}

#[cfg(test)]
mod process_supervision_tests {
    use super::*;

    fn run_with(outcome_ok: bool, aligned_flags: &[Option<bool>]) -> Run {
        let trajectory: Vec<Step> = aligned_flags
            .iter()
            .enumerate()
            .map(|(i, aligned)| Step {
                step: u32::try_from(i + 1).unwrap(),
                action: String::new(),
                tool: "exec".into(),
                ok: Some(true),
                goal_aligned: *aligned,
                tokens: 0,
                output_tokens: 0,
                reverted: false,
                note: String::new(),
                paths: Vec::new(),
            })
            .collect();
        Run {
            goal: "g".into(),
            scope: vec!["x".into()],
            outcome: Outcome {
                achieved: outcome_ok,
                tests_pass: Some(outcome_ok),
                typecheck_pass: Some(outcome_ok),
                lint_pass: Some(outcome_ok),
            },
            tokens_total: 0,
            cost_usd: 0.0,
            golden: None,
            verify_command: None,
            verify_target: None,
            reflection: None,
            mast: None,
            trajectory,
            kernel_version: None,
            n_steps: None,
            n_toolcalls: None,
            latency_seconds: None,
            mode: None,
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn successful_run_flags_misaligned_step() {
        let run = run_with(true, &[Some(true), Some(false), Some(true)]);
        let verdicts = score_steps(&run);
        assert_eq!(verdicts.len(), 3);
        assert!(!verdicts[0].suspicious);
        assert!(
            verdicts[1].suspicious,
            "misaligned step in a successful run"
        );
        assert!(!verdicts[2].suspicious);
    }

    #[test]
    fn failed_run_with_all_clean_steps_is_unexplained() {
        let run = run_with(false, &[Some(true), Some(true)]);
        let verdicts = score_steps(&run);
        assert!(
            verdicts.iter().all(|v| v.suspicious),
            "failure unexplained at step level"
        );
    }

    #[test]
    fn failed_run_with_explicit_bad_step_is_explained() {
        let run = run_with(false, &[Some(true), Some(false)]);
        let verdicts = score_steps(&run);
        assert!(
            !verdicts.iter().any(|v| v.suspicious),
            "the bad step explains the failure"
        );
    }
}

#[cfg(test)]
mod family_tests {
    use super::family_of;

    #[test]
    fn families_group_suffix_variants() {
        assert_eq!(family_of("real-ticket-001-v2"), "real-ticket");
        assert_eq!(family_of("real-ticket-001-v2-rerun"), "real-ticket");
        assert_eq!(family_of("codex-exp-002"), "codex-exp");
        assert_eq!(family_of("reactive-loop"), "reactive-loop");
        assert_eq!(family_of("flailing"), "flailing");
        assert_eq!(family_of("harnessed"), "harnessed");
    }
}
