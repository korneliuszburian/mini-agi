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
    /// 1-based line number in the journal file (awk `NR` semantics).
    pub line_no: usize,
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
    for (i, line) in text.lines().enumerate() {
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
            line_no: i + 1,
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
                    let has_earlier_begin = items[..index].iter().any(|e| {
                        e.kind == JournalKind::Begin && e.ts.as_str() <= event.ts.as_str()
                    });
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

/// One anomaly found by the line-based audit.
#[derive(Debug, Clone)]
pub struct AuditAnomaly {
    /// 1-based journal line number of the offending event.
    pub line_no: usize,
    /// Human-readable description (`PoC` `audit.sh` wording).
    pub message: String,
}

/// Result of the line-based audit.
#[derive(Debug, Default)]
pub struct AuditReport {
    /// Anomalies after the newest complete green checkpoint — fail the gate.
    pub bad: Vec<AuditAnomaly>,
    /// Anomalies before the newest complete green — warnings only.
    pub historical: Vec<AuditAnomaly>,
}

/// Line-based completeness audit (PORT of `PoC` `scripts/audit.sh`, T008
/// semantics):
///
/// - a `BEGIN` for an already-open label is an **orphan BEGIN** anomaly at
///   the earlier `BEGIN` line;
/// - a `VERIFY-PASS`/`VERIFY-FAIL` without an open `BEGIN` is a
///   **VERIFY without BEGIN** anomaly;
/// - `VERIFY-PASS` closes its `BEGIN` and advances the historical boundary
///   (newest complete green); `VERIFY-FAIL` is a terminal outcome and also
///   resolves its `BEGIN`;
/// - an unclosed `BEGIN` is an orphan anomaly **unless** it is the literal
///   last line of the journal (verification in progress — `checkpoint.sh`
///   appends `BEGIN` before running the verifier);
/// - anomalies before the newest complete green are historical (warned,
///   not failing); from it onward they fail the gate.
#[must_use]
pub fn audit_journal(events: &[JournalEvent]) -> AuditReport {
    let mut report = AuditReport::default();
    let mut anomalies: Vec<AuditAnomaly> = Vec::new();
    let mut open: BTreeMap<&str, usize> = BTreeMap::new();
    let mut newest_complete_green = 0usize;
    for event in events {
        match event.kind {
            JournalKind::Begin => {
                if let Some(earlier) = open.insert(&event.label, event.line_no) {
                    anomalies.push(AuditAnomaly {
                        line_no: earlier,
                        message: format!("orphan BEGIN: {}", event.label),
                    });
                }
            }
            JournalKind::VerifyPass => {
                if open.remove(event.label.as_str()).is_none() {
                    anomalies.push(AuditAnomaly {
                        line_no: event.line_no,
                        message: format!("VERIFY without BEGIN: {}", event.label),
                    });
                } else {
                    newest_complete_green = event.line_no;
                }
            }
            JournalKind::VerifyFail => {
                if open.remove(event.label.as_str()).is_none() {
                    anomalies.push(AuditAnomaly {
                        line_no: event.line_no,
                        message: format!("VERIFY without BEGIN: {}", event.label),
                    });
                }
            }
            JournalKind::Status | JournalKind::End => {}
        }
    }
    let last_line = events.last().map_or(0, |e| e.line_no);
    for (label, line_no) in open {
        if line_no != last_line {
            anomalies.push(AuditAnomaly {
                line_no,
                message: format!("orphan BEGIN: {label}"),
            });
        }
    }
    for anomaly in anomalies {
        if anomaly.line_no > newest_complete_green {
            report.bad.push(anomaly);
        } else {
            report.historical.push(anomaly);
        }
    }
    report
}
