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
    // Verifiable reward layer (ADR-0011): when the run declares a
    // deterministic verifier, CLOSED requires it to pass — a self-
    // reported outcome is not trusted.
    let verification = crate::verifier::verify_run(root, &run_path).ok();
    let mut verified = true;
    if let Some(v) = &verification {
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
            _ => lines.push(
                "  deterministic verifier: not declared — outcome is the run's own claim".into(),
            ),
        }
    }
    let closed = if report.composite >= TARGET_COMPOSITE && verified {
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
        lines.push(if verified {
            format!("  gap open: composite below {TARGET_COMPOSITE} — keep working")
        } else {
            "  gap open: deterministic verifier disagrees — outcome untrusted, keep working".into()
        });
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
        let text = verify(&root, "real-ticket-008-v2-rerun", claimant).unwrap();
        assert!(text.contains("CLOSED"), "{text}");
        assert!(
            ticket::read_claims(&root).unwrap().is_empty(),
            "claim must be released at the target"
        );
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
        let text = verify(&root, "real-ticket-008-v2-rerun", claimant).unwrap();
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
        let text = verify(&root, "real-ticket-008-v2-rerun", claimant).unwrap();
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
    fn pick_target_reports_nothing_left_when_all_closed() {
        let root = repo();
        let err = pick_target(&root, None, 0.5).expect_err("all gaps are closed by rerun");
        assert!(err.contains("no case below the target is dispatchable"));
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
            "- id: TICKET-9\n- title: reactive-loop gap\n- goal: fix reactive-loop\n",
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
