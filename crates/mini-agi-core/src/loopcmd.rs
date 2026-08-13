//! Proactive composition loop (condensed).
//!
//! Business model: a gap is a case whose run reports `outcome.achieved:
//! false`; the loop turns it into a ticket, dispatches a worker slice,
//! and CLOSES it only when the run's declared deterministic gate passes
//! (`verify_command` run in `verify_target`). The measurement machinery
//! (composite scoring, judge calibration, registers) was removed as
//! over-verification: the gate IS the verification.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::eval::Run;

/// Composite a case must reach to leave the loop's open set (kept as a
/// constant for API compatibility; the minimal model is achieved=true).
pub const TARGET_COMPOSITE: f64 = 0.5;

/// One case's gap lifecycle state (the authoritative ledger record).
///
/// `evals/ledger/<case>.json` is written by `loopcmd` ONLY (never
/// hand-edited, never touched by ticket/worker code), atomically (temp +
/// rename) under the claims lock. Terminal states (`closed`, `exhausted`,
/// `unverifiable`) make a case permanently not dispatchable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapState {
    /// Known gap, never dispatched.
    Open,
    /// Dispatched at least once; a claim is expected.
    Dispatched,
    /// Closed by a passing gate on an achieved run (atomic close).
    Closed,
    /// Retry bound exceeded with no achieved rerun.
    Exhausted,
    /// No verifiable gate exists; never dispatchable.
    Unverifiable,
}

/// The authoritative gap lifecycle record at `evals/ledger/<case>.json`.
///
/// A base case owns its row; rerun dirs are attempt artifacts and never
/// get their own row — closing a rerun strips `-rerun-N` and closes the
/// BASE, recording the closing rerun dir in `closed_by`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    /// Base case (never a rerun dir name).
    pub case: String,
    /// Lifecycle state.
    pub state: GapState,
    /// Case that first opened the gap (the base run dir).
    pub opened_by: String,
    /// UTC stamp of the first dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_dispatched_at: Option<String>,
    /// Number of dispatches.
    #[serde(default)]
    pub attempts: usize,
    /// UTC stamp of the most recent dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempted_at: Option<String>,
    /// Closing rerun dir (base case closes via its rerun).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_by: Option<String>,
    /// UTC stamp of the atomic close.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    /// Ticket whose claim was released on close.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_ticket: Option<String>,
}

impl Default for Gap {
    fn default() -> Self {
        Self {
            case: String::new(),
            state: GapState::Open,
            opened_by: String::new(),
            first_dispatched_at: None,
            attempts: 0,
            last_attempted_at: None,
            closed_by: None,
            verified_at: None,
            closed_ticket: None,
        }
    }
}

/// Is the state terminal (never dispatchable again)?
#[must_use]
pub const fn gap_is_terminal(state: &GapState) -> bool {
    matches!(
        state,
        GapState::Closed | GapState::Exhausted | GapState::Unverifiable
    )
}

impl GapState {
    /// The serialized state name (for messages).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Dispatched => "dispatched",
            Self::Closed => "closed",
            Self::Exhausted => "exhausted",
            Self::Unverifiable => "unverifiable",
        }
    }
}

impl Gap {
    /// State name as a string (message helper).
    #[must_use]
    pub const fn state_name(&self) -> &'static str {
        self.state.name()
    }
}

/// Ledger file path for a base case.
#[must_use]
pub fn ledger_path(root: &Path, case: &str) -> PathBuf {
    root.join("evals/ledger").join(format!("{case}.json"))
}

/// Read a case's ledger row, distinguishing ABSENT from CORRUPT.
///
/// A corrupt row is a hard error (fail-closed): treating it as "never
/// dispatched" would silently reset the retry bound and make a
/// Closed/Exhausted case re-dispatchable.
fn read_ledger_checked(root: &Path, case: &str) -> Result<Option<Gap>, String> {
    let path = ledger_path(root, case);
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("ledger {} is corrupt: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read ledger {}: {e}", path.display())),
    }
}

/// Read a case's ledger row (absent = gap not yet opened). A corrupt row
/// is logged LOUDLY and treated as absent (best-effort call sites);
/// authoritative paths use [`read_ledger_checked`].
#[must_use]
pub fn read_ledger(root: &Path, case: &str) -> Option<Gap> {
    match read_ledger_checked(root, case) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("loopcmd: {e}");
            None
        }
    }
}

/// Atomically write a ledger row (temp file + rename). Callers MUST hold
/// the claims lock so the row cannot be raced by another loop writer.
///
/// # Errors
///
/// Returns an io error when the ledger directory or temp file cannot be
/// written.
pub fn write_ledger_atomic(root: &Path, gap: &Gap) -> io::Result<()> {
    let path = ledger_path(root, &gap.case);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = crate::ticket::tmp_unique(&path, "ledger");
    let json = serde_json::to_string(gap).map_err(io::Error::other)?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| std::io::Write::write_all(&mut f, json.as_bytes()))?;
    fs::rename(&tmp, &path)
}

/// Mark a case in a terminal/recorded state in the ledger (exhausted,
/// unverifiable). Best-effort under the claims lock: the state write and
/// any claim release happen in one locked section. Errors are logged to
/// stderr, not swallowed silently.
fn mark_state(root: &Path, case: &str, state: GapState) {
    let lock = match crate::ticket::lock_claims(root) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("loopcmd: cannot lock claims to mark {case} {state:?}: {e}");
            return;
        }
    };
    let mut gap = match read_ledger_checked(root, case) {
        Ok(Some(g)) => g,
        Ok(None) => Gap {
            case: case.to_string(),
            state: GapState::Open,
            opened_by: case.to_string(),
            ..Gap::default()
        },
        Err(e) => {
            eprintln!("loopcmd: {e}");
            drop(lock);
            return;
        }
    };
    // EXHAUSTED per §3.1 = claim released, ticket left OPEN with a note.
    if matches!(&state, GapState::Exhausted)
        && let Some(ticket) = ticket_for_case(root, case)
        && let Some(claimant) = claimant_for(root, &ticket.id)
        && let Err(e) = crate::ticket::release_ticket_locked(root, &ticket.id, &claimant)
    {
        // A stranded lease is worse than a failed ledger mark — say it
        // loudly (the case will be terminal, so the claim has no other
        // release path).
        eprintln!(
            "loopcmd: cannot release {} on exhausting {case}: {e}",
            ticket.id
        );
    } else if let Some(ticket) = ticket_for_case(root, case) {
        let _ = crate::ticket::append_ticket_note(
            root,
            &ticket.id,
            &format!(
                "exhausted in the ledger on {}",
                crate::memory::utc_now_date()
            ),
        );
    }
    gap.state = state;
    if let Err(e) = write_ledger_atomic(root, &gap) {
        eprintln!("loopcmd: cannot mark {case} {:?}: {e}", gap.state);
    }
    drop(lock);
}

/// Has the case exceeded the configured rerun bound (its attempts vs
/// `max_rerun_attempts`)? The ledger's attempt count is authoritative
/// when a row exists (ARCHITECTURE-CONDENSED 5.2: the bound is ENFORCED
/// at dispatch time, not merely reported by `status`).
#[must_use]
fn case_exceeded_bound(root: &Path, case: &str) -> bool {
    let Some(max) = crate::config::Config::load(root).max_rerun_attempts else {
        return false;
    };
    let attempts = read_ledger(root, case).map_or(0, |g| g.attempts);
    attempts > max
}

