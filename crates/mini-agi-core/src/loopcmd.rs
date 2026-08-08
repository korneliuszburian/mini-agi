//! Proactive composition loop (Phase 6.4): the gap -> ticket -> slice ->
//! rerun loop without human routing (ADR-0005 failure-signal loop; Wish
//! Factory pattern, Yegge 2026-08, canonical 2026-08-03-002).
//!
//! Modes: `status` (read-only rows: cases below the loop target, their
//! tickets and claims), `dispatch` (pick the worst open case, ensure its
//! ticket, claim it, write the slice spec), `verify` (score + ingest a
//! rerun; at the target, release the claim and report the gap closed).

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::eval;
use crate::ticket::{self, Ticket};

/// Composite a case must reach to leave the loop's open set (same bar as
/// TICKET-9's rerun target).
pub const TARGET_COMPOSITE: f64 = 0.5;

/// One case's loop row: composite, ticket mapping, claim, ticket status.
#[derive(Debug)]
pub struct LoopRow {
    /// Case name.
    pub case: String,
    /// Latest composite score.
    pub composite: f64,
    /// Composite of the `<case>-rerun` case, when it exists.
    pub rerun_composite: Option<f64>,
    /// Best composite across original + reruns (SQLQE #19 best-result).
    pub best_composite: Option<f64>,
    /// Rerun attempts recorded for the case (1 original + reruns).
    pub attempts: usize,
    /// Repair-gate classification of the latest run (GGC #60).
    pub repair_signal: Option<eval::RepairSignal>,
    /// True when rerun attempts exceed the configured bound and the best
    /// result is still below the target (CRC #69 abstention: further
    /// retries would only burn budget) — the case needs a human decision.
    pub exhausted: bool,
    /// Mapped ticket id, when one exists.
    pub ticket: Option<String>,
    /// Ticket lifecycle status (OPEN/CLOSED).
    pub status: Option<String>,
    /// Claimant holding the lease, when claimed.
    pub claimant: Option<String>,
}

/// `loop status` result, sorted by composite ascending.
#[derive(Debug)]
pub struct LoopStatus {
    /// Rows below the loop target.
    pub cases: Vec<LoopRow>,
    /// Mean composite across all runs.
    pub composite_avg: f64,
    /// Number of scored runs.
    pub runs: usize,
}

/// Find the ticket whose goal/title references `case` (backlog dedup rule)
/// or whose id corresponds to the case (`real-ticket-001-v2` ->
/// `TICKET-001-v2`), with a digit boundary on the id match.
#[must_use]
pub fn ticket_for_case(root: &Path, case: &str) -> Option<Ticket> {
    let case_lower = case.to_lowercase();
    ticket::list_tickets(root)
        .unwrap_or_default()
        .into_iter()
        .find(|t| {
            t.goal.contains(case) || t.title.contains(case) || id_matches_case(&t.id, &case_lower)
        })
}

/// Id-to-case match with a digit boundary: `ticket-001` must not match a
/// case containing `ticket-0012`.
fn id_matches_case(id: &str, case_lower: &str) -> bool {
    let id_lower = id.to_lowercase();
    let Some(rest) = id_lower.strip_prefix("ticket-") else {
        return false;
    };
    let needle = format!("ticket-{rest}");
    case_lower.match_indices(&needle).any(|(pos, _)| {
        case_lower[pos + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_digit())
    })
}

/// Claimant of a ticket, if any.
#[must_use]
fn claimant_for(root: &Path, ticket_id: &str) -> Option<String> {
    ticket::read_claims(root)
        .unwrap_or_default()
        .into_iter()
        .find(|c| c.ticket == ticket_id)
        .map(|c| c.claimant)
}

/// Published time series (Phase 8 slice 3, Compounding-Test discipline):
/// one row per closed gap appended to `docs/METRICS.md`.
///
/// # Errors
///
/// Returns the underlying filesystem error — callers report, never
/// silently swallow.
fn append_metrics(root: &Path, case: &str, composite: f64, tokens: u64) -> std::io::Result<()> {
    let path = root.join("docs/METRICS.md");
    fs::create_dir_all(path.parent().unwrap_or(root))?;
    let family = crate::eval::family_of(case);
    let header = "| date | case | family | composite | tokens |\n| --- | --- | --- | --- | --- |\n";
    let row = format!(
        "| {} | {} | {} | {composite:.4} | {tokens} |\n",
        crate::memory::utc_now_date(),
        case,
        family
    );
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let old_format = existing.contains("| date |") && !existing.contains("| family |");
    let body = if old_format {
        let mut migrated = header.to_string();
        for line in existing.lines() {
            let cols: Vec<&str> = line.split('|').map(str::trim).collect();
            if cols.len() >= 6
                && !cols[1].is_empty()
                && cols[1] != "date"
                && !cols[1].starts_with('-')
            {
                let case = cols[2];
                let _ = writeln!(
                    migrated,
                    "| {} | {} | {} | {} | {} |",
                    cols[1],
                    case,
                    crate::eval::family_of(case),
                    cols[3],
                    cols[4]
                );
            }
        }
        migrated
    } else if existing.contains("| date |") {
        existing
    } else {
        header.to_string()
    };
    // A migrated file is always written back so the header lands; the
    // dedup guard applies only to new-format rows.
    if old_format {
        if !body.contains(&format!("| {case} |")) {
            return fs::write(&path, format!("{body}{row}"));
        }
        return fs::write(&path, body);
    }
    if body.contains(&format!("| {case} |")) {
        return Ok(());
    }
    fs::write(&path, format!("{body}{row}"))
}

/// Count of rerun attempt dirs for a case (`<case>-rerun`, `<case>-rerun-2`,
/// ...) — the pilot-before-scale numerator (Ringelmann 2606.02646).
#[must_use]
pub fn count_reruns(root: &Path, case: &str) -> usize {
    let cases_dir = root.join("evals/cases");
    let Ok(entries) = fs::read_dir(&cases_dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            is_rerun_attempt_dir(&name, case) && e.path().join("run.json").is_file()
        })
        .count()
}

/// Composite of the best rerun attempt: max over every
/// `<case>-rerun`, `-rerun-2`, ... dir, when any exists. Tracks the
/// *best*, not the latest (SQLQE #19: a bad retry must not regress).
#[must_use]
pub fn rerun_composite(root: &Path, case: &str) -> Option<f64> {
    let cases_dir = root.join("evals/cases");
    let Ok(entries) = fs::read_dir(&cases_dir) else {
        return None;
    };
    let mut best: Option<f64> = None;
    for e in entries.flatten() {
        let file_name = e.file_name();
        let name = file_name.to_string_lossy();
        if !is_rerun_attempt_dir(&name, case) {
            continue;
        }
        let run = e.path().join("run.json");
        if !run.is_file() {
            continue;
        }
        if let Ok(r) = eval::score_run(&run, root, &root.join("evals/golden")) {
            best = Some(best.map_or(r.composite, |b| b.max(r.composite)));
        }
    }
    best
}

/// Best composite across the original and all rerun attempts (SQLQE
/// #19 best-result tracking: return the best seen when retries exhaust;
/// a bad retry cannot regress). `None` when no runnable run exists.
#[must_use]
pub fn best_composite(root: &Path, case: &str) -> Option<f64> {
    let original = {
        let run = root.join("evals/cases").join(case).join("run.json");
        eval::score_run(&run, root, &root.join("evals/golden"))
            .ok()
            .map(|r| r.composite)
    };
    match (original, rerun_composite(root, case)) {
        (Some(o), Some(r)) => Some(o.max(r)),
        (Some(o), None) => Some(o),
        (None, Some(r)) => Some(r),
        (None, None) => None,
    }
}

/// A case is closed when its rerun reaches the loop target — the original
/// run stays as a historical fixture (TICKET-9 semantics).
#[must_use]
pub fn case_closed_by_rerun(root: &Path, case: &str) -> bool {
    rerun_composite(root, case)
        .is_some_and(|c| c >= crate::config::Config::target_composite_for(root))
}

