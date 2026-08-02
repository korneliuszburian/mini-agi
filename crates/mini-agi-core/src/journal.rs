//! Checkpoint journal parsing and completeness audit.
//!
//! PORT of `PoC` `scripts/checkpoint-gate.py` semantics (T008 amendments):
//! every `VERIFY-PASS`/`VERIFY-FAIL` must have an earlier `BEGIN` for the
//! same label; a `STATUS` line resolves a missing-BEGIN violation ONLY if
//! the violating VERIFY predates `ACK_LEGACY_UNTIL` (legacy-only escape
//! hatch — closed from the gate boundary onward, per REVIEW-005).
//!
//! Journal lines look like:
//! `2026-08-02T10:53:31Z VERIFY-PASS TICKET-001 @ 0d0231d`
//! Other lines (e.g. `CHECKPOINT-ABORT`) are skipped by the parser.

use std::collections::BTreeMap;

/// Kinds of journal events (`PoC` checkpoint-gate `LINE` regex).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalKind {
    /// A checkpoint was opened (commit made / clean tree noted).
    Begin,
    /// The verifier passed at a checkpoint.
    VerifyPass,
    /// The verifier failed at a checkpoint.
    VerifyFail,
    /// Acknowledgment of a journal anomaly (legacy-only semantics).
    Status,
    /// Explicit close of a checkpoint.
    End,
}

impl JournalKind {
    fn parse(kind: &str) -> Option<Self> {
        match kind {
            "BEGIN" => Some(Self::Begin),
            "VERIFY-PASS" => Some(Self::VerifyPass),
            "VERIFY-FAIL" => Some(Self::VerifyFail),
            "STATUS" => Some(Self::Status),
            "END" => Some(Self::End),
            _ => None,
        }
    }
}

/// One parsed journal line.
#[derive(Debug, Clone)]
pub struct JournalEvent {
    /// ISO-8601 UTC timestamp (first token).
    pub ts: String,
    /// Event kind (second token).
    pub kind: JournalKind,
    /// Label (third token) — the checkpoint or ticket this event belongs to.
    pub label: String,
}

/// Result of the completeness audit.
#[derive(Debug, Default)]
pub struct JournalViolations {
    /// Violations from `GATE_SINCE` onward — fail the gate.
    pub bad: Vec<String>,
    /// Violations before `GATE_SINCE` — preserved evidence, not failing.
    pub historical: Vec<String>,
}

/// Gate introduction boundary (UTC): the gate's own commit time.
///
/// v3 journal-gate introduction = 2026-08-02T18:14:41Z (authoring time of
/// the committing change). Violations before it are historical (reported,
/// not failing); from it onward they fail the gate. Mirrors `PoC`
/// `checkpoint-gate.py` `GATE_SINCE` (v2: commit `601e11c`,
/// 2026-08-02T13:03:58Z).
pub const GATE_SINCE: &str = "2026-08-02T18:14:41Z";

/// Ack cutoff (UTC): a `STATUS` line resolves a missing-BEGIN violation
/// ONLY if the violating `VERIFY` is dated before this timestamp.
///
/// Equal to `GATE_SINCE` in v3 (empty journal at gate introduction) — acks
/// are disabled for this repo's lifetime unless a dated decision
/// explicitly re-enables them for a pre-cutoff defect (inherited `PoC`
/// `ADR-0003`).
pub const ACK_LEGACY_UNTIL: &str = GATE_SINCE;

/// Parse journal text into events, skipping non-event lines.
#[must_use]
pub fn parse_journal(text: &str) -> Vec<JournalEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let (Some(ts), Some(kind), Some(label)) = (
            tokens.next(),
            tokens.next().and_then(JournalKind::parse),
            tokens.next(),
        ) else {
            continue;
        };
        events.push(JournalEvent {
            ts: ts.to_string(),
            kind,
            label: label.to_string(),
        });
    }
    events
}