/// One case's loop row.
#[derive(Debug)]
pub struct LoopRow {
    /// Case dispatched.
    pub case: String,
    /// 1.0 when a rerun achieved, else None.
    pub rerun_composite: Option<f64>,
    /// Best achieved across original + reruns.
    pub best_composite: Option<f64>,
    /// Original run + rerun attempts.
    pub attempts: usize,
    /// Retry bound exceeded with no achieved rerun.
    pub exhausted: bool,
    /// Mapped ticket id.
    pub ticket: Option<String>,
    /// Ticket lifecycle status.
    pub status: Option<String>,
    /// Lease holder.
    pub claimant: Option<String>,
}

/// `loop status` result.
#[derive(Debug)]
pub struct LoopStatus {
    /// Rows below the target.
    pub cases: Vec<LoopRow>,
    /// Case count.
    pub runs: usize,
}

/// Dispatch result.
#[derive(Debug)]
pub struct DispatchOutcome {
    /// Case dispatched.
    pub case: String,
    /// Ticket id claimed.
    pub ticket: String,
    /// Slice spec path.
    pub spec: PathBuf,
    /// Whether the ticket was created.
    pub ticket_created: bool,
}

/// Objective plan result.
#[derive(Debug)]
pub struct ObjectiveOutcome {
    /// Cases actually dispatched.
    pub dispatched: Vec<DispatchOutcome>,
    /// Skipped: no declared gate.
    pub skipped_no_verifier: Vec<String>,
    /// Skipped: blocked by an open ticket.
    pub skipped_blocked: Vec<String>,
    /// Skipped: run unreadable.
    pub skipped_unavailable: Vec<String>,
    /// Skipped: retry bound exceeded.
    pub skipped_exhausted: Vec<String>,
    /// Cost budget in USD (None = unlimited).
    pub budget_cost: Option<f64>,
    /// Declared cost of dispatched cases.
    pub budget_spent: f64,
}

/// Is `case` a rerun-output dir (`-rerun`, `-rerun-2`, ...)?
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

/// Hard wall cap (seconds) for one `loop verify` gate run
/// (ARCHITECTURE-CONDENSED 5.2 — every gate executes through
/// `worker::run_capped` with this cap).
pub const GATE_WALL_CAP_SECS: u64 = 120;

/// Resolve a declared `verify_target` into the directory the gate runs in.
///
/// Relative paths resolve against the repo root, the result is
/// canonicalized, and MUST stay inside the canonical root unless
/// `.miniagi.json` sets `allow_outside_targets: true` (default false →
/// rejected). The target must exist and be a directory.
///
/// # Errors
///
/// Returns a message when the declaration is empty, does not resolve, is
/// not a directory, or escapes the root without the explicit opt-in.
pub fn resolve_target(root: &Path, declared: &str) -> Result<PathBuf, String> {
    let declared = declared.trim();
    if declared.is_empty() {
        return Err("no verify_target declared".into());
    }
    let raw = Path::new(declared);
    let candidate = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        root.join(declared)
    };
    let canonical = candidate.canonicalize().map_err(|e| {
        format!("verify_target '{declared}' does not resolve to an existing directory ({e})")
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "verify_target '{declared}' resolves to {}, which is not a directory",
            canonical.display()
        ));
    }
    let root_canon = root
        .canonicalize()
        .map_err(|e| format!("repo root {} cannot be canonicalized ({e})", root.display()))?;
    let allow_outside = crate::config::Config::load(root).allow_outside_targets;
    if !allow_outside && !canonical.starts_with(&root_canon) {
        return Err(format!(
            "verify_target '{declared}' resolves to {}, outside the repo root {} — rejected (set allow_outside_targets: true to allow)",
            canonical.display(),
            root_canon.display()
        ));
    }
    Ok(canonical)
}