/// Cases below the loop target with their work-graph mapping.
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn status(root: &Path) -> Result<LoopStatus, io::Error> {
    let report = crate::insights::insights(root)?;
    let mut rows = Vec::new();
    for case in &report.cases {
        if case.composite >= crate::config::Config::target_composite_for(root) {
            continue;
        }
        let ticket = ticket_for_case(root, &case.case);
        let (ticket_id, status_, claimant) = ticket.as_ref().map_or((None, None, None), |t| {
            (
                Some(t.id.clone()),
                Some(t.status.clone()),
                claimant_for(root, &t.id),
            )
        });
        // Compute the rerun best ONCE per case and derive the original
        // score from the already-scored insight row (cycle-33 review F4:
        // avoids re-scanning + re-scoring the cases dir per call).
        let rerun = rerun_composite(root, &case.case);
        let best = match (case.composite, rerun) {
            (o, Some(r)) => Some(o.max(r)),
            (o, None) => Some(o),
        };
        let attempts = 1 + count_reruns(root, &case.case);
        // Repair gate + bounded-retry abstention (cycle-33 findings):
        // surface the per-case repair classification and whether further
        // retries are pointless (attempts past the configured bound with
        // the best result still below the target — CRC #69 abstention).
        let repair_signal =
            std::fs::read_to_string(root.join("evals/cases").join(&case.case).join("run.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<eval::Run>(&text).ok())
                .map(|run| {
                    eval::repair_signal(&run, crate::config::Config::load(root).max_repeated_steps)
                });
        let exhausted = crate::config::Config::load(root)
            .max_rerun_attempts
            .is_some_and(|limit| attempts > limit)
            && !best.is_some_and(|b| b >= crate::config::Config::target_composite_for(root));
        rows.push(LoopRow {
            case: case.case.clone(),
            composite: case.composite,
            rerun_composite: rerun,
            best_composite: best,
            attempts,
            repair_signal,
            exhausted,
            ticket: ticket_id,
            status: status_,
            claimant,
        });
    }
    rows.sort_by(|a, b| a.composite.total_cmp(&b.composite));
    Ok(LoopStatus {
        cases: rows,
        composite_avg: report.composite_avg,
        runs: report.runs,
    })
}

/// True when `case` is a rerun-output dir (`-rerun`, `-rerun-2`, ...) —
/// the OUTPUT of a rerun, never a dispatchable source itself.
fn is_rerun_case(case: &str) -> bool {
    let Some(idx) = case.find("-rerun") else {
        return false;
    };
    let tail = &case[idx + "-rerun".len()..];
    tail.is_empty()
        || tail
            .strip_prefix('-')
            .is_some_and(|s| s.parse::<usize>().is_ok())
}

/// True when `name` is a rerun-attempt dir for `case` (`{case}-rerun`,
/// `{case}-rerun-2`, ...) — the strict counterpart of the loose
/// `starts_with` check, shared by `count_reruns`/`rerun_composite` so a
/// `-rerun-junk` dir is neither counted as an attempt nor treated as a
/// dispatchable source (cycle-33 review F7).
fn is_rerun_attempt_dir(name: &str, case: &str) -> bool {
    name.starts_with(&format!("{case}-rerun")) && is_rerun_case(name)
}

/// The dispatch target: the lowest-composite case below `below` that has
/// no CLOSED ticket and no active claim (lease semantics, ADR-0008).
fn pick_target(root: &Path, case: Option<&str>, below: f64) -> Result<String, String> {
    if let Some(case) = case {
        let run = root.join("evals/cases").join(case).join("run.json");
        if !run.is_file() {
            return Err(format!("no run.json for case '{case}'"));
        }
        if let Some(ticket) = ticket_for_case(root, case)
            && ticket.status == "CLOSED"
        {
            return Err(format!(
                "case '{case}' is already closed by ticket {}",
                ticket.id
            ));
        }
        return Ok(case.to_string());
    }
    let report = crate::insights::insights(root).map_err(|e| e.to_string())?;
    let candidates: Vec<&crate::insights::CaseInsight> = report
        .cases
        .iter()
        .filter(|c| {
            // A `-rerun` dir is the OUTPUT of a rerun, not a dispatchable
            // source; it is tracked via the parent case's rerun state.
            !is_rerun_case(&c.case) && c.composite < below
        })
        .collect();
    // Repair-aware ordering (cycle-33 finding, GGC #60): blind retries
    // reproduce semantic failures (~78% of errors), so a dispatch should
    // prefer cases a rerun can actually fix (Mechanical / Spinning) over
    // cases whose clean-but-wrong result a retry would only repeat
    // (Semantic). Lower priority value = dispatch first. Priorities are
    // precomputed ONCE (not inside the comparator) so run.json is read
    // once per case and the sort is stable under concurrent edits.
    let max_repeated = crate::config::Config::load(root).max_repeated_steps;
    let mut ranked: Vec<(u8, f64, &crate::insights::CaseInsight)> = candidates
        .iter()
        .map(|c| {
            let signal =
                std::fs::read_to_string(root.join("evals/cases").join(&c.case).join("run.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<eval::Run>(&text).ok())
                    .map(|run| eval::repair_signal(&run, max_repeated));
            let priority = match signal {
                Some(eval::RepairSignal::Mechanical | eval::RepairSignal::Spinning) => 0,
                Some(eval::RepairSignal::Semantic) => 1,
                _ => 2,
            };
            (priority, c.composite, *c)
        })
        .collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    let candidates: Vec<&crate::insights::CaseInsight> =
        ranked.into_iter().map(|(_, _, c)| c).collect();
    // Precompute the rerun-derived state ONCE per case (cycle-33 review
    // F1): best_composite + attempts each scan/re-score the cases dir, so
    // doing them per candidate inside the loop is O(C·(D+R·score)).
    // A single pass fills the map; the loop only reads it.
    let cfg = crate::config::Config::load(root);
    let mut state: std::collections::HashMap<String, (Option<f64>, usize)> =
        std::collections::HashMap::new();
    for c in &candidates {
        let rerun = rerun_composite(root, &c.case);
        let best = match (c.composite, rerun) {
            (o, Some(r)) => Some(o.max(r)),
            (o, None) => Some(o),
        };
        let attempts = 1 + count_reruns(root, &c.case);
        state.insert(c.case.clone(), (best, attempts));
    }
    for candidate in candidates {
        let (best, attempts) = state.get(&candidate.case).copied().unwrap_or((None, 1));
        if best.is_some_and(|b| b >= below) {
            continue;
        }
        // Bounded-retry abstention (CRC #69 + GGC #60): skip cases past
        // their rerun bound with best below the target — they need a
        // human decision, not another dispatch.
        if cfg.max_rerun_attempts.is_some_and(|limit| attempts > limit) {
            continue;
        }
        let Some(ticket) = ticket_for_case(root, &candidate.case) else {
            return Ok(candidate.case.clone());
        };
        if ticket.status == "CLOSED" || claimant_for(root, &ticket.id).is_some() {
            continue;
        }
        return Ok(candidate.case.clone());
    }
    Err(
        "no case below the target is dispatchable (all have closed tickets or active claims)"
            .into(),
    )
}

/// Write the implementation slice for a case next to its ticket.
///
/// # Errors
///
/// Returns the underlying filesystem error.
fn write_spec(root: &Path, case: &str, ticket_id: &str) -> io::Result<PathBuf> {
    let run_path = root.join("evals/cases").join(case).join("run.json");
    let run: eval::Run = serde_json::from_str(&fs::read_to_string(&run_path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let spec_dir = root.join("artifacts").join(ticket_id);
    fs::create_dir_all(&spec_dir)?;
    let path = spec_dir.join("spec.md");
    let mut body = String::new();
    let w = |b: &mut String, s: &str| b.write_str(s).map_err(io::Error::other);
    w(
        &mut body,
        &format!("# SLICE SPEC — {ticket_id} (case: {case})\n\n"),
    )?;
    w(
        &mut body,
        "- source: `mini-agi loop dispatch` (Phase 6.4, no human routing)\n",
    )?;
    w(&mut body, &format!("- goal: {}\n", run.goal))?;
    w(
        &mut body,
        &format!(
            "- scope: {}\n",
            run.scope
                .iter()
                .map(|s| format!("`{s}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )?;
    if let Some(golden) = &run.golden {
        w(&mut body, &format!("- golden: {golden}\n"))?;
    }
    match (&run.verify_command, &run.verify_target) {
        (Some(vc), vt) => {
            let vt = vt.as_deref().unwrap_or("<repo root>");
            w(&mut body, &format!("- verify_command: {vc} in {vt}\n"))?;
        }
        (None, _) => w(
            &mut body,
            "- verify_command: (none declared — caller MUST pass\n  --verify/--target to `mini-agi codex`, which refuses trust-only runs)\n",
        )?,
    }
    w(
        &mut body,
        "\n## Acceptance (measured by `mini-agi loop verify`)\n\n",
    )?;
    w(
        &mut body,
        &format!(
            "1. composite >= {} on the rerun case `{case}-rerun`\n",
            crate::config::Config::target_composite_for(root)
        ),
    )?;
    w(
        &mut body,
        "2. `outcome.achieved` and all outcome gates true (per run.json outcome)\n",
    )?;
    w(
        &mut body,
        "3. `mini-agi run failures` on the rerun: no repeated failing actions\n",
    )?;
    // Hard per-run budget gate (production-readiness E): the caps the
    // loop will enforce at verify time are declared here so the worker
    // knows them.
    let cfg = crate::config::Config::load(root);
    let mut budget_line = String::from("- budget:");
    match (cfg.max_tokens, cfg.max_cost_usd) {
        (Some(t), Some(c)) => {
            let _ = write!(budget_line, " max {t} tokens / ${c:.2} cost (hard caps)");
        }
        (Some(t), None) => {
            let _ = write!(budget_line, " max {t} tokens (hard cap)");
        }
        (None, Some(c)) => {
            let _ = write!(budget_line, " max ${c:.2} cost (hard cap)");
        }
        (None, None) => budget_line.push_str(" none configured"),
    }
    budget_line.push('\n');
    w(&mut body, &budget_line)?;
    // Red-team signal (VERIFIABLE-REWARD-RESEARCH D): a case with a
    // recent verifier-vs-judge disagreement must warn the worker —
    // the judged outcome previously overstated success here.
    if crate::verifier::disagreement_cases(root)
        .iter()
        .any(|c| c == case || c == case.strip_suffix("-rerun").unwrap_or(case))
    {
        w(
            &mut body,
            "  [warn] red-team: a prior run on this case DISAGREED with the\n  verifier — investigate before trusting the judged outcome\n",
        )?;
    }
    // Repair gate (cycle-33 finding, GGC #60): classify the prior run's
    // failure mode so a fresh session does not blindly repeat it. A
    // SEMANTIC failure (clean steps, unachieved outcome) is
    // executable-but-wrong — a blind retry reproduces ~78% of these;
    // the worker must change approach, not resubmit the same plan. A
    // SPINNING run must not be repeated verbatim.
    if let Ok(prior) =
        serde_json::from_str::<eval::Run>(&fs::read_to_string(&run_path).unwrap_or_default())
    {
        let max_repeated = crate::config::Config::load(root).max_repeated_steps;
        // Spinning is injected first and can combine with the
        // mechanical/semantic directive below (a run can both spin and
        // fail a gate — both are actionable for the fresh session).
        if eval::repair_signal(&prior, max_repeated) == eval::RepairSignal::Spinning {
            w(
                &mut body,
                "  [gate] SPINNING — the prior trajectory repeats the same\n  (tool, action) consecutively. Do NOT repeat that loop; break it.\n",
            )?;
        }
        // Mechanical vs semantic is independent of spinning: a run can
        // spin AND fail a gate. Check the step-level signal directly so
        // a spinning+mechanical run gets both directives.
        let has_mechanical = prior.trajectory.iter().any(|s| {
            (s.ok == Some(false) && eval::is_gate_failure(&prior, s))
                || s.goal_aligned == Some(false)
                || s.reverted
        });
        if has_mechanical {
            w(
                &mut body,
                "  [gate] mechanical failure (gate/goal/revert on a step) —\n  target the failing step; a corrected retry is expected to help.\n",
            )?;
        } else if eval::repair_signal(&prior, max_repeated) == eval::RepairSignal::Semantic {
            w(
                &mut body,
                "  [gate] SEMANTIC failure — steps are clean but the outcome is not\n  achieved: the prior solution is executable-but-wrong. Do NOT resubmit\n  the same plan; change the approach and verify the corrected behavior.\n",
            )?;
        }
    }
    w(
        &mut body,
        "4. target repo `verify.sh` ALL GREEN (where applicable)\n",
    )?;
    w(
        &mut body,
        "\n## Implementation discipline (fresh session)\n\n",
    )?;
    w(
        &mut body,
        "- Plan first, tests first, then implement — never repeat a failing action.\n",
    )?;
    w(
        &mut body,
        "- Read `memory/derived/failures.md` (do not repeat) and\n  `memory/derived/mismatches.md` (match the golden step shape) before starting.\n",
    )?;
    w(
        &mut body,
        &format!(
            "- Record the run truthfully as `evals/cases/{case}-rerun/run.json`\n  (goal/scope identical to the original case; write/edit steps carry\n  their `paths` inside scope).\n"
        ),
    )?;
    w(
        &mut body,
        &format!("- Then run: `mini-agi run ingest`, `mini-agi loop verify {case}-rerun`.\n"),
    )?;
    // Reflexion context (Phase 8 slice 2): top-K recorded failures for
    // this case, with verbal reflections and MAST classifications — a
    // fresh session starts knowing what went wrong and how to avoid it.
    let register = crate::failure::read_register(root).unwrap_or_default();
    let mut related: Vec<_> = register
        .iter()
        .filter(|e| e.case == case || e.case == format!("{case}-rerun"))
        .collect();
    related.sort_by_key(|e| std::cmp::Reverse(e.count));
    related.truncate(3);
    if !related.is_empty() {
        w(
            &mut body,
            "\n## Failure context (Reflexion — do not repeat)\n\n",
        )?;
        for e in related {
            w(
                &mut body,
                &format!(
                    "- `{}` tool={} action=\"{}\" count={} steps={:?} case={}\n",
                    e.hash, e.tool, e.action, e.count, e.steps, e.case
                ),
            )?;
            if let Some(mast) = &e.mast {
                w(&mut body, &format!("  mast: {mast}\n"))?;
            }
            if let Some(refl) = &e.reflection {
                w(&mut body, &format!("  reflection: {refl}\n"))?;
            }
        }
    }
    fs::write(&path, body)?;
    Ok(path)
}

/// Dispatch result.
#[derive(Debug)]
pub struct DispatchOutcome {
    /// Case dispatched.
    pub case: String,
    /// Ticket id claimed for the case.
    pub ticket: String,
    /// Path of the written slice spec.
    pub spec: PathBuf,
    /// Whether the ticket was created by this dispatch.
    pub ticket_created: bool,
}

/// Objective plan result (hardening audit P2-11 / C.11).
///
/// A bounded batch of dispatches under a shared budget with a global
/// stop — the gated `BabyAGI` task-list-as-plan (per-step
/// re-prioritization rejected).
#[derive(Debug)]
pub struct ObjectiveOutcome {
    /// Cases actually dispatched.
    pub dispatched: Vec<DispatchOutcome>,
    /// Cases skipped: no verifier declared (P0-3).
    pub skipped_no_verifier: Vec<String>,
    /// Cases skipped: blocked by an open ticket (ADR-0008 work graph).
    pub skipped_blocked: Vec<String>,
    /// Cases skipped: run.json unreadable.
    pub skipped_unavailable: Vec<String>,
    /// Cases skipped: rerun bound exhausted with best below the target
    /// (cycle-33 abstention — needs a human decision, not a retry).
    pub skipped_exhausted: Vec<String>,
    /// Cost budget in USD (None = unlimited).
    pub budget_cost: Option<f64>,
    /// Accumulated declared cost of the dispatched cases.
    pub budget_spent: f64,
}

/// `loop objective`: dispatch the worst `max_cases` open gaps that are
/// verifiable (P0-3), unclaimed, and unblocked (ADR-0008), stopping when
/// `max_cases` is reached or the cost budget is spent.
///
/// # Errors
///
/// Returns a message when status cannot be read or a dispatch fails.
pub fn objective(
    root: &Path,
    max_cases: usize,
    claimant: &str,
    budget_cost: Option<f64>,
) -> Result<ObjectiveOutcome, String> {
    let target = crate::config::Config::target_composite_for(root);
    let report = crate::insights::insights(root).map_err(|e| e.to_string())?;
    let mut candidates: Vec<&crate::insights::CaseInsight> = report
        .cases
        .iter()
        .filter(|c| {
            // A `-rerun` dir is the OUTPUT of a rerun, not a dispatchable
            // source; it is tracked via the parent case's rerun state.
            !is_rerun_case(&c.case) && c.composite < target
        })
        .collect();
    candidates.sort_by(|a, b| a.composite.total_cmp(&b.composite));
    let mut out = ObjectiveOutcome {
        dispatched: Vec::new(),
        skipped_no_verifier: Vec::new(),
        skipped_blocked: Vec::new(),
        skipped_unavailable: Vec::new(),
        skipped_exhausted: Vec::new(),
        budget_cost,
        budget_spent: 0.0,
    };
    // Precompute rerun-derived state once per candidate (cycle-33 review
    // F1): best_composite + count_reruns each scan/re-score the cases
    // dir; doing them per candidate inside the loop is O(C·(D+R·score)).
    let cfg = crate::config::Config::load(root);
    let mut state: std::collections::HashMap<String, (Option<f64>, usize)> =
        std::collections::HashMap::new();
    for c in &candidates {
        let rerun = rerun_composite(root, &c.case);
        let best = match (c.composite, rerun) {
            (o, Some(r)) => Some(o.max(r)),
            (o, None) => Some(o),
        };
        let attempts = 1 + count_reruns(root, &c.case);
        state.insert(c.case.clone(), (best, attempts));
    }
    for candidate in candidates {
        if out.dispatched.len() >= max_cases {
            break;
        }
        if let Some(budget) = budget_cost
            && out.budget_spent >= budget
        {
            break;
        }
        let case = &candidate.case;
        let (best, attempts) = state.get(case).copied().unwrap_or((None, 1));
        // Best-result tracking (SQLQE #19): if the best result seen so
        // far — original or any rerun — already reaches the target, the
        // gap is effectively closed; do not re-dispatch a case whose best
        // cannot be improved by another retry.
        if best.is_some_and(|b| b >= target) {
            continue;
        }
        // Bounded-retry abstention (CRC #69 + GGC #60): a case past its
        // rerun bound with best still below the target is EXHAUSTED —
        // further retries only burn budget; it needs a human decision,
        // not another dispatch.
        if cfg.max_rerun_attempts.is_some_and(|limit| attempts > limit) {
            out.skipped_exhausted.push(case.clone());
            continue;
        }
        let run_path = root.join("evals/cases").join(case).join("run.json");
        let Ok(run) =
            serde_json::from_str::<eval::Run>(&fs::read_to_string(&run_path).unwrap_or_default())
        else {
            out.skipped_unavailable.push(case.clone());
            continue;
        };
        // P0-3: no-dispatch-without-verifier.
        if run.verify_command.is_none() {
            out.skipped_no_verifier.push(case.clone());
            continue;
        }
        if let Some(t) = ticket_for_case(root, case) {
            if t.status == "CLOSED" || claimant_for(root, &t.id).is_some() {
                continue;
            }
            if ticket_is_blocked_by_open(root, &t) {
                out.skipped_blocked.push(case.clone());
                continue;
            }
        }
        let d = dispatch(root, Some(case), target, claimant)?;
        out.budget_spent += run.cost_usd;
        out.dispatched.push(d);
    }
    Ok(out)
}

/// ADR-0008: is `ticket` blocked by an open (not CLOSED) ticket it
/// depends on? A blocker in progress still blocks the dependent.
fn ticket_is_blocked_by_open(root: &Path, ticket: &crate::ticket::Ticket) -> bool {
    ticket
        .blocked_by
        .iter()
        .any(|dep| crate::ticket::find_ticket(root, dep).is_ok_and(|dt| dt.status != "CLOSED"))
}

/// `loop dispatch`: pick the worst open case, ensure its ticket, claim it
/// (lease), and write the slice spec.
///
/// # Errors
///
/// Returns a message when no case is dispatchable or a lease is held by
/// someone else.
pub fn dispatch(
    root: &Path,
    case: Option<&str>,
    below: f64,
    claimant: &str,
) -> Result<DispatchOutcome, String> {
    let case = pick_target(root, case, below)?;
    // P0-3 (hardening audit C.3): no trust-only worker runs. The spec
    // embeds the case's verify_command when one exists; when it does not
    // (historical frozen fixtures predate the verifier), the caller MUST
    // pass --verify/--target to `mini-agi codex` — which refuses to
    // execute without a verifier. The enforcement boundary is the worker
    // run, not the gap case.
    let existing = ticket_for_case(root, &case);
    let (ticket_id, ticket_created) = if let Some(t) = existing {
        (t.id, false)
    } else {
        let id = create_case_ticket(root, &case)?;
        (id, true)
    };
    ticket::claim_ticket(root, &ticket_id, claimant, false)
        .map_err(|e| format!("cannot claim {ticket_id}: {e}"))?;
    let spec = write_spec(root, &case, &ticket_id).map_err(|e| e.to_string())?;
    Ok(DispatchOutcome {
        case,
        ticket: ticket_id,
        spec,
        ticket_created,
    })
}

fn create_case_ticket(root: &Path, case: &str) -> Result<String, String> {
    let dir = ticket::tickets_dir(root);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _lock = ticket::lock_claims(root).map_err(|e| e.to_string())?;
    let existing = ticket::list_tickets(root).unwrap_or_default();
    let next_number = existing
        .iter()
        .filter_map(|t| {
            t.id.strip_prefix("TICKET-")
                .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|d| d.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1;
    let id = format!("TICKET-{next_number}");
    let body = format!(
        "# Ticket\n\n- id: {id}\n- title: Fix capability gap: {case} scores below the loop target\n- goal (one sentence): Bring {case} composite above {TARGET_COMPOSITE} by fixing the failing run.\n- scope: evals/cases\n- domain: eval\n"
    );
    fs::write(dir.join(format!("{id}.md")), body).map_err(|e| e.to_string())?;
    Ok(id)
}

/// `loop verify`: score + ingest a rerun case; at/above the target the
/// claim on the underlying ticket is released and the gap reports closed.
///
/// # Errors
///
/// `loop verify`: score + ingest a rerun; at/above the target (with the
/// verifier passing and zero gate regressions) the claim is released and
/// the gap reports closed. Returns `(text, closed)` so callers can exit
/// non-zero on an open gap (codex review finding).
///
/// # Errors
///
/// On a verified close, consolidate a canonical contrast fact pairing the
/// failure reflection (from the register) with the verified success
/// evidence — future runs condition on the contrast, not just the
/// failure lesson. Returns `true` (the close stands); a persistence
/// failure is reported as a warning line, never a silent drop.
fn record_close_evidence(
    root: &Path,
    base: &str,
    case: &str,
    report: &crate::eval::ScoreReport,
    verification: Option<&crate::verifier::Verification>,
    lines: &mut Vec<String>,
) -> bool {
    let failure_reflection = crate::failure::read_register(root)
        .unwrap_or_default()
        .iter()
        .find(|e| e.case == base)
        .and_then(|e| e.reflection.clone())
        .unwrap_or_else(|| "none recorded".to_string());
    // The success-evidence phrase must reflect the ACTUAL trust path
    // (codex review): a verifier pass says "deterministic gate passed";
    // an --allow-unverified close says "explicit trust".
    let trust_path = match verification.map(|v| v.status.as_str()) {
        Some("verified") => "deterministic gate passed",
        _ => "explicit trust (no deterministic verifier)",
    };
    let contrast = format!(
        "FACT: gap {base} closed by rerun {case} (composite {:.4}, verifier {}) — failure reflection: {failure_reflection} — success evidence: {trust_path}",
        report.composite,
        verification.map_or_else(|| "none".to_string(), |v| v.status.clone())
    );
    match crate::memory::consolidate(
        root,
        &contrast,
        &format!("loop-verify-{case}"),
        &crate::memory::ConsolidateOptions {
            domain: "eval".into(),
            require_signoff: false,
            dry_run: false,
        },
    ) {
        Ok(_) => {}
        Err(e) => lines.push(format!("  warning: contrast fact not persisted — {e}")),
    }
    true
}

/// Error-budget audit (cycle-33 Flat Score #98 pattern): surface the
/// per-channel failure counts and the success-at-budget projection, plus
/// the repetition watchdog (hardening audit P1-5) signal for a spinning
/// worker — so the end-of-run composite cannot hide degraded reliability.
fn verify_error_budget_audit(
    root: &Path,
    run_path: &Path,
    report: &crate::eval::ScoreReport,
    lines: &mut Vec<String>,
) {
    let audit = &report.error_budget;
    let budget_line: Vec<String> = audit
        .success_at_budget
        .iter()
        .enumerate()
        .map(|(k, ok)| format!("{k}:{}", if *ok { "ok" } else { "fail" }))
        .collect();
    lines.push(format!(
        "  error budget: {} steps, {} failed (dedup), {} gate-fail, {} goal-drift, {} reverted (by tool: {:?})",
        audit.total_steps,
        audit.failed_steps,
        audit.failed_gate_steps,
        audit.goal_drift_steps,
        audit.reverted_steps,
        audit.failed_by_tool
    ));
    lines.push(format!(
        "  success at budget k (k: status): {}",
        budget_line.join(" ")
    ));
    // Repetition watchdog: a trajectory repeating the same (tool,
    // action) verbatim beyond max_repeated_steps is a spinning worker —
    // a signal, not a hard block (repeated probes can be legitimate).
    if let Some(max) = crate::config::Config::load(root).max_repeated_steps
        && let Some(run) = serde_json::from_str::<eval::Run>(
            &std::fs::read_to_string(run_path).unwrap_or_default(),
        )
        .ok()
        && eval::max_consecutive_repeat(&run) > max
    {
        let repeats = eval::max_consecutive_repeat(&run);
        lines.push(format!(
            "  warning: repetition watchdog — {repeats} identical consecutive steps (max {max}); the worker may have spun"
        ));
    }
}

/// Close (or refuse to close) a gap by verifying a rerun against the
/// case's deterministic verifier and the eval gate.
///
/// # Errors
///
/// Returns a message when the case cannot be scored or ingested.
pub fn verify(
    root: &Path,
    case: &str,
    claimant: &str,
    allow_unverified: bool,
) -> Result<(String, bool), String> {
    let base = case.strip_suffix("-rerun").unwrap_or(case);
    let run_path = root.join("evals/cases").join(case).join("run.json");
    let report = eval::score_run(&run_path, root, &root.join("evals/golden"))
        .map_err(|e| format!("cannot score {case}: {e}"))?;
    let mut lines = vec![
        format!("verify {case}: composite {:.4}", report.composite),
        format!("  outcome: {}", report.dims.outcome),
        format!(
            "  mismatches vs golden: {}",
            report.tool_mismatches_vs_golden
        ),
    ];
    // Error-budget audit (cycle-33 Flat Score #98 pattern) + repetition
    // watchdog (hardening audit P1-5): surface per-channel failure
    // counts, the success-at-budget projection, and a spinning-worker
    // signal so the composite cannot hide degraded reliability.
    verify_error_budget_audit(root, &run_path, &report, &mut lines);
    // Verifiable reward layer (ADR-0011): when the run declares a
    // deterministic verifier, CLOSED requires it to pass — a self-
    // reported outcome is not trusted. A verifier ERROR (e.g. missing
    // target repo) also blocks close (codex review finding).
    let mut verified = true;
    // Judge-abstention gate (cycle-33 findings, CRC #69 + Flat Score
    // #98): close must not trust a judged outcome when the
    // verifier-vs-judge calibration says the judge overstates success.
    // `judge_drift` accumulates disagreements; when precision drops below
    // the configured minimum (default 1.0 = any disagreement is a
    // signal), the judged composite is not a trustworthy close input —
    // abstain (block close) until the judge is recalibrated.
    let cfg0 = crate::config::Config::load(root);
    let mut judge_trusted = true;
    {
        let drift = crate::verifier::judge_drift(root);
        let min_precision = cfg0.min_judge_precision;
        if drift.total > 0 && drift.precision() < min_precision {
            judge_trusted = false;
            lines.push(format!(
                "  abstain: judge precision {:.3} below min {min_precision:.3} — close blocked (judge overstates success; recalibrate)",
                drift.precision()
            ));
        }
    }
    let verification = match crate::verifier::verify_run(root, &run_path) {
        Ok(v) => Some(v),
        Err(e) => {
            verified = false;
            lines.push(format!(
                "  deterministic verifier: ERROR — {e} (close blocked)"
            ));
            None
        }
    };
    if let Some(v) = &verification {
        if let Err(e) = crate::verifier::append_calibration(
            root,
            &crate::verifier::CalibrationRow {
                at: crate::memory::utc_now_stamp(),
                case: case.to_string(),
                status: v.status.clone(),
                claimed: v.claimed,
                composite: report.composite,
                exit: v.exit_code,
                command: v.command.clone(),
                target: v.target.clone(),
            },
        ) {
            lines.push(format!("  warning: calibration row not persisted — {e}"));
        }
        if let (Some(command), Some(target)) = (&v.command, &v.target)
            && let Err(e) = crate::verifier::append_attribution(
                root,
                &crate::verifier::VerifyAttribution {
                    at: crate::memory::utc_now_stamp(),
                    case: case.to_string(),
                    command: command.clone(),
                    target: target.clone(),
                    status: v.status.clone(),
                },
            )
        {
            lines.push(format!("  warning: attribution not persisted — {e}"));
        }
        match v.status.as_str() {
            "verified" => lines.push(format!(
                "  deterministic verifier: PASS ({})",
                v.command.as_deref().unwrap_or("")
            )),
            "disagrees" => {
                verified = false;
                lines.push(format!(
                    "  deterministic verifier: DISAGREES with the claimed outcome ({} exit {}) — judge-calibration signal",
                    v.command.as_deref().unwrap_or(""),
                    v.exit_code.map_or_else(|| "-".into(), |c| c.to_string())
                ));
            }
            _ => {
                if allow_unverified {
                    lines.push("  deterministic verifier: not declared — closing on --allow-unverified (self-reported outcome trusted explicitly)".into());
                } else {
                    verified = false;
                    lines.push("  deterministic verifier: not declared — close requires a verifier or --allow-unverified".into());
                }
            }
        }
    }
    // Evidence enters the world model only when trusted (codex review):
    // ingest AFTER verification, skipped when the verifier disagrees.
    if verified {
        match crate::insights::ingest_run(root, &run_path, None) {
            Ok(ingest) => lines.push(format!("  ingested: {} new facts", ingest.new_facts)),
            Err(e) => lines.push(format!("  warning: ingest failed — {e}")),
        }
    }
    // Best-state regression bound (Phase 8 slice 3): the gate must have
    // ZERO regressions for a close — a slice never displaces the frozen
    // suite state (RSIBench-Data 2607.25886: preserve strong checkpoints).
    let gate = crate::eval::run_gate(
        &crate::eval::score_all_cases(&root.join("evals/cases"), root, &root.join("evals/golden"))
            .map_err(|e| e.to_string())?,
        &serde_json::from_str::<Vec<crate::eval::GateEntry>>(
            &fs::read_to_string(root.join("evals/results/baseline.json"))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
        0.05,
        1,
    );
    let gate_clean = gate.failures == 0;
    if !gate_clean {
        lines.push(format!(
            "  gate regressions: {} — close blocked (best-state bound)",
            gate.failures
        ));
    }
    // Hard per-run budget gates (production-readiness E): a rerun that
    // exceeds the configured max_tokens / max_cost_usd is flagged and
    // blocks close — an unbounded rerun must not displace the frozen
    // suite. The caps are declared in the ticket spec (write_spec) and
    // enforced here at the loop seam.
    let cfg = crate::config::Config::load(root);
    let mut in_budget = true;
    if let Some(max) = cfg.max_tokens
        && report.tokens_total > max
    {
        in_budget = false;
        lines.push(format!(
            "  budget: tokens {} > max {max} — close blocked (hard budget gate)",
            report.tokens_total
        ));
    }
    if let Some(max) = cfg.max_cost_usd
        && report.cost_usd > max
    {
        in_budget = false;
        lines.push(format!(
            "  budget: cost ${:.4} > max ${max:.4} — close blocked (hard budget gate)",
            report.cost_usd
        ));
    }
    let closed = if report.composite >= crate::config::Config::target_composite_for(root)
        && verified
        && judge_trusted
        && gate_clean
        && in_budget
    {
        if let Some(ticket) = ticket_for_case(root, base) {
            if claimant_for(root, &ticket.id).as_deref() == Some(claimant) {
                ticket::release_ticket(root, &ticket.id, claimant)
                    .map_err(|e| format!("cannot release {}: {e}", ticket.id))?;
                lines.push(format!(
                    "  gap closed: {base} released claim on {} (lease handed back)",
                    ticket.id
                ));
            } else {
                lines.push(format!(
                    "  gap closed: {base} (claim on {} not held by {claimant} — left as-is)",
                    ticket.id
                ));
            }
        }
        // Compounding-Test discipline: publish the measurement. A
        // metrics write failure is reported, not silently dropped.
        if let Err(e) = append_metrics(root, case, report.composite, report.tokens_total) {
            lines.push(format!("  warning: metrics not published — {e}"));
        }
        // Reflection-diff (Phase 9 slice 3, GRSD 2607.28076) + success
        // evidence: on close, consolidate a canonical contrast fact
        // pairing the failure reflection (from the register) with the
        // verified success evidence.
        record_close_evidence(root, base, case, &report, verification.as_ref(), &mut lines)
    } else {
        lines.push(if !verified {
            "  gap open: deterministic verifier not satisfied — outcome untrusted, keep working"
                .into()
        } else if !gate_clean {
            "  gap open: gate regressions — best-state bound holds".into()
        } else if !in_budget {
            "  gap open: over budget — hard budget gate (see lines above)".into()
        } else if !judge_trusted {
            "  gap open: judge abstention — close blocked until the judge is recalibrated".into()
        } else {
            format!(
                "  gap open: composite below {} — keep working",
                crate::config::Config::target_composite_for(root)
            )
        });
        false
    };
    lines.push(format!(
        "  eval gate: {} regressions across {} cases",
        gate.failures, gate.case_count
    ));
    lines.push("  next: mini-agi derive && mini-agi provenance".to_string());
    lines.insert(
        0,
        format!("loop verify: {}", if closed { "CLOSED" } else { "OPEN" }),
    );
    // Comprehensive action log (production-readiness D.1): record the
    // loop-verify action with the claimant as principal.
    let _ = crate::audit::append_action(root, "loop-verify", claimant, case);
    Ok((lines.join("\n"), closed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .canonicalize()
            .unwrap()
    }

    /// A disposable repo with one case (`real-ticket-008-v2`, composite
    /// 0.9774), its golden, its ticket, and an empty gate baseline. Each
    /// test passes a unique tag: parallel tests share the process, and a
    /// shared dir would race.
    fn tmp_case_root(tag: &str) -> PathBuf {
        let root = env::temp_dir().join(format!("mag-loop-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repo = repo();
        fs::create_dir_all(root.join("evals/cases/real-ticket-008-v2")).unwrap();
        fs::copy(
            repo.join("evals/cases/real-ticket-008-v2/run.json"),
            root.join("evals/cases/real-ticket-008-v2/run.json"),
        )
        .unwrap();
        fs::create_dir_all(root.join("evals/golden")).unwrap();
        fs::copy(
            repo.join("evals/golden/real-ticket-compact.json"),
            root.join("evals/golden/real-ticket-compact.json"),
        )
        .unwrap();
        fs::create_dir_all(root.join("evals/results")).unwrap();
        fs::write(root.join("evals/results/baseline.json"), "[]").unwrap();
        fs::create_dir_all(root.join("tickets")).unwrap();
        fs::copy(
            repo.join("tickets/TICKET-008.md"),
            root.join("tickets/TICKET-008.md"),
        )
        .unwrap();
        root
    }

    #[test]
    fn status_lists_low_cases_with_ticket_mapping() {
        let root = repo();
        let status = status(&root).unwrap();
        assert!(status.runs >= 12);
        assert!(!status.cases.is_empty());
        assert!(status.cases[0].composite <= status.cases[1].composite);
        assert!(status.cases.iter().all(|r| r.composite < TARGET_COMPOSITE));
        let r001 = status
            .cases
            .iter()
            .find(|r| r.case == "real-ticket-001-v2")
            .expect("real-ticket-001-v2 below target");
        assert_eq!(r001.ticket.as_deref(), Some("TICKET-001-v2"));
    }

    #[test]
    fn dispatch_writes_spec_and_claims_in_tmp_repo() {
        let root = tmp_case_root("dispatch");
        let claimant = "loop-test";
        let outcome = dispatch(&root, Some("real-ticket-008-v2"), 0.5, claimant)
            .expect("dispatch real-ticket-008-v2");
        assert!(!outcome.ticket_created);
        assert_eq!(outcome.ticket, "TICKET-008-v2");
        assert!(outcome.spec.is_file());
        let spec_text = fs::read_to_string(&outcome.spec).unwrap();
        assert!(spec_text.contains("real-ticket-008-v2-rerun"));
        assert!(spec_text.contains("composite >= 0.5"));
        let claims = ticket::read_claims(&root).unwrap();
        assert!(
            claims
                .iter()
                .any(|c| c.ticket == "TICKET-008-v2" && c.claimant == claimant)
        );
        ticket::release_ticket(&root, "TICKET-008-v2", claimant).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dispatch_spec_flags_missing_verifier() {
        // P0-3 (hardening audit C.3): a case without a verifier must be
        // flagged in the spec so the caller knows to supply
        // --verify/--target; `cmd_codex` enforces the refusal.
        let root = tmp_case_root("no-verifier");
        let nover = root.join("evals/cases/scratch-noverifier");
        fs::create_dir_all(&nover).unwrap();
        fs::write(
            nover.join("run.json"),
            r#"{"goal":"x","scope":[],"outcome":{"achieved":false},"tokens_total":1,"cost_usd":0.0,"golden":null,"trajectory":[]}"#,
        )
        .unwrap();
        let outcome = dispatch(&root, Some("scratch-noverifier"), 0.5, "loop-test")
            .expect("dispatch proceeds; the spec flags the missing verifier");
        let spec_text = fs::read_to_string(&outcome.spec).unwrap();
        assert!(
            spec_text.contains("caller MUST pass"),
            "spec must flag the missing verifier: {spec_text}"
        );
        // A verifiable case embeds its verify_command in the spec.
        let root2 = tmp_case_root("with-verifier");
        let outcome2 = dispatch(&root2, Some("real-ticket-008-v2"), 0.5, "loop-test").unwrap();
        let spec2 = fs::read_to_string(&outcome2.spec).unwrap();
        assert!(
            spec2.contains("verify_command:"),
            "spec must embed the declared verifier: {spec2}"
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&root2);
    }

    #[test]
    fn dispatch_spec_injects_repair_gate_for_semantic_failure() {
        let root = tmp_case_root("repair-gate");
        // Overwrite the run with a SEMANTIC failure: clean steps but an
        // unachieved outcome — the GGC #60 case where blind retry
        // reproduces the executable-but-wrong result.
        let run_path = root.join("evals/cases/real-ticket-008-v2/run.json");
        let mut run: eval::Run =
            serde_json::from_str(&fs::read_to_string(&run_path).unwrap()).unwrap();
        run.outcome.achieved = false;
        for s in &mut run.trajectory {
            s.ok = Some(true);
            s.goal_aligned = Some(true);
            s.reverted = false;
        }
        fs::write(&run_path, serde_json::to_string(&run).unwrap()).unwrap();
        let outcome = dispatch(&root, Some("real-ticket-008-v2"), 0.5, "loop-test").unwrap();
        let spec = fs::read_to_string(&outcome.spec).unwrap();
        assert!(
            spec.contains("[gate] SEMANTIC failure"),
            "spec must warn the worker not to blindly retry: {spec}"
        );
        assert!(
            spec.contains("Do NOT resubmit"),
            "spec must direct a procedure change: {spec}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dispatch_spec_injects_both_spinning_and_mechanical_directives() {
        let root = tmp_case_root("repair-gate-both");
        // Enable the repetition watchdog so the Spinning branch can fire.
        fs::write(root.join(".miniagi.json"), r#"{"max_repeated_steps": 2}"#).unwrap();
        let run_path = root.join("evals/cases/real-ticket-008-v2/run.json");
        let mut run: eval::Run =
            serde_json::from_str(&fs::read_to_string(&run_path).unwrap()).unwrap();
        // A run that both spins (repeats the same tool+action 3x) and
        // fails a gate: both directives must reach the fresh session.
        run.outcome.achieved = false;
        for s in &mut run.trajectory {
            s.tool = "exec".into();
            s.action = "make verify".into();
            s.ok = Some(true);
            s.goal_aligned = Some(true);
            s.reverted = false;
        }
        run.trajectory[0].ok = Some(false);
        fs::write(&run_path, serde_json::to_string(&run).unwrap()).unwrap();
        let outcome = dispatch(&root, Some("real-ticket-008-v2"), 0.5, "loop-test").unwrap();
        let spec = fs::read_to_string(&outcome.spec).unwrap();
        assert!(spec.contains("SPINNING"), "spec must flag the loop: {spec}");
        assert!(
            spec.contains("mechanical failure"),
            "spec must also flag the gate failure: {spec}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_releases_claim_only_at_target_and_keeps_it_below() {
        // Passing rerun: real-ticket-008-v2-rerun carries the 0.9774
        // fixture run -> the claim on TICKET-008 is released.
        let root = tmp_case_root("verify");
        let rerun = root.join("evals/cases/real-ticket-008-v2-rerun");
        fs::create_dir_all(&rerun).unwrap();
        fs::copy(
            repo().join("evals/cases/real-ticket-008-v2/run.json"),
            rerun.join("run.json"),
        )
        .unwrap();
        let claimant = "loop-verify";
        ticket::claim_ticket(&root, "TICKET-008-v2", claimant, true).unwrap();
        let (text, _) = verify(&root, "real-ticket-008-v2-rerun", claimant, true).unwrap();
        assert!(text.contains("CLOSED"), "{text}");
        assert!(
            ticket::read_claims(&root).unwrap().is_empty(),
            "claim must be released at the target"
        );
        let _ = fs::remove_dir_all(&root);

        // Unverified close refusal (Phase 9 trust enforcement): no
        // verifier declared and no --allow-unverified -> gap stays OPEN.
        let root = tmp_case_root("verify-refuse");
        let rerun = root.join("evals/cases/real-ticket-008-v2-rerun");
        fs::create_dir_all(&rerun).unwrap();
        fs::copy(
            repo().join("evals/cases/real-ticket-008-v2/run.json"),
            rerun.join("run.json"),
        )
        .unwrap();
        ticket::claim_ticket(&root, "TICKET-008-v2", claimant, true).unwrap();
        let (text, _) = verify(&root, "real-ticket-008-v2-rerun", claimant, false).unwrap();
        assert!(text.contains("OPEN"), "{text}");
        assert!(text.contains("--allow-unverified"), "{text}");
        let _ = fs::remove_dir_all(&root);

        // Failing rerun: a weak run (composite 0) below the target keeps
        // the claim held.
        let root = tmp_case_root("verify-open");
        let weak = root.join("evals/cases/real-ticket-008-v2-rerun");
        fs::create_dir_all(&weak).unwrap();
        fs::write(
            weak.join("run.json"),
            r#"{"goal":"TICKET-008","scope":["x"],"outcome":{"achieved":false},"tokens_total":1,"cost_usd":0.01,"golden":null,"trajectory":[{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        ticket::claim_ticket(&root, "TICKET-008-v2", claimant, true).unwrap();
        let (text, _) = verify(&root, "real-ticket-008-v2-rerun", claimant, false).unwrap();
        assert!(text.contains("OPEN"), "{text}");
        assert!(
            ticket::read_claims(&root)
                .unwrap()
                .iter()
                .any(|c| c.ticket == "TICKET-008-v2" && c.claimant == claimant),
            "claim must stay held below the target"
        );
        let _ = fs::remove_dir_all(&root);

        // Verifier disagreement: composite above target but the declared
        // gate fails -> the gap stays OPEN (outcome untrusted, ADR-0011).
        let root = tmp_case_root("verify-disagree");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("fail.sh"), "#!/bin/sh\nexit 1\n").unwrap();
        let rerun = root.join("evals/cases/real-ticket-008-v2-rerun");
        fs::create_dir_all(&rerun).unwrap();
        fs::write(
            rerun.join("run.json"),
            format!(
                r#"{{"goal":"TICKET-008","scope":["x"],"outcome":{{"achieved":true}},"tokens_total":1,"cost_usd":0.01,"golden":null,"verify_command":"sh fail.sh","verify_target":{},"trajectory":[{{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}}]}}"#,
                serde_json::to_string(&target.to_string_lossy()).unwrap()
            ),
        )
        .unwrap();
        ticket::claim_ticket(&root, "TICKET-008-v2", claimant, true).unwrap();
        let (text, _) = verify(&root, "real-ticket-008-v2-rerun", claimant, false).unwrap();
        assert!(text.contains("OPEN"), "{text}");
        assert!(text.contains("DISAGREES"), "{text}");
        assert!(
            ticket::read_claims(&root)
                .unwrap()
                .iter()
                .any(|c| c.ticket == "TICKET-008-v2" && c.claimant == claimant),
            "claim must stay held when the verifier disagrees"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_abstains_on_low_judge_precision() {
        // CRC #69 + Flat Score #98: close must not trust a judged outcome
        // when the verifier-vs-judge calibration says the judge
        // overstates success. A passing rerun fixture, but a disagreement
        // row in calibration (precision 0 < min 1.0) -> close abstained.
        let root = tmp_case_root("verify-abstain");
        let rerun = root.join("evals/cases/real-ticket-008-v2-rerun");
        fs::create_dir_all(&rerun).unwrap();
        fs::copy(
            repo().join("evals/cases/real-ticket-008-v2/run.json"),
            rerun.join("run.json"),
        )
        .unwrap();
        crate::verifier::append_calibration(
            &root,
            &crate::verifier::CalibrationRow {
                at: "2026-08-07T00:00:00Z".into(),
                case: "case-x".into(),
                status: "disagrees".into(),
                claimed: true,
                composite: 0.9,
                exit: Some(1),
                command: Some("sh v.sh".into()),
                target: Some("/tmp/x".into()),
            },
        )
        .unwrap();
        let claimant = "loop-abstain";
        ticket::claim_ticket(&root, "TICKET-008-v2", claimant, true).unwrap();
        let (text, closed) = verify(&root, "real-ticket-008-v2-rerun", claimant, true).unwrap();
        assert!(
            !closed,
            "close must abstain when judge precision is low: {text}"
        );
        assert!(text.contains("abstain"), "{text}");
        assert!(text.contains("judge precision"), "{text}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn pick_target_reports_nothing_left_when_all_closed() {
        // Hermetic fixture: one case whose run claims achieved=true (a
        // closed case is not dispatchable). The live-repo version was
        // fragile — any real open gap (e.g. a new case) broke it.
        let root = tmp_case_root("nothing-left");
        let run_path = root.join("evals/cases/real-ticket-008-v2/run.json");
        let mut run: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&run_path).unwrap()).unwrap();
        run["outcome"]["achieved"] = serde_json::json!(true);
        fs::write(&run_path, serde_json::to_string_pretty(&run).unwrap()).unwrap();
        let err = pick_target(&root, None, 0.5).expect_err("all gaps are closed by rerun");
        assert!(err.contains("no case below the target is dispatchable"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ticket_for_case_requires_digit_boundary() {
        let root = tmp_case_root("boundary");
        // "real-ticket-008-v2" must match TICKET-008-v2-style ids, but a
        // case named "real-ticket-0089" must NOT match id TICKET-008.
        let case_ok = ticket_for_case(&root, "real-ticket-008-v2");
        assert_eq!(case_ok.map(|t| t.id), Some("TICKET-008-v2".to_string()));
        let case_no = ticket_for_case(&root, "real-ticket-0089-v2");
        assert!(case_no.is_none(), "digit boundary must not match");
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn objective_dispatches_only_verifiable_unblocked_cases() {
        // P2-11: the objective stops at max_cases and skips no-verifier
        // (P0-3) and blocked cases.
        let root = tmp_case_root("objective");
        // A verifiable low case: real-ticket-008-v2 has a verifier? No —
        // seed a scratch case WITH a verifier by copying the rerun shape.
        let scratch = root.join("evals/cases/obj-low");
        fs::create_dir_all(&scratch).unwrap();
        fs::write(
            scratch.join("run.json"),
            r#"{"goal":"x","scope":["a"],"outcome":{"achieved":false},"tokens_total":1,"cost_usd":0.05,"golden":null,"verify_command":"sh verify.sh","verify_target":"/tmp/x","trajectory":[{"step":1,"tool":"exec","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        // A no-verifier low case that must be skipped.
        let nover = root.join("evals/cases/obj-nover");
        fs::create_dir_all(&nover).unwrap();
        fs::write(
            nover.join("run.json"),
            r#"{"goal":"x","scope":[],"outcome":{"achieved":false},"tokens_total":1,"cost_usd":0.01,"golden":null,"trajectory":[{"step":1,"tool":"exec","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        // max_cases=2 -> obj-low dispatches, obj-nover is examined and
        // skipped (P0-3 no verifier).
        let plan = objective(&root, 2, "obj-test", None).unwrap();
        assert_eq!(plan.dispatched.len(), 1);
        assert_eq!(plan.dispatched[0].case, "obj-low");
        assert!(
            plan.skipped_no_verifier.iter().any(|c| c == "obj-nover"),
            "no-verifier case must be skipped (P0-3)"
        );
        // Budget stop: budget 0.01 < obj-low's 0.05 -> after spending it
        // stops. With max_cases large and only one verifiable case, it
        // dispatches anyway (budget only stops BETWEEN cases). Re-run
        // with a tiny budget and a second verifiable case to prove stop.
        let scratch2 = root.join("evals/cases/obj-mid");
        fs::create_dir_all(&scratch2).unwrap();
        fs::write(
            scratch2.join("run.json"),
            r#"{"goal":"x","scope":["b"],"outcome":{"achieved":false},"tokens_total":1,"cost_usd":0.02,"golden":null,"verify_command":"sh v.sh","verify_target":"/tmp/x","trajectory":[{"step":1,"tool":"exec","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        let plan2 = objective(&root, 10, "obj-test", Some(0.02)).unwrap();
        // Sorted worst first: obj-low (0.0) then obj-mid (0.0). After the
        // first (0.05 spent >= 0.02 budget) the budget stops the second.
        assert_eq!(plan2.dispatched.len(), 1, "budget must stop the batch");
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn verify_blocks_close_on_hard_budget_breach() {
        // Production-readiness E: a rerun over the configured
        // max_tokens / max_cost_usd must NOT close, even when the
        // composite, verifier and gate are satisfied.
        let root = tmp_case_root("budget");
        fs::write(root.join(".miniagi.json"), r#"{"max_tokens": 1000}"#).unwrap();
        let rerun = root.join("evals/cases/real-ticket-008-v2-rerun");
        fs::create_dir_all(&rerun).unwrap();
        fs::copy(
            repo().join("evals/cases/real-ticket-008-v2/run.json"),
            rerun.join("run.json"),
        )
        .unwrap();
        let (text, closed) =
            verify(&root, "real-ticket-008-v2-rerun", "budget-test", true).expect("verify runs");
        assert!(!closed, "over-budget rerun must not close:\n{text}");
        assert!(text.contains("tokens 265897 > max 1000"), "{text}");
        assert!(text.contains("hard budget gate"), "{text}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dispatch_spec_warns_on_disagreement_case() {
        // Red-team signal: a case with a prior verifier-vs-judge
        // disagreement gets a warning in its dispatch spec.
        let root = tmp_case_root("redteam");
        crate::verifier::append_calibration(
            &root,
            &crate::verifier::CalibrationRow {
                at: "2026-08-04T00:00:00Z".into(),
                case: "real-ticket-008-v2".into(),
                status: "disagrees".into(),
                claimed: true,
                composite: 0.9,
                exit: Some(1),
                command: Some("sh v.sh".into()),
                target: Some("/tmp/x".into()),
            },
        )
        .unwrap();
        let outcome = dispatch(&root, Some("real-ticket-008-v2"), 0.5, "rt-test").unwrap();
        let spec = fs::read_to_string(&outcome.spec).unwrap();
        assert!(spec.contains("red-team"), "spec must warn: {spec}");
        // A clean case has no warning.
        let root2 = tmp_case_root("clean2");
        let outcome2 = dispatch(&root2, Some("real-ticket-008-v2"), 0.5, "rt-test").unwrap();
        let spec2 = fs::read_to_string(&outcome2.spec).unwrap();
        assert!(!spec2.contains("red-team"), "no warn expected: {spec2}");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&root2);
    }

    #[test]
    fn rerun_composite_tracks_best_across_multiple_reruns() {
        let root = tmp_case_root("best-rerun");
        let cases = root.join("evals/cases");
        // Original: real-ticket-008-v2 fixture (0.9774).
        // Rerun 1: copy the same run (equal composite).
        fs::create_dir_all(cases.join("real-ticket-008-v2-rerun")).unwrap();
        fs::copy(
            repo().join("evals/cases/real-ticket-008-v2/run.json"),
            cases.join("real-ticket-008-v2-rerun/run.json"),
        )
        .unwrap();
        // Rerun 2: overwrite with an unachieved run (much lower composite)
        // — must NOT regress the tracked rerun best.
        let mut low: eval::Run = serde_json::from_str(
            &fs::read_to_string(repo().join("evals/cases/real-ticket-008-v2/run.json")).unwrap(),
        )
        .unwrap();
        low.outcome.achieved = false;
        fs::create_dir_all(cases.join("real-ticket-008-v2-rerun-2")).unwrap();
        fs::write(
            cases.join("real-ticket-008-v2-rerun-2/run.json"),
            serde_json::to_string(&low).unwrap(),
        )
        .unwrap();
        let best_rerun = rerun_composite(&root, "real-ticket-008-v2").unwrap();
        assert!(
            best_rerun >= 0.9,
            "best rerun must not regress to the low rerun-2: {best_rerun}"
        );
        // best_composite = max(original, best rerun) = original's value.
        let best = best_composite(&root, "real-ticket-008-v2").unwrap();
        assert!(
            (best - best_rerun).abs() < 1e-9,
            "original and rerun-1 share the fixture score"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn best_composite_without_reruns_is_original() {
        let root = tmp_case_root("best-original");
        let best = best_composite(&root, "real-ticket-008-v2").unwrap();
        assert!(best >= 0.9, "no rerun -> best = original fixture: {best}");
        assert!(rerun_composite(&root, "real-ticket-008-v2").is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn objective_blocks_exhausted_case_beyond_rerun_bound() {
        let root = tmp_case_root("exhausted");
        // Enable the retry bound at 1 attempt.
        fs::write(root.join(".miniagi.json"), r#"{"max_rerun_attempts": 1}"#).unwrap();
        // Build a low case (composite ~0) with a declared verifier so the
        // no-verifier skip does not mask the exhaustion check.
        let mut run: eval::Run = serde_json::from_str(
            &fs::read_to_string(repo().join("evals/cases/real-ticket-008-v2/run.json")).unwrap(),
        )
        .unwrap();
        run.goal = "low case".into();
        run.scope = vec!["scripts/".into()];
        run.golden = None;
        run.verify_command = Some("true".into());
        run.outcome.achieved = false;
        for s in &mut run.trajectory {
            s.ok = Some(true);
            s.goal_aligned = Some(true);
            s.reverted = false;
        }
        let cases = root.join("evals/cases");
        fs::create_dir_all(cases.join("lowcase")).unwrap();
        fs::write(
            cases.join("lowcase/run.json"),
            serde_json::to_string(&run).unwrap(),
        )
        .unwrap();
        // Two rerun attempts -> attempts = 3 > bound 1.
        fs::create_dir_all(cases.join("lowcase-rerun")).unwrap();
        fs::write(
            cases.join("lowcase-rerun/run.json"),
            serde_json::to_string(&run).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(cases.join("lowcase-rerun-2")).unwrap();
        fs::write(
            cases.join("lowcase-rerun-2/run.json"),
            serde_json::to_string(&run).unwrap(),
        )
        .unwrap();
        let target = crate::config::Config::target_composite_for(&root);
        let out = objective(&root, 5, "loop-test", None).unwrap();
        assert!(
            out.dispatched.is_empty(),
            "an exhausted case must not be re-dispatched: {:?}",
            out.dispatched
        );
        // pick_target also refuses.
        let picked = pick_target(&root, None, target);
        assert!(
            picked.is_err(),
            "exhausted case must not be picked: {picked:?}"
        );
        // status surfaces the exhaustion.
        let status = status(&root).unwrap();
        let row = status
            .cases
            .iter()
            .find(|r| r.case == "lowcase")
            .expect("case present");
        assert!(row.exhausted, "row must be flagged exhausted");
        assert_eq!(row.attempts, 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn is_rerun_case_recognizes_all_attempt_dirs() {
        assert!(is_rerun_case("case-rerun"));
        assert!(is_rerun_case("case-rerun-2"));
        assert!(is_rerun_case("case-rerun-10"));
        assert!(!is_rerun_case("case"));
        assert!(
            !is_rerun_case("case-rerun-x"),
            "non-numeric suffix is a case name"
        );
        assert!(!is_rerun_case("rerun"), "no case prefix");
    }

    #[test]
    fn pick_target_prefers_mechanical_over_semantic_failure() {
        let root = tmp_case_root("repair-order");
        let cases = root.join("evals/cases");
        let mut base: eval::Run = serde_json::from_str(
            &fs::read_to_string(repo().join("evals/cases/real-ticket-008-v2/run.json")).unwrap(),
        )
        .unwrap();
        base.verify_command = Some("true".into());
        base.golden = None;
        base.scope = vec!["scripts/".into()];
        base.outcome.achieved = false;
        // Semantic case: clean steps, unachieved outcome (composite ~0).
        let mut sem = base.clone();
        sem.goal = "semantic case".into();
        for s in &mut sem.trajectory {
            s.ok = Some(true);
            s.goal_aligned = Some(true);
            s.reverted = false;
        }
        fs::create_dir_all(cases.join("a-sem-case")).unwrap();
        fs::write(
            cases.join("a-sem-case/run.json"),
            serde_json::to_string(&sem).unwrap(),
        )
        .unwrap();
        // Mechanical case: one failed gate step (composite ~0 but a
        // repair can target it). Give it a higher composite so the
        // priority ordering, not the composite, decides.
        let mut mech = base;
        mech.goal = "mechanical case".into();
        for (i, s) in mech.trajectory.iter_mut().enumerate() {
            if i == 0 {
                // A genuine gate failure (ADR-0013): gate command + ok:false.
                s.action = "make verify".into();
                s.ok = Some(false);
            } else {
                s.ok = Some(true);
            }
            s.goal_aligned = Some(true);
            s.reverted = false;
        }
        fs::create_dir_all(cases.join("b-mech-case")).unwrap();
        fs::write(
            cases.join("b-mech-case/run.json"),
            serde_json::to_string(&mech).unwrap(),
        )
        .unwrap();
        let target = crate::config::Config::target_composite_for(&root);
        let picked = pick_target(&root, None, target).unwrap();
        assert_eq!(
            picked, "b-mech-case",
            "repair-aware dispatch must prefer the mechanical case over semantic"
        );
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod reflexion_tests {
    use super::*;
    use std::env;

    #[test]
    fn dispatch_spec_injects_failure_context_with_reflection_and_mast() {
        let root = env::temp_dir().join(format!("mag-refl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .canonicalize()
            .unwrap();
        fs::create_dir_all(root.join("evals/cases/reactive-loop")).unwrap();
        fs::copy(
            repo.join("evals/cases/reactive-loop/run.json"),
            root.join("evals/cases/reactive-loop/run.json"),
        )
        .unwrap();
        fs::create_dir_all(root.join("tickets")).unwrap();
        fs::write(
            root.join("tickets/TICKET-9.md"),
            "- id: TICKET-9\n- title: reactive-loop gap\n- goal: fix reactive-loop\n- scope: evals/cases\n",
        )
        .unwrap();
        // Seed the register with the reflective entry (as the real
        // register now has it).
        let entry = crate::failure::FailureEntry {
            hash: crate::hash::fact_id("edit|edit same line"),
            tool: "edit".into(),
            action: "edit same line".into(),
            count: 2,
            steps: vec![4, 6],
            case: "reactive-loop".into(),
            reflection: Some(
                "repeated the identical failing edit three times — plan before editing".into(),
            ),
            mast: Some("FM-1.3 step repetition".into()),
            verifier: None,
        };
        crate::failure::update_register(&root, std::slice::from_ref(&entry)).unwrap();
        let outcome = dispatch(&root, Some("reactive-loop"), 0.5, "refl-test").unwrap();
        let spec = fs::read_to_string(&outcome.spec).unwrap();
        assert!(
            spec.contains("## Failure context (Reflexion — do not repeat)"),
            "{spec}"
        );
        assert!(spec.contains("FM-1.3 step repetition"), "{spec}");
        assert!(spec.contains("plan before editing"), "{spec}");
        ticket::release_ticket(&root, "TICKET-9", "refl-test").unwrap();
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod reflection_diff_tests {
    use super::*;
    use std::env;

    #[test]
    fn close_writes_contrast_fact_with_failure_reflection() {
        let root = env::temp_dir().join(format!("mag-contrast-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .canonicalize()
            .unwrap();
        fs::create_dir_all(root.join("evals/cases/real-ticket-008-v2-rerun")).unwrap();
        fs::copy(
            repo.join("evals/cases/real-ticket-008-v2/run.json"),
            root.join("evals/cases/real-ticket-008-v2-rerun/run.json"),
        )
        .unwrap();
        fs::create_dir_all(root.join("evals/cases/real-ticket-008-v2")).unwrap();
        fs::copy(
            repo.join("evals/cases/real-ticket-008-v2/run.json"),
            root.join("evals/cases/real-ticket-008-v2/run.json"),
        )
        .unwrap();
        fs::create_dir_all(root.join("evals/golden")).unwrap();
        fs::copy(
            repo.join("evals/golden/real-ticket-compact.json"),
            root.join("evals/golden/real-ticket-compact.json"),
        )
        .unwrap();
        fs::create_dir_all(root.join("evals/results")).unwrap();
        fs::write(root.join("evals/results/baseline.json"), "[]").unwrap();
        fs::create_dir_all(root.join("tickets")).unwrap();
        fs::write(
            root.join("tickets/TICKET-008.md"),
            "- id: TICKET-008-v2\n- title: t\n- goal: g\n- scope: evals/cases\n",
        )
        .unwrap();
        // Seed the failure register with a reflective entry for the base.
        let entry = crate::failure::FailureEntry {
            hash: crate::hash::fact_id("exec|make verify"),
            tool: "exec".into(),
            action: "make verify".into(),
            count: 2,
            steps: vec![3, 5],
            case: "real-ticket-008-v2".into(),
            reflection: Some("ran the gate without reading the diff — check the diff first".into()),
            mast: Some("FM-3.2 no or incomplete verification".into()),
            verifier: Some("disagrees".into()),
        };
        crate::failure::update_register(&root, std::slice::from_ref(&entry)).unwrap();
        let (text, closed) = verify(&root, "real-ticket-008-v2-rerun", "contrast", true).unwrap();
        assert!(closed, "{text}");
        // The contrast fact must exist in canonical with both sides.
        let facts = crate::memory::canonical_facts(&root);
        let bodies: Vec<&str> = facts.iter().map(|(b, _)| b.as_str()).collect();
        assert!(
            bodies
                .iter()
                .any(|b| b.contains("failure reflection: ran the gate without reading the diff")),
            "{bodies:?}"
        );
        assert!(
            bodies
                .iter()
                .any(|b| b.contains("success evidence:") && b.contains("explicit trust")),
            "{bodies:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod metrics_migration_tests {
    use super::*;
    use std::env;

    #[test]
    fn old_format_metrics_migrates_to_family_columns() {
        let root = env::temp_dir().join(format!("mag-migm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(
            root.join("docs/METRICS.md"),
            "| date | case | composite | tokens |\n| --- | --- | --- | --- |\n| 2026-08-03 | codex-exp-002-rerun | 1.0000 | 18400 |\n",
        )
        .unwrap();
        append_metrics(&root, "codex-exp-002-rerun", 1.0, 18400).unwrap();
        let text = fs::read_to_string(root.join("docs/METRICS.md")).unwrap();
        assert!(text.contains("| family |"), "{text}");
        assert!(
            text.contains("| codex-exp-002-rerun | codex-exp |"),
            "{text}"
        );
        assert_eq!(
            text.matches("codex-exp-002-rerun").count(),
            1,
            "no duplicate row"
        );
    }
}

#[cfg(test)]
mod attempts_tests {
    use super::*;
    use std::env;

    #[test]
    fn attempts_reflect_rerun_presence() {
        // Real repo: real-ticket-001-v2 has a rerun (2 attempts);
        // harnessed has none (1 attempt).
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .canonicalize()
            .unwrap();
        let status = status(&root).unwrap();
        let r001 = status
            .cases
            .iter()
            .find(|r| r.case == "real-ticket-001-v2")
            .unwrap();
        assert_eq!(r001.attempts, 2);
        let harnessed = status.cases.iter().find(|r| r.case == "harnessed");
        // harnessed is above target -> not in the below-target rows; use
        // rerun_composite directly instead.
        assert!(harnessed.is_none() || harnessed.unwrap().attempts == 1);
    }
}
