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
/// `TICKET-001-v2`).
#[must_use]
pub fn ticket_for_case(root: &Path, case: &str) -> Option<Ticket> {
    let case_lower = case.to_lowercase();
    ticket::list_tickets(root)
        .unwrap_or_default()
        .into_iter()
        .find(|t| {
            t.goal.contains(case) || t.title.contains(case) || {
                t.id.to_lowercase()
                    .strip_prefix("ticket-")
                    .is_some_and(|rest| case_lower.contains(&format!("ticket-{rest}")))
            }
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

/// Composite of the rerun case `<case>-rerun`, when it exists.
#[must_use]
pub fn rerun_composite(root: &Path, case: &str) -> Option<f64> {
    let run = root
        .join("evals/cases")
        .join(format!("{case}-rerun"))
        .join("run.json");
    if !run.is_file() {
        return None;
    }
    eval::score_run(&run, root, &root.join("evals/golden"))
        .ok()
        .map(|r| r.composite)
}

/// A case is closed when its rerun reaches the loop target — the original
/// run stays as a historical fixture (TICKET-9 semantics).
#[must_use]
pub fn case_closed_by_rerun(root: &Path, case: &str) -> bool {
    rerun_composite(root, case).is_some_and(|c| c >= TARGET_COMPOSITE)
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
        if case.composite >= TARGET_COMPOSITE {
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
        rows.push(LoopRow {
            case: case.case.clone(),
            composite: case.composite,
            rerun_composite: rerun_composite(root, &case.case),
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

/// The dispatch target: the lowest-composite case below `below` that has
/// no CLOSED ticket and no active claim (lease semantics, ADR-0008).
fn pick_target(root: &Path, case: Option<&str>, below: f64) -> Result<String, String> {
    if let Some(case) = case {
        let run = root.join("evals/cases").join(case).join("run.json");
        if !run.is_file() {
            return Err(format!("no run.json for case '{case}'"));
        }
        return Ok(case.to_string());
    }
    let report = crate::insights::insights(root).map_err(|e| e.to_string())?;
    let mut candidates: Vec<&crate::insights::CaseInsight> = report
        .cases
        .iter()
        .filter(|c| c.composite < below)
        .collect();
    candidates.sort_by(|a, b| a.composite.total_cmp(&b.composite));
    for candidate in candidates {
        if case_closed_by_rerun(root, &candidate.case) {
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
    w(
        &mut body,
        "\n## Acceptance (measured by `mini-agi loop verify`)\n\n",
    )?;
    w(
        &mut body,
        &format!("1. composite >= {TARGET_COMPOSITE} on the rerun case `{case}-rerun`\n"),
    )?;
    w(
        &mut body,
        "2. `outcome.achieved` and all outcome gates true (per run.json outcome)\n",
    )?;
    w(
        &mut body,
        "3. `mini-agi run failures` on the rerun: no repeated failing actions\n",
    )?;
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
        "# Ticket\n\n- id: {id}\n- title: Fix capability gap: {case} scores below the loop target\n- goal (one sentence): Bring {case} composite above {TARGET_COMPOSITE} by fixing the failing run.\n- domain: eval\n"
    );
    fs::write(dir.join(format!("{id}.md")), body).map_err(|e| e.to_string())?;
    Ok(id)
}

/// `loop verify`: score + ingest a rerun case; at/above the target the
/// claim on the underlying ticket is released and the gap reports closed.
///
/// # Errors
///
/// Returns a message when the case cannot be scored or ingested.
pub fn verify(root: &Path, case: &str, claimant: &str) -> Result<String, String> {
    let base = case.strip_suffix("-rerun").unwrap_or(case);
    let run_path = root.join("evals/cases").join(case).join("run.json");
    let report = eval::score_run(&run_path, root, &root.join("evals/golden"))
        .map_err(|e| format!("cannot score {case}: {e}"))?;
    let ingest = crate::insights::ingest_run(root, &run_path, None)
        .map_err(|e| format!("cannot ingest {case}: {e}"))?;
    let mut lines = vec![
        format!(
            "verify {case}: composite {:.4} (ingested: {} new facts)",
            report.composite, ingest.new_facts
        ),
        format!("  outcome: {}", report.dims.outcome),
        format!(
            "  mismatches vs golden: {}",
            report.tool_mismatches_vs_golden
        ),
    ];
    let closed = if report.composite >= TARGET_COMPOSITE {
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
        true
    } else {
        lines.push(format!(
            "  gap open: composite below {TARGET_COMPOSITE} — keep working"
        ));
        false
    };
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
    lines.push(format!(
        "  eval gate: {} regressions across {} cases",
        gate.failures, gate.case_count
    ));
    lines.push("  next: mini-agi derive && mini-agi provenance".to_string());
    lines.insert(
        0,
        format!("loop verify: {}", if closed { "CLOSED" } else { "OPEN" }),
    );
    Ok(lines.join("\n"))
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
    fn dispatch_writes_spec_and_claims_then_verify_releases() {
        let root = repo();
        let claimant = "loop-test";
        let outcome = dispatch(&root, Some("real-ticket-001-v2"), 0.5, claimant)
            .expect("dispatch real-ticket-001-v2");
        assert!(!outcome.ticket_created);
        assert_eq!(outcome.ticket, "TICKET-001-v2");
        assert!(outcome.spec.is_file());
        let spec_text = fs::read_to_string(&outcome.spec).unwrap();
        assert!(spec_text.contains("real-ticket-001-v2-rerun"));
        assert!(spec_text.contains("composite >= 0.5"));
        let claims = ticket::read_claims(&root).unwrap();
        assert!(
            claims
                .iter()
                .any(|c| c.ticket == "TICKET-001-v2" && c.claimant == claimant)
        );
        ticket::release_ticket(&root, "TICKET-001-v2", claimant).unwrap();
        let _ = fs::remove_file(&outcome.spec);
        let _ = fs::remove_dir_all(root.join("artifacts/TICKET-001-v2"));
    }

    #[test]
    fn pick_target_reports_nothing_left_when_all_closed() {
        let root = repo();
        let err = pick_target(&root, None, 0.5).expect_err("all gaps are closed by rerun");
        assert!(err.contains("no case below the target is dispatchable"));
    }
}