/// The cases dirs carrying a run.json.
fn case_dirs(cases_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(cases_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.join("run.json").is_file() {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// Read a case's run.
fn read_run(case_dir: &Path) -> Option<Run> {
    let text = fs::read_to_string(case_dir.join("run.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// A case is OPEN when its run reports achieved=false.
#[must_use]
pub fn case_is_open(case_dir: &Path) -> bool {
    read_run(case_dir).is_none_or(|r| !r.achieved())
}

/// Does `haystack` mention `case` as a whole token (bounded by a
/// non-alphanumeric char or the edges)? A raw `contains` would alias
/// `gap-a` to the ticket of `gap-ab` (wrong ticket claimed/closed).
/// A case name is matched as a WHOLE token: the neighbours must be a
/// boundary char, where `-` and `_` are TOKEN chars (a hyphen-prefixed
/// sibling like `gap-a-b` must NOT token-match `gap-a`).
fn is_case_boundary(c: char) -> bool {
    !c.is_alphanumeric() && c != '-' && c != '_'
}

fn mentions_case(haystack: &str, case: &str) -> bool {
    haystack.match_indices(case).any(|(pos, _)| {
        let before_ok = haystack[..pos]
            .chars()
            .next_back()
            .is_none_or(is_case_boundary);
        let after = &haystack[pos + case.len()..];
        let after_ok = after.chars().next().is_none_or(is_case_boundary);
        before_ok && after_ok
    })
}

/// Find the ticket whose goal/title/id references `case` (token-bounded).
#[must_use]
pub fn ticket_for_case(root: &Path, case: &str) -> Option<crate::ticket::Ticket> {
    crate::ticket::list_tickets(root)
        .unwrap_or_default()
        .into_iter()
        .find(|t| {
            mentions_case(&t.goal, case)
                || mentions_case(&t.title, case)
                || id_matches_case(&t.id, &case.to_lowercase())
        })
}

fn id_matches_case(id: &str, case_lower: &str) -> bool {
    let id_lower = id.to_lowercase();
    let Some(rest) = id_lower.strip_prefix("ticket-") else {
        return false;
    };
    let needle = format!("ticket-{rest}");
    case_lower.match_indices(&needle).any(|(pos, _)| {
        // LEFT boundary too (parity with mentions_case): `xyticket-7`
        // must not alias to TICKET-7.
        let before_ok = case_lower[..pos]
            .chars()
            .next_back()
            .is_none_or(is_case_boundary);
        before_ok
            && case_lower[pos + needle.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_ascii_digit())
    })
}

/// Claimant of a ticket, if any.
#[must_use]
fn claimant_for(root: &Path, ticket_id: &str) -> Option<String> {
    // A corrupt registry must not SILENTLY look empty here: mark_state's
    // Exhausted path would then skip the claim release and strand the
    // lease on a terminal case. Be loud (the authoritative sites already
    // hard-fail; this read is best-effort but never silent).
    match crate::ticket::read_claims(root) {
        Ok(claims) => claims
            .into_iter()
            .find(|c| c.ticket == ticket_id)
            .map(|c| c.claimant),
        Err(e) => {
            eprintln!("loopcmd: claims registry unreadable ({e}) — lease lookups are incomplete");
            None
        }
    }
}

/// Rerun attempts for a case (`<case>-rerun`, `-rerun-2`, ...).
fn rerun_dirs(cases_dir: &Path, case: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(cases_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(&format!("{case}-rerun")) && is_rerun_case(&name) {
            out.push(e.path());
        }
    }
    out
}

/// Count of rerun attempts for a case.
#[must_use]
pub fn count_reruns(root: &Path, case: &str) -> usize {
    rerun_dirs(&root.join("evals/cases"), case).len()
}

/// Cases below the loop target with their work-graph mapping.
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn status(root: &Path) -> Result<LoopStatus, io::Error> {
    let cases_dir = root.join("evals/cases");
    let mut rows = Vec::new();
    for dir in case_dirs(&cases_dir) {
        let case = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Rerun dirs are attempt artifacts, not gaps — they close the
        // BASE and never own a ledger row (parity with pick_target /
        // objective).
        if is_rerun_case(&case) {
            continue;
        }
        let run = read_run(&dir);
        let open = run.is_none_or(|r| !r.achieved());
        if !open {
            continue;
        }
        if read_ledger(root, &case).is_some_and(|g| gap_is_terminal(&g.state)) {
            continue;
        }
        let ticket = ticket_for_case(root, &case);
        let (ticket_id, status_, claimant) = ticket.as_ref().map_or((None, None, None), |t| {
            (
                Some(t.id.clone()),
                Some(t.status.clone()),
                claimant_for(root, &t.id),
            )
        });
        let reruns = rerun_dirs(&cases_dir, &case);
        let rerun_achieved = reruns
            .iter()
            .any(|d| read_run(d).is_some_and(|r| r.achieved()));
        // The ledger's attempt count is authoritative when a row exists
        // (the dispatch-time bound uses it); fall back to the filesystem
        // count for never-dispatched cases so status and dispatch agree.
        let attempts = read_ledger(root, &case).map_or(1 + reruns.len(), |g| g.attempts.max(1));
        let max_reruns = crate::config::Config::load(root).max_rerun_attempts;
        rows.push(LoopRow {
            case: case.clone(),
            rerun_composite: rerun_achieved.then_some(1.0),
            best_composite: Some(if rerun_achieved { 1.0 } else { 0.0 }),
            attempts,
            exhausted: max_reruns.is_some_and(|m| attempts > m) && !rerun_achieved,
            ticket: ticket_id,
            status: status_,
            claimant,
        });
    }
    rows.sort_by(|a, b| a.case.cmp(&b.case));
    Ok(LoopStatus {
        cases: rows,
        runs: case_dirs(&cases_dir).len(),
    })
}

/// Returns whether `case` is a non-hidden plain path segment.
#[must_use]
pub fn case_is_plain_segment(case: &str) -> bool {
    !case.is_empty()
        && case != "."
        && case != ".."
        && !case.starts_with('.')
        && !case.contains('/')
        && !case.contains('\\')
        && !case.contains(':')
}

/// No-progress guard: explain why dispatch has no work.
#[must_use]
pub fn dispatch_no_work(root: &Path, _below: f64) -> Option<String> {
    let cases_dir = root.join("evals/cases");
    let candidates: Vec<PathBuf> = case_dirs(&cases_dir)
        .into_iter()
        .filter(|d| {
            let name = d
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            !is_rerun_case(&name)
                && read_run(d).is_none_or(|r| !r.achieved())
                && !read_ledger(root, &name).is_some_and(|g| gap_is_terminal(&g.state))
                && !case_exceeded_bound(root, &name)
        })
        .collect();
    if candidates.is_empty() {
        return Some("no cases below the target — loop is clear".to_string());
    }
    let mut closed = 0usize;
    let mut leased = 0usize;
    for d in &candidates {
        let name = d
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(t) = ticket_for_case(root, &name)
            && t.status == "CLOSED"
        {
            closed += 1;
        } else if let Some(t) = ticket_for_case(root, &name)
            && claimant_for(root, &t.id).is_some()
        {
            leased += 1;
        }
    }
    if closed + leased < candidates.len() {
        return None;
    }
    let mut parts = Vec::new();
    if closed > 0 {
        parts.push(format!("{closed} closed"));
    }
    if leased > 0 {
        parts.push(format!("{leased} leased/claimed"));
    }
    Some(format!(
        "{} case(s) below target, none dispatchable — {}; STOP",
        candidates.len(),
        parts.join(", ")
    ))
}

/// Pick the worst open case (explicit `case`, or the first open one).
fn pick_target(root: &Path, case: Option<&str>) -> Result<String, String> {
    let cases_dir = root.join("evals/cases");
    if let Some(case) = case {
        if !case_is_plain_segment(case) {
            return Err(format!(
                "invalid case name '{case}' — use a plain name (no separators)"
            ));
        }
        let dir = cases_dir.join(case);
        if !dir.join("run.json").is_file() {
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
        if let Some(gap) = read_ledger_checked(root, case)?
            && gap_is_terminal(&gap.state)
        {
            return Err(format!(
                "case '{case}' is already {} in the ledger (evals/ledger/{case}.json)",
                gap.state_name()
            ));
        }
        if case_exceeded_bound(root, case) {
            mark_state(root, case, GapState::Exhausted);
            return Err(format!(
                "case '{case}' has exceeded max_rerun_attempts — exhausted in the ledger"
            ));
        }
        return Ok(case.to_string());
    }
    for d in case_dirs(&cases_dir) {
        let name = d
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_rerun_case(&name) || read_run(&d).is_some_and(|r| r.achieved()) {
            continue;
        }
        if let Some(gap) = read_ledger_checked(root, &name)?
            && gap_is_terminal(&gap.state)
        {
            continue;
        }
        if case_exceeded_bound(root, &name) {
            mark_state(root, &name, GapState::Exhausted);
            continue;
        }
        if let Some(ticket) = ticket_for_case(root, &name)
            && (ticket.status == "CLOSED" || claimant_for(root, &ticket.id).is_some())
        {
            continue;
        }
        return Ok(name);
    }
    Err(
        "no case below the target is dispatchable (all have closed tickets or active claims)"
            .into(),
    )
}

/// `loop dispatch`: pick the worst open case, ensure its ticket, claim it
/// (lease), and write the slice spec.
///
/// # Errors
///
/// Returns a message when no case is dispatchable or a lease is held.
pub fn dispatch(
    root: &Path,
    case: Option<&str>,
    below: f64,
    claimant: &str,
) -> Result<DispatchOutcome, String> {
    let _ = below;
    crate::config::Config::load_checked(root)?;
    let case = pick_target(root, case)?;
    let run = read_run(&root.join("evals/cases").join(&case))
        .ok_or_else(|| format!("run unreadable for case '{case}'"))?;
    if run.verify_command.is_none() || run.verify_target.is_none() {
        // The case is unverifiable: record it in the ledger (terminal) so
        // the state machine reflects reality instead of an open gap that
        // can never dispatch (§3.1 OPEN->UNVERIFIABLE).
        mark_state(root, &case, GapState::Unverifiable);
        return Err(format!(
            "case '{case}' declares no complete gate (verify_command AND verify_target) — refusing dispatch"
        ));
    }
    let existing = ticket_for_case(root, &case);
    let (ticket_id, ticket_created) = if let Some(t) = existing {
        (t.id, false)
    } else {
        let id = create_case_ticket(root, &case)?;
        (id, true)
    };
    crate::ticket::claim_ticket(root, &ticket_id, claimant, false)
        .map_err(|e| format!("cannot claim {ticket_id}: {e}"))?;
    // Transactional dispatch (§3.1): one lock-held section that re-checks
    // the ledger (TOCTOU: another writer may have closed the case between
    // pick_target and the lock), marks dispatched, then writes the spec.
    // On ANY failure the claim is released and a created ticket removed —
    // no leased case with a missing spec/ledger row.
    let lock = crate::ticket::lock_claims(root).map_err(|e| {
        // Stranded-lease guard: the claim was taken above; a lock failure
        // must not leave a permanently-leased ticket with no spec/ledger.
        // Use the LOCKING release (it re-acquires, possibly stealing a
        // stale lock) — never an unlocked registry write.
        let _ = crate::ticket::release_ticket(root, &ticket_id, claimant);
        if ticket_created {
            let _ =
                fs::remove_file(crate::ticket::tickets_dir(root).join(format!("{ticket_id}.md")));
        }
        e.to_string()
    })?;
    let prior_gap = match read_ledger_checked(root, &case) {
        Ok(g) => g,
        Err(e) => {
            // Stranded-lease guard on the corrupt-ledger path too: the
            // claim + created ticket must roll back here (the rollback
            // closure below would not be reached).
            let _ = crate::ticket::release_ticket_locked(root, &ticket_id, claimant);
            if ticket_created {
                let _ = fs::remove_file(
                    crate::ticket::tickets_dir(root).join(format!("{ticket_id}.md")),
                );
            }
            return Err(e);
        }
    };
    let rollback = |prior_gap: &Option<Gap>| {
        let _ = crate::ticket::release_ticket_locked(root, &ticket_id, claimant);
        if ticket_created {
            let _ =
                fs::remove_file(crate::ticket::tickets_dir(root).join(format!("{ticket_id}.md")));
        }
        // Restore the PRIOR ledger row — never delete it: a pre-existing
        // row carries attempt history (deleting it would reset the retry
        // bound and erase the case's dispatch record).
        match prior_gap {
            Some(g) => {
                let _ = write_ledger_atomic(root, g);
            }
            None => {
                let _ = fs::remove_file(ledger_path(root, &case));
            }
        }
    };
    let now = crate::memory::utc_now_stamp();
    let mut gap = prior_gap.clone().unwrap_or_else(|| Gap {
        case: case.clone(),
        state: GapState::Open,
        opened_by: case.clone(),
        ..Gap::default()
    });
    if gap_is_terminal(&gap.state) {
        rollback(&prior_gap);
        return Err(format!(
            "case '{case}' is {} in the ledger (closed between pick and claim) — redispatch refused",
            gap.state_name()
        ));
    }
    if gap.first_dispatched_at.is_none() {
        gap.first_dispatched_at = Some(now.clone());
    }
    gap.attempts += 1;
    gap.last_attempted_at = Some(now);
    gap.state = GapState::Dispatched;
    // Spec FIRST, ledger LAST: the ledger row is the commit point — a
    // spec failure leaves NO phantom dispatched row (§3.1 ordering).
    let spec = match write_spec(root, &case, &ticket_id) {
        Ok(s) => s,
        Err(e) => {
            rollback(&prior_gap);
            return Err(format!("cannot write spec: {e}"));
        }
    };
    write_ledger_atomic(root, &gap).map_err(|e| {
        rollback(&prior_gap);
        format!("cannot write ledger: {e}")
    })?;
    drop(lock);
    Ok(DispatchOutcome {
        case,
        ticket: ticket_id,
        spec,
        ticket_created,
    })
}

/// `loop objective`: dispatch the worst open cases up to `max_cases`.
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
    crate::config::Config::load_checked(root)?;
    let cases_dir = root.join("evals/cases");
    let mut out = ObjectiveOutcome {
        dispatched: Vec::new(),
        skipped_no_verifier: Vec::new(),
        skipped_blocked: Vec::new(),
        skipped_unavailable: Vec::new(),
        skipped_exhausted: Vec::new(),
        budget_cost,
        budget_spent: 0.0,
    };
    for d in case_dirs(&cases_dir) {
        if out.dispatched.len() >= max_cases {
            break;
        }
        let name = d
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_rerun_case(&name) || read_run(&d).is_some_and(|r| r.achieved()) {
            continue;
        }
        if read_ledger(root, &name).is_some_and(|g| gap_is_terminal(&g.state)) {
            continue;
        }
        if case_exceeded_bound(root, &name) {
            mark_state(root, &name, GapState::Exhausted);
            out.skipped_exhausted.push(name.clone());
            continue;
        }
        let Some(run) = read_run(&d) else {
            out.skipped_unavailable.push(name.clone());
            continue;
        };
        if run.verify_command.is_none() || run.verify_target.is_none() {
            out.skipped_no_verifier.push(name.clone());
            continue;
        }
        if let Some(t) = ticket_for_case(root, &name)
            && (t.status == "CLOSED" || claimant_for(root, &t.id).is_some())
        {
            continue;
        }
        // Budget governor (§5.2): stop dispatching once the declared
        // budget would be exceeded by the next case's declared cost.
        if let Some(budget) = out.budget_cost
            && out.budget_spent + run.cost_usd > budget
        {
            // STOP, not skip: the batch is ordered worst-first; once the
            // budget is exhausted the remaining cases are beyond it.
            break;
        }
        let d_out = dispatch(root, Some(&name), TARGET_COMPOSITE, claimant)?;
        out.budget_spent += run.cost_usd;
        out.dispatched.push(d_out);
    }
    Ok(out)
}

/// Write the implementation slice for a case next to its ticket.
fn write_spec(root: &Path, case: &str, ticket_id: &str) -> io::Result<PathBuf> {
    use std::fmt::Write as _;
    let run = read_run(&root.join("evals/cases").join(case))
        .ok_or_else(|| io::Error::other("run unreadable"))?;
    // Defense-in-depth: a ticket id that escaped parse_ticket's path-safe
    // check must never join into a write path (traversal sink).
    if !ticket_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ticket id '{ticket_id}' is not path-safe"),
        ));
    }
    let spec_dir = root.join("artifacts").join(ticket_id);
    fs::create_dir_all(&spec_dir)?;
    let path = spec_dir.join("spec.md");
    let mut body = format!("# SLICE SPEC — {ticket_id} (case: {case})\n\n");
    body.push_str("- source: `mini-agi loop dispatch` (condensed)\n");
    // Flatten the goal: this file is EXECUTABLE and the worker parses the
    // FIRST `- verify_command:` line — a goal containing an injected
    // `\n- verify_command: true` would replace the declared gate.
    let goal_flat = run.goal.split_whitespace().collect::<Vec<_>>().join(" ");
    let _ = writeln!(body, "- goal: {goal_flat}");
    // The spec is EXECUTABLE: `mini-agi codex <spec>` parses and runs
    // these. Redacting them would execute a MUTATED gate (diverging from
    // the run.json executable copy). Display surfaces (loop verify output,
    // ledger) stay redacted; this executable contract must not be.
    // Flatten NEWLINES in the command (preserving quotes/spaces so the
    // executed shell string is unchanged): an injected `\n- verify_target:`
    // inside an untrusted verify_command would relocate the worker's gate
    // target ahead of the real line.
    let vc = run.verify_command.unwrap_or_default().replace('\n', " ");
    // Flatten vt too: an injected `\n- verify_command:` line in an
    // untrusted verify_target must not reach the executable spec.
    let vt = run
        .verify_target
        .unwrap_or_else(|| "<repo root>".into())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // Separate lines, never `{vc} in {vt}`: a command legitimately
    // containing " in " must not be re-split by the codex parser.
    let _ = writeln!(body, "- verify_command: {vc}");
    let _ = writeln!(body, "- verify_target: {vt}");
    let _ = writeln!(
        body,
        "- acceptance: `mini-agi loop verify {case}-rerun` closes only when the declared gate passes"
    );
    fs::write(&path, body)?;
    Ok(path)
}

fn create_case_ticket(root: &Path, case: &str) -> Result<String, String> {
    let dir = crate::ticket::tickets_dir(root);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _lock = crate::ticket::lock_claims(root).map_err(|e| e.to_string())?;
    // Re-check under the lock: two concurrent dispatches of the same case
    // can both miss the pre-check; if a ticket for the case now exists,
    // reuse it instead of creating a second one (two live leases for one
    // case strand the loser — terminal ledger rows have no release path).
    if let Some(t) = ticket_for_case(root, case) {
        return Ok(t.id);
    }
    let next = crate::ticket::list_tickets(root)
        .unwrap_or_default()
        .iter()
        .filter_map(|t| {
            t.id.strip_prefix("TICKET-")
                .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
                .and_then(|d| d.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1;
    let id = format!("TICKET-{next}");
    let body = format!(
        "# Ticket\n\n- id: {id}\n- title: Fix capability gap: {case} below the loop target\n- goal (one sentence): Bring {case} to achieved by fixing the failing run.\n- scope: evals/cases\n- domain: eval\n"
    );
    fs::write(dir.join(format!("{id}.md")), body).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Verify a rerun: run the declared gate; close when it passes.
///
/// # Errors
///
/// Returns a message when the case cannot be read.
pub fn verify(
    root: &Path,
    case: &str,
    claimant: &str,
    allow_unverified: bool,
) -> Result<(String, bool), String> {
    crate::config::Config::load_checked(root)?;
    if !case_is_plain_segment(case) {
        return Err(format!(
            "invalid case name '{case}' — use a plain name (no separators)"
        ));
    }
    // The base is the gap owner: `foo-rerun` -> `foo`, `foo-rerun-2` ->
    // `foo` (ARCHITECTURE-CONDENSED: rerun dirs are attempt artifacts and
    // never own a ledger row; a numbered rerun still closes the BASE).
    // Only a SUFFIX counts: a plain case name with a mid-string `-rerun-`
    // (e.g. `my-rerun-tool`) is NOT a rerun dir — `is_rerun_case` treats
    // it as a base and the gate must close IT, not strip it.
    let base = case.strip_suffix("-rerun").map_or_else(
        || {
            let mut base = case;
            while let Some(idx) = base.rfind("-rerun-") {
                let tail = &base[idx + "-rerun-".len()..];
                if !tail.is_empty() && tail.bytes().all(|c| c.is_ascii_digit()) {
                    base = &base[..idx];
                } else {
                    break;
                }
            }
            base
        },
        |stripped| stripped,
    );
    let run_path = root.join("evals/cases").join(case).join("run.json");
    let run = read_run(run_path.parent().expect("case dir"))
        .ok_or_else(|| format!("cannot read {case}"))?;
    // The gate is the BASE case's declared contract (§3.2): when the
    // closing dir is a rerun, the rerun's own run.json must not be able
    // to weaken the acceptance — an edited rerun gate would change what
    // "close" means. `achieved` still comes from the closing run.
    // FAIL-CLOSED: an unreadable/corrupt base run.json must not silently
    // fall back to the rerun's self-declared gate.
    let gate = if base.ne(case) {
        read_run(&root.join("evals/cases").join(base)).ok_or_else(|| {
            format!("cannot read the base case's run.json to obtain its declared gate ({base})")
        })?
    } else {
        run.clone()
    };
    let mut lines = vec![format!("verify {case}: achieved={}", run.achieved())];

    // Guards run BEFORE the gate executes (up to 120s): a terminal base
    // must not be re-gated, and a foreign claim must not force a gate run
    // only to be refused after.
    if let Some(gap) = read_ledger_checked(root, base)?
        && gap_is_terminal(&gap.state)
    {
        lines.push(format!(
            "  gap already {} in the ledger — no-op",
            gap.state_name()
        ));
        let closed = gap.state == GapState::Closed;
        lines.insert(
            0,
            format!("loop verify: {}", if closed { "CLOSED" } else { "OPEN" }),
        );
        return Ok((lines.join("\n"), closed));
    }
    if let Some(t) = ticket_for_case(root, base)
        && let Some(foreign) = crate::ticket::read_claims(root)
            .map_err(|e| e.to_string())?
            .iter()
            .find(|c| c.ticket == t.id && c.claimant != claimant)
            .map(|c| c.claimant.clone())
    {
        return Err(format!(
            "cannot verify {base}: ticket {} is claimed by {foreign}, not {claimant}",
            t.id
        ));
    }

    let mut passed = false;
    if let (Some(cmd), Some(target)) = (&gate.verify_command, &gate.verify_target) {
        // The declared target is UNTRUSTED run data: resolve it against
        // the repo root and refuse to run the gate anywhere that escapes
        // it (ARCHITECTURE-CONDENSED 5.1), then execute through the
        // capped runner (5.2): hard wall cap + truncated output, never a
        // bare `Command::output()`.
        let target_dir = resolve_target(root, target)?;
        // §5.3: the command is echoed redacted (embedded credentials
        // never reach stdout) — the ORIGINAL still executes.
        let cmd_r = crate::redact::redact(cmd);
        let res =
            crate::worker::run_capped("sh", &["-c", cmd], &target_dir, Some(GATE_WALL_CAP_SECS))
                .map_err(|e| format!("gate unavailable: {e}"))?;
        if res.aborted {
            lines.push(format!(
                "  gate: FAIL ({cmd_r} aborted after {GATE_WALL_CAP_SECS}s wall cap)"
            ));
        } else if res.status == Some(0) {
            passed = true;
            lines.push(format!("  gate: PASS ({cmd_r})"));
        } else {
            lines.push(format!(
                "  gate: FAIL ({cmd_r} exit {})",
                res.status.unwrap_or(-1)
            ));
        }
    } else if allow_unverified {
        passed = true;
        lines.push("  gate: not declared — closing on --allow-unverified (explicit trust)".into());
    } else {
        lines.push(
            "  gate: not declared — close requires a declared verify_command or --allow-unverified"
                .into(),
        );
    }

    // Re-read the closing run's `achieved` AFTER the gate: the gate ran
    // for up to 120s, during which a concurrent worker could have
    // rewritten the rerun dir's run.json (achieved true->false). Closing
    // on the STALE pre-gate value would stamp closed_by/verified_at on a
    // run the disk no longer reports as achieved.
    // FAIL-CLOSED: an unreadable closing run.json during the gate means
    // the disk no longer reports the run — do NOT fall back to the stale
    // pre-gate value (the close would stamp evidence the disk lacks).
    let achieved_now = read_run(&root.join("evals/cases").join(case)).is_some_and(|r| r.achieved());
    let closed = achieved_now && passed;
    if closed {
        // Atomic close under the claims lock: ledger (state=closed,
        // closed_by=<closing rerun dir>, verified_at) -> release the
        // claim -> ticket file status: CLOSED. Any failure rolls every
        // on-disk state back (the ledger is the single commit point).
        let close_lock = crate::ticket::lock_claims(root).map_err(|e| e.to_string())?;
        let prior_gap = read_ledger_checked(root, base)?;
        // Idempotence guard: a terminal prior state (already Closed /
        // Exhausted / Unverifiable) must not be re-closed — re-running
        // `loop verify` on a closed base would re-execute the gate and
        // overwrite closed_by/verified_at. Mirrors the dispatch guard.
        if let Some(g) = &prior_gap
            && gap_is_terminal(&g.state)
        {
            drop(close_lock);
            lines.push(format!(
                "  gap already {} in the ledger — no-op",
                g.state_name()
            ));
            lines.insert(
                0,
                format!(
                    "loop verify: {}",
                    if g.state == GapState::Closed {
                        "CLOSED"
                    } else {
                        "OPEN"
                    }
                ),
            );
            return Ok((lines.join("\n"), g.state == GapState::Closed));
        }
        // An unreadable claims registry must NOT be treated as empty (the
        // close would rewrite it and silently erase every lease).
        let prior_claims = crate::ticket::read_claims(root).map_err(|e| e.to_string())?;
        let prior_ticket = ticket_for_case(root, base);
        let now = crate::memory::utc_now_stamp();
        let close_gap = Gap {
            case: base.to_string(),
            state: GapState::Closed,
            opened_by: prior_gap
                .as_ref()
                .map_or_else(|| base.to_string(), |g| g.opened_by.clone()),
            first_dispatched_at: prior_gap
                .as_ref()
                .and_then(|g| g.first_dispatched_at.clone()),
            attempts: prior_gap.as_ref().map_or(1, |g| g.attempts.max(1)),
            last_attempted_at: prior_gap.as_ref().and_then(|g| g.last_attempted_at.clone()),
            closed_by: Some(case.to_string()),
            verified_at: Some(now),
            closed_ticket: prior_ticket.as_ref().map(|t| t.id.clone()),
        };
        let close_ticket = prior_ticket.as_ref().map(|t| t.id.clone());
        let rollback = |root: &Path,
                        prior_gap: Option<&Gap>,
                        prior_claims: &[crate::ticket::Claim],
                        close_ticket: &Option<String>,
                        prior_status: &Option<String>| {
            let _ = match prior_gap {
                Some(g) => write_ledger_atomic(root, g),
                None => fs::remove_file(ledger_path(root, close_gap.case.as_str())),
            };
            let _ = crate::ticket::write_claims_registry(root, prior_claims);
            if let (Some(id), Some(status)) = (close_ticket, prior_status) {
                let _ = crate::ticket::set_ticket_status(root, id, status);
            }
        };
        let prior_status = prior_ticket.as_ref().map(|t| t.status.clone());
        // Foreign-claim guard: if another claimant holds the lease, this
        // verify must not mark the ticket CLOSED (that would strand their
        // claim with no release path — the close bypasses release_ticket).
        if let Some(id) = close_ticket.as_ref()
            && let Some(foreign) = prior_claims
                .iter()
                .find(|c| &c.ticket == id && c.claimant != claimant)
                .map(|c| c.claimant.as_str())
        {
            return Err(format!(
                "cannot close {base}: ticket {id} is claimed by {foreign}, not {claimant}"
            ));
        }
        if let Err(e) = write_ledger_atomic(root, &close_gap) {
            return Err(format!("cannot write ledger: {e}"));
        }
        if let Some(id) = &close_ticket {
            let mine = prior_claims
                .iter()
                .any(|c| c.ticket == *id && c.claimant == claimant);
            if mine {
                let remaining: Vec<_> = prior_claims
                    .iter()
                    .filter(|c| c.ticket != *id)
                    .cloned()
                    .collect();
                if let Err(e) = crate::ticket::write_claims_registry(root, &remaining) {
                    rollback(
                        root,
                        prior_gap.as_ref(),
                        &prior_claims,
                        &close_ticket,
                        &prior_status,
                    );
                    return Err(format!("cannot release {id}: {e}"));
                }
            }
            if let Err(e) = crate::ticket::set_ticket_status(root, id, "CLOSED") {
                rollback(
                    root,
                    prior_gap.as_ref(),
                    &prior_claims,
                    &close_ticket,
                    &prior_status,
                );
                return Err(format!("cannot mark {id} CLOSED: {e}"));
            }
        }
        lines.push(format!(
            "  gap closed: {base} (gate passed) ledger state=closed closed_by={case}"
        ));
    } else {
        // DISPATCHED->DISPATCHED (§3.1): a failed/not-achieved verify is
        // still an attempt — record it in the base's ledger (under the
        // lock) so attempts track verification, not just dispatches.
        let _lock = crate::ticket::lock_claims(root).map_err(|e| e.to_string())?;
        if let Some(mut gap) = read_ledger_checked(root, base)? {
            // Terminal check (mirrors the close branch): another process
            // may have closed the base while our gate ran — never mutate
            // a Closed/Exhausted row's attempt count.
            if !gap_is_terminal(&gap.state) {
                gap.attempts += 1;
                gap.last_attempted_at = Some(crate::memory::utc_now_stamp());
                if let Err(e) = write_ledger_atomic(root, &gap) {
                    // The attempt record is lost — say so in the verdict so a
                    // caller cannot mistake "attempt recorded" for "attempt
                    // lost" (the retry bound would silently stop counting).
                    lines.push(format!(
                        "  warning: could not record the failed verify attempt for {base}: {e}"
                    ));
                }
            }
        }
        lines.push("  gap open: outcome not verified — keep working".into());
    }
    lines.insert(
        0,
        format!("loop verify: {}", if closed { "CLOSED" } else { "OPEN" }),
    );
    Ok((lines.join("\n"), closed))
}

#[cfg(test)]
mod loop_tests {
    use super::*;
    use std::fs;

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mag-loop-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("evals/cases")).unwrap();
        fs::create_dir_all(root.join("tickets")).unwrap();
        root
    }

    fn write_run(root: &Path, case: &str, achieved: bool, gate: Option<(&str, &str)>) {
        let dir = root.join("evals/cases").join(case);
        fs::create_dir_all(&dir).unwrap();
        let run = serde_json::json!({
            "goal": case,
            "scope": ["x"],
            "outcome": { "achieved": achieved },
            "trajectory": [],
            "verify_command": gate.map(|(c, _)| c),
            "verify_target": gate.map(|(_, t)| t),
        });
        fs::write(dir.join("run.json"), serde_json::to_string(&run).unwrap()).unwrap();
    }

    fn write_ticket(root: &Path, case: &str) -> String {
        let id = format!(
            "TICKET-{:04}",
            case.bytes().map(u32::from).sum::<u32>() % 10000
        );
        fs::write(
            root.join("tickets").join(format!("{id}.md")),
            format!("- id: {id}\n- title: {case} gap\n- goal: fix {case}\n- scope: evals/cases\n"),
        )
        .unwrap();
        id
    }

    #[test]
    fn verify_closes_only_when_gate_passes_and_run_achieved() {
        let root = tmp_root("v-close");
        write_run(&root, "gap-a", false, Some(("true", ".")));
        write_run(&root, "gap-a-rerun", true, Some(("true", ".")));
        let ticket = write_ticket(&root, "gap-a");
        crate::ticket::claim_ticket(&root, &ticket, "t", true).unwrap();
        let (text, closed) = verify(&root, "gap-a-rerun", "t", false).unwrap();
        assert!(closed, "passing gate on an achieved run closes: {text}");
        assert!(
            crate::ticket::read_claims(&root).unwrap().is_empty(),
            "lease released"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_stays_open_when_gate_fails() {
        let root = tmp_root("v-fail");
        write_run(&root, "gap-b", false, Some(("false", ".")));
        write_run(&root, "gap-b-rerun", true, Some(("false", ".")));
        let ticket = write_ticket(&root, "gap-b");
        crate::ticket::claim_ticket(&root, &ticket, "t", true).unwrap();
        let (text, closed) = verify(&root, "gap-b-rerun", "t", false).unwrap();
        assert!(!closed, "a failing gate keeps the gap open: {text}");
        assert!(
            crate::ticket::read_claims(&root)
                .unwrap()
                .iter()
                .any(|c| c.ticket == ticket),
            "claim held on failure"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_open_without_declared_gate_and_no_allow() {
        let root = tmp_root("v-nogate");
        write_run(&root, "gap-c", false, None);
        write_run(&root, "gap-c-rerun", true, None);
        let (_, closed) = verify(&root, "gap-c-rerun", "t", false).unwrap();
        assert!(!closed, "no gate + no allow-unverified stays open");
        let (_, closed2) = verify(&root, "gap-c-rerun", "t", true).unwrap();
        assert!(closed2, "allow-unverified closes on explicit trust");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dispatch_rejects_case_without_complete_gate() {
        let root = tmp_root("d-gate");
        write_run(&root, "gap-d", false, None);
        let err = dispatch(&root, Some("gap-d"), 0.5, "t").unwrap_err();
        assert!(err.contains("verify_command AND verify_target"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn status_lists_open_and_skips_achieved() {
        let root = tmp_root("s-list");
        write_run(&root, "open-a", false, None);
        write_run(&root, "done-b", true, None);
        let s = status(&root).unwrap();
        assert_eq!(s.cases.len(), 1);
        assert_eq!(s.cases[0].case, "open-a");
        let _ = fs::remove_dir_all(&root);
    }

    fn write_ledger(root: &Path, case: &str, state: GapState) -> Gap {
        let gap = Gap {
            case: case.to_string(),
            state,
            opened_by: case.to_string(),
            ..Gap::default()
        };
        write_ledger_atomic(root, &gap).unwrap();
        gap
    }

    #[test]
    fn dispatch_writes_a_ledger_row_and_terminal_state_blocks_redispatch() {
        let root = tmp_root("l-dispatch");
        write_run(&root, "gap-x", false, Some(("true", ".")));
        let out = dispatch(&root, Some("gap-x"), 0.5, "t").unwrap();
        assert_eq!(out.case, "gap-x");
        let row = read_ledger(&root, "gap-x").expect("dispatch writes a ledger row");
        assert_eq!(row.state, GapState::Dispatched);
        assert!(row.first_dispatched_at.is_some(), "first dispatch stamped");
        assert_eq!(row.attempts, 1);
        write_ledger(&root, "gap-x", GapState::Closed);
        let err = dispatch(&root, Some("gap-x"), 0.5, "t").unwrap_err();
        assert!(
            err.contains("closed"),
            "a terminal ledger state blocks redispatch: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dispatch_skips_terminal_cases_in_auto_pick() {
        let root = tmp_root("l-pick");
        write_run(&root, "gap-z", false, Some(("true", ".")));
        write_ledger(&root, "gap-z", GapState::Closed);
        let err = dispatch(&root, None, 0.5, "t").unwrap_err();
        assert!(
            err.contains("no case"),
            "auto-pick skips terminal-ledger cases: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_marks_the_base_ledger_closed_and_closes_the_ticket() {
        let root = tmp_root("l-close");
        write_run(&root, "gap-y", false, Some(("true", ".")));
        write_run(&root, "gap-y-rerun", true, Some(("true", ".")));
        let ticket = write_ticket(&root, "gap-y");
        crate::ticket::claim_ticket(&root, &ticket, "t", true).unwrap();
        let (text, closed) = verify(&root, "gap-y-rerun", "t", false).unwrap();
        assert!(closed, "{text}");
        let row = read_ledger(&root, "gap-y").expect("verify writes a closed ledger row");
        assert_eq!(row.state, GapState::Closed);
        assert_eq!(row.closed_by.as_deref(), Some("gap-y-rerun"));
        assert!(row.verified_at.is_some(), "verified_at stamped on close");
        assert_eq!(row.closed_ticket.as_deref(), Some(ticket.as_str()));
        let t = crate::ticket::find_ticket(&root, &ticket).unwrap();
        assert_eq!(t.status, "CLOSED", "ticket file carries status: CLOSED");
        assert!(
            crate::ticket::read_claims(&root).unwrap().is_empty(),
            "lease released on close"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_target_requires_a_declaration() {
        let root = tmp_root("rt-empty");
        assert!(resolve_target(&root, "").is_err(), "empty target rejected");
        assert!(
            resolve_target(&root, "   ").is_err(),
            "blank target rejected"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_target_rejects_outside_absolute_paths() {
        let root = tmp_root("rt-abs");
        let err = resolve_target(&root, "/etc").unwrap_err();
        assert!(
            err.contains("outside"),
            "absolute outside target rejected: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_target_rejects_symlink_escape() {
        let root = tmp_root("rt-sym");
        fs::create_dir_all(root.join("cases")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc", root.join("cases/escape")).unwrap();
        let err = resolve_target(&root, "cases/escape").unwrap_err();
        assert!(
            err.contains("outside"),
            "a symlink planted inside the root that escapes is rejected: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_target_resolves_relative_inside_root() {
        let root = tmp_root("rt-in");
        fs::create_dir_all(root.join("evals/cases")).unwrap();
        let t = resolve_target(&root, "evals/cases").unwrap();
        assert!(t.is_absolute(), "{t:?}");
        assert!(t.starts_with(&root), "{t:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_target_requires_an_existing_directory() {
        let root = tmp_root("rt-dir");
        let err = resolve_target(&root, "does-not-exist").unwrap_err();
        assert!(
            err.contains("directory") || err.contains("no such"),
            "{err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_target_allows_outside_when_opted_in() {
        let root = tmp_root("rt-opt");
        fs::write(
            root.join(".miniagi.json"),
            r#"{"allow_outside_targets": true}"#,
        )
        .unwrap();
        let t = resolve_target(&root, "/etc").unwrap();
        assert_eq!(t, fs::canonicalize("/etc").unwrap());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_rejects_a_gate_target_outside_the_root() {
        let root = tmp_root("v-out");
        write_run(&root, "gap-o", false, Some(("true", "/etc")));
        write_run(&root, "gap-o-rerun", true, Some(("true", "/etc")));
        let err = verify(&root, "gap-o-rerun", "t", false).unwrap_err();
        assert!(
            err.contains("outside"),
            "verify refuses to run a gate in an outside target: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn id_matches_case_requires_a_left_boundary() {
        assert!(
            !id_matches_case("TICKET-7", "xyticket-7"),
            "mid-word ticket reference must not alias"
        );
        assert!(
            !id_matches_case("TICKET-7", "gap-ticket-7"),
            "a hyphen-prefixed reference is a different token"
        );
        assert!(
            id_matches_case("TICKET-7", "gap ticket-7"),
            "a bounded ticket-7 reference matches"
        );
    }

    #[test]
    fn create_case_ticket_reuses_an_existing_case_ticket() {
        let root = tmp_root("t-reuse");
        write_run(&root, "gap-r", false, Some(("true", ".")));
        let id1 = create_case_ticket(&root, "gap-r").unwrap();
        let id2 = create_case_ticket(&root, "gap-r").unwrap();
        assert_eq!(
            id1, id2,
            "a second create for the same case reuses the ticket"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ticket_for_case_matches_whole_case_names_only() {
        let root = tmp_root("t-alias");
        fs::write(
            root.join("tickets/TICKET-1.md"),
            "- id: TICKET-1\n- title: fix gap-ab\n- goal: bring gap-ab to achieved\n- scope: evals/cases\n",
        )
        .unwrap();
        assert!(
            ticket_for_case(&root, "gap-ab").is_some(),
            "the exact case resolves its ticket"
        );
        assert!(
            ticket_for_case(&root, "gap-a").is_none(),
            "a substring case must NOT alias to the gap-ab ticket"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_on_an_already_closed_base_is_a_noop() {
        let root = tmp_root("v-idem");
        write_run(&root, "gap-q", false, Some(("true", ".")));
        write_run(&root, "gap-q-rerun", true, Some(("true", ".")));
        let ticket = write_ticket(&root, "gap-q");
        crate::ticket::claim_ticket(&root, &ticket, "t", true).unwrap();
        let (_, closed) = verify(&root, "gap-q-rerun", "t", false).unwrap();
        assert!(closed);
        let closed_at = read_ledger(&root, "gap-q").unwrap().verified_at;
        // re-run the SAME verify — must be a no-op, not re-execute + overwrite.
        let (text, closed2) = verify(&root, "gap-q-rerun", "t", false).unwrap();
        assert!(closed2, "already-closed base reports CLOSED");
        assert!(
            read_ledger(&root, "gap-q").unwrap().verified_at == closed_at,
            "verified_at must not be overwritten: {text}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_numbered_rerun_closes_the_base_not_the_rerun_dir() {
        let root = tmp_root("v-rerun2");
        write_run(&root, "gap-z", false, Some(("true", ".")));
        write_run(&root, "gap-z-rerun-2", true, Some(("true", ".")));
        let (text, closed) = verify(&root, "gap-z-rerun-2", "t", false).unwrap();
        assert!(closed, "{text}");
        assert!(
            read_ledger(&root, "gap-z").is_some_and(|g| g.state == GapState::Closed),
            "a numbered rerun (-rerun-2) closes the BASE ledger row"
        );
        assert!(
            !ledger_path(&root, "gap-z-rerun-2").is_file(),
            "a rerun dir never owns a ledger row"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn status_hides_terminal_ledger_cases() {
        let root = tmp_root("s-ledger");
        write_run(&root, "open-a", false, None);
        write_run(&root, "closed-b", false, None);
        write_ledger(&root, "closed-b", GapState::Closed);
        let s = status(&root).unwrap();
        assert_eq!(
            s.cases.len(),
            1,
            "terminal-ledger cases are not listed as open"
        );
        assert_eq!(s.cases[0].case, "open-a");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dispatch_refuses_an_exhausted_case_and_marks_the_ledger() {
        let root = tmp_root("x-disp");
        fs::write(root.join(".miniagi.json"), r#"{"max_rerun_attempts": 2}"#).unwrap();
        write_run(&root, "gap-x", false, Some(("true", ".")));
        let gap = Gap {
            case: "gap-x".into(),
            state: GapState::Dispatched,
            opened_by: "gap-x".into(),
            attempts: 3,
            ..Gap::default()
        };
        write_ledger_atomic(&root, &gap).unwrap();
        let err = dispatch(&root, Some("gap-x"), 0.5, "t").unwrap_err();
        assert!(
            err.contains("exhausted"),
            "an over-bound case is refused as exhausted: {err}"
        );
        let row = read_ledger(&root, "gap-x").expect("ledger row");
        assert_eq!(
            row.state,
            GapState::Exhausted,
            "ledger marks the case exhausted"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn objective_skips_exhausted_cases_and_reports_them() {
        let root = tmp_root("x-obj");
        fs::write(root.join(".miniagi.json"), r#"{"max_rerun_attempts": 1}"#).unwrap();
        write_run(&root, "exh-a", false, Some(("true", ".")));
        write_run(&root, "ok-b", false, Some(("true", ".")));
        let gap = Gap {
            case: "exh-a".into(),
            state: GapState::Dispatched,
            opened_by: "exh-a".into(),
            attempts: 2,
            ..Gap::default()
        };
        write_ledger_atomic(&root, &gap).unwrap();
        let out = objective(&root, 5, "t", None).unwrap();
        assert_eq!(out.dispatched.len(), 1, "only the open case dispatches");
        assert_eq!(out.dispatched[0].case, "ok-b");
        assert!(out.skipped_exhausted.contains(&"exh-a".to_string()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_mid_string_rerun_is_a_base_case_not_stripped() {
        let root = tmp_root("v-midrerun");
        write_run(&root, "my-rerun-tool", true, Some(("true", ".")));
        let (text, closed) = verify(&root, "my-rerun-tool", "t", false).unwrap();
        assert!(closed, "{text}");
        assert!(
            read_ledger(&root, "my-rerun-tool").is_some_and(|g| g.state == GapState::Closed),
            "a base case with a mid-string '-rerun-' closes ITS OWN ledger row"
        );
        assert!(
            !ledger_path(&root, "my").is_file(),
            "the mid-string name is never stripped"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_uses_the_base_gate_not_the_rerun_gate() {
        let root = tmp_root("v-basegate");
        write_run(&root, "gap-g", false, Some(("false", ".")));
        write_run(&root, "gap-g-rerun", true, Some(("true", ".")));
        let (text, closed) = verify(&root, "gap-g-rerun", "t", false).unwrap();
        assert!(
            !closed,
            "a rerun cannot weaken the base's failing gate to close the gap: {text}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn verify_refuses_close_when_another_claimant_holds_the_lease() {
        let root = tmp_root("v-foreign");
        write_run(&root, "gap-f", false, Some(("true", ".")));
        write_run(&root, "gap-f-rerun", true, Some(("true", ".")));
        let ticket = write_ticket(&root, "gap-f");
        crate::ticket::claim_ticket(&root, &ticket, "alice", true).unwrap();
        let err = verify(&root, "gap-f-rerun", "bob", false).unwrap_err();
        assert!(
            err.contains("claimed by alice"),
            "a close by a non-holder is refused: {err}"
        );
        assert!(
            crate::ticket::read_claims(&root)
                .unwrap()
                .iter()
                .any(|c| c.ticket == ticket && c.claimant == "alice"),
            "the foreign lease survives"
        );
        assert!(
            !read_ledger(&root, "gap-f").is_some_and(|g| g.state == GapState::Closed),
            "no close, no ledger write"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dispatch_refuses_on_a_malformed_config() {
        let root = tmp_root("cfg-bad");
        fs::write(root.join(".miniagi.json"), "{ not json").unwrap();
        write_run(&root, "gap-c", false, Some(("true", ".")));
        let err = dispatch(&root, Some("gap-c"), 0.5, "t").unwrap_err();
        assert!(
            err.contains("invalid JSON"),
            "a malformed .miniagi.json is a hard error, not silently unlimited: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dispatch_marks_a_no_gate_case_unverifiable() {
        let root = tmp_root("u-nogate");
        write_run(&root, "gap-u", false, None);
        dispatch(&root, Some("gap-u"), 0.5, "t").unwrap_err();
        let row = read_ledger(&root, "gap-u").expect("ledger row");
        assert_eq!(row.state, GapState::Unverifiable);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn objective_stops_at_the_budget_governor() {
        let root = tmp_root("b-budget");
        for (case, cost) in [("aa-x", 0.4f64), ("bb-y", 0.4f64)] {
            let dir = root.join("evals/cases").join(case);
            fs::create_dir_all(&dir).unwrap();
            let run = serde_json::json!({
                "goal": case, "scope": [], "outcome": {"achieved": false},
                "trajectory": [], "cost_usd": cost,
                "verify_command": "true", "verify_target": ".",
            });
            fs::write(dir.join("run.json"), serde_json::to_string(&run).unwrap()).unwrap();
        }
        let out = objective(&root, 5, "t", Some(0.5)).unwrap();
        assert_eq!(out.dispatched.len(), 1, "budget stops the batch");
        assert!(
            out.budget_spent <= 0.5,
            "spent {:.2} within budget",
            out.budget_spent
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn exhausted_mark_releases_the_claim() {
        let root = tmp_root("x-release");
        write_run(&root, "gap-x", false, Some(("true", ".")));
        let ticket = write_ticket(&root, "gap-x");
        crate::ticket::claim_ticket(&root, &ticket, "t", true).unwrap();
        let gap = Gap {
            case: "gap-x".into(),
            state: GapState::Dispatched,
            opened_by: "gap-x".into(),
            attempts: 5,
            ..Gap::default()
        };
        write_ledger_atomic(&root, &gap).unwrap();
        fs::write(root.join(".miniagi.json"), r#"{"max_rerun_attempts": 2}"#).unwrap();
        let err = dispatch(&root, Some("gap-x"), 0.5, "t").unwrap_err();
        assert!(err.contains("exhausted"), "{err}");
        assert_eq!(
            read_ledger(&root, "gap-x").unwrap().state,
            GapState::Exhausted
        );
        assert!(
            crate::ticket::read_claims(&root).unwrap().is_empty(),
            "exhaustion releases the claim"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