/// Audit parsed events; returns failing and historical violations.
///
/// Per label (sorted), every `VERIFY-PASS`/`VERIFY-FAIL` must have an
/// earlier `BEGIN`; `STATUS` can resolve only legacy (pre-cutoff)
/// violations that precede it.
#[must_use]
pub fn violations(events: &[JournalEvent], since: &str) -> JournalViolations {
    let mut grouped: BTreeMap<&str, Vec<&JournalEvent>> = BTreeMap::new();
    for event in events {
        grouped.entry(&event.label).or_default().push(event);
    }
    let mut result = JournalViolations::default();
    for (label, items) in grouped {
        let mut unresolved: Vec<(usize, &str)> = Vec::new();
        for (index, event) in items.iter().enumerate() {
            match event.kind {
                JournalKind::VerifyPass | JournalKind::VerifyFail => {
                    let has_earlier_begin = items[..index]
                        .iter()
                        .any(|e| e.kind == JournalKind::Begin && e.ts.as_str() < event.ts.as_str());
                    if !has_earlier_begin {
                        unresolved.push((index, event.ts.as_str()));
                    }
                }
                JournalKind::Status => {
                    let prior = unresolved.iter().position(|(idx, ts)| {
                        *idx < index && *ts < event.ts.as_str() && *ts < ACK_LEGACY_UNTIL
                    });
                    if let Some(pos) = prior {
                        unresolved.remove(pos);
                    }
                }
                JournalKind::Begin | JournalKind::End => {}
            }
        }
        for (_, ts) in unresolved {
            let msg = format!("{label}: VERIFY without earlier BEGIN @ {ts}");
            if ts < since {
                result.historical.push(msg);
            } else {
                result.bad.push(msg);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "2026-08-02T10:00:54Z BEGIN demo-final -> 1237758
2026-08-02T10:00:55Z VERIFY-PASS demo-final @ 1237758
2026-08-02T15:00:00Z BEGIN t002 -> 57ce2c7
2026-08-02T15:10:00Z VERIFY-PASS t002 @ 57ce2c7
";

    const VIOLATION: &str = "2026-08-02T19:00:00Z VERIFY-PASS t002 @ 57ce2c7
";

    const HISTORICAL: &str = "2026-08-02T10:53:31Z VERIFY-PASS TICKET-001 @ 0d0231d
";

    fn audit(text: &str) -> JournalViolations {
        violations(&parse_journal(text), GATE_SINCE)
    }

    #[test]
    fn begin_before_verify_passes() {
        let v = audit(GOOD);
        assert!(v.bad.is_empty());
        assert!(v.historical.is_empty());
    }

    #[test]
    fn begin_after_verify_fails() {
        let text = "2026-08-02T19:00:00Z VERIFY-PASS t002 @ 57ce2c7
2026-08-02T19:01:00Z BEGIN t002 -> 57ce2c7
";
        let v = audit(text);
        assert_eq!(v.bad.len(), 1);
        assert!(v.bad[0].contains("VERIFY without earlier BEGIN"));
        assert!(v.historical.is_empty());
    }

    #[test]
    fn verify_without_begin_fails() {
        let v = audit(VIOLATION);
        assert_eq!(v.bad.len(), 1);
        assert!(v.bad[0].contains("VERIFY without earlier BEGIN"));
        assert!(v.historical.is_empty());
    }

    #[test]
    fn status_cannot_acknowledge_post_boundary_violation() {
        let text = "2026-08-02T19:00:00Z VERIFY-PASS TICKET-002 @ 57ce2c7
2026-08-02T19:01:00Z STATUS TICKET-002 mechanism defect
";
        let v = audit(text);
        assert_eq!(v.bad.len(), 1);
        assert!(v.bad[0].contains("VERIFY without earlier BEGIN"));
        assert!(v.historical.is_empty());
    }

    #[test]
    fn legacy_status_still_resolves_pre_cutoff_violation() {
        let text = "2026-08-02T11:08:18Z VERIFY-PASS TICKET-002 @ 50af668
2026-08-02T11:12:17Z STATUS TICKET-002 mechanism defect: begin no-op
";
        let v = audit(text);
        assert!(v.bad.is_empty());
        assert!(v.historical.is_empty());
    }

    #[test]
    fn verify_begin_verify_without_ack_fails() {
        let text = "2026-08-02T19:00:00Z VERIFY-PASS TICKET-002 @ 57ce2c7
2026-08-02T19:01:00Z BEGIN TICKET-002 -> 57ce2c7
2026-08-02T19:02:00Z VERIFY-PASS TICKET-002 @ 57ce2c7
";
        let v = audit(text);
        assert_eq!(v.bad.len(), 1);
        assert!(v.bad[0].contains("19:00:00Z"));
        assert!(v.historical.is_empty());
    }

    #[test]
    fn status_before_verify_does_not_acknowledge_violation() {
        let text = "2026-08-02T19:00:00Z STATUS TICKET-002 mechanism defect
2026-08-02T19:01:00Z VERIFY-PASS TICKET-002 @ 57ce2c7
";
        let v = audit(text);
        assert_eq!(v.bad.len(), 1);
        assert!(v.historical.is_empty());
    }

    #[test]
    fn pre_gate_violation_is_historical() {
        let v = audit(HISTORICAL);
        assert!(v.bad.is_empty());
        assert_eq!(v.historical.len(), 1);
        assert!(v.historical[0].contains("TICKET-001"));
    }

    #[test]
    fn since_boundary_respects_timestamp() {
        let text = "2026-08-02T11:00:00Z VERIFY-PASS x @ abc\n";
        let v = violations(&parse_journal(text), GATE_SINCE);
        assert!(v.bad.is_empty());
        assert_eq!(v.historical.len(), 1);
        let v = violations(&parse_journal(text), "2026-08-02T10:00:00Z");
        assert_eq!(v.bad.len(), 1);
        assert!(v.historical.is_empty());
    }

    #[test]
    fn non_event_lines_are_skipped() {
        let text = "2026-08-02T15:00:00Z CHECKPOINT-ABORT step dirty paths\n
2026-08-02T15:01:00Z BEGIN step -> abc123\n
2026-08-02T15:02:00Z VERIFY-PASS step @ abc123\n";
        let v = audit(text);
        assert!(v.bad.is_empty());
        assert!(v.historical.is_empty());
    }
}
