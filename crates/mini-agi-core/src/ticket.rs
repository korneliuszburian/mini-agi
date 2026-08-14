//! Ticket lifecycle — kernel side of the pipeline's ticket flow.
//!
//! Tickets live in `tickets/TICKET-<n>.md`. Two forms are accepted:
//!
//! - **JSON tickets** (the `PoC` handoff contract, `ADR-0007`): a JSON object
//!   with `id`, `title`, `goal`, `scope` — validated against the bundled
//!   `ticket` contract.
//! - **Markdown tickets**: frontmatter `id:`/`title:`/`goal:`/`scope:`
//!   (list) — the local-markdown tracker form (`to-tickets` skill).
//!
//! The eval engine already resolves `TICKET-<n>` metadata from `goal`
//! (`eval::ticket_metadata_for_run`); this module adds the lifecycle view:
//! list, show, validate.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A ticket's resolved metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Ticket {
    /// `TICKET-<n>` id.
    pub id: String,
    /// Short title.
    pub title: String,
    /// The goal handed to the agent.
    pub goal: String,
    /// Scope entries (path prefixes/globs the work may touch).
    pub scope: Vec<String>,
    /// Ids of tickets this one depends on (ADR-0008 work graph).
    #[serde(default)]
    pub blocked_by: Vec<String>,
    /// Lifecycle status: OPEN (default) or CLOSED (parsed from the file).
    #[serde(default = "default_status")]
    pub status: String,
    /// The exact file this ticket was loaded from (populated by
    /// `find_ticket`/`load_ticket`; not serialized).
    #[serde(skip)]
    pub path: PathBuf,
}

fn default_status() -> String {
    "OPEN".into()
}

/// Ticket lifecycle errors.
#[derive(Debug)]
pub enum TicketError {
    /// Filesystem error.
    Io(io::Error),
    /// The file exists but is neither valid JSON nor parseable markdown.
    Parse(String),
    /// The ticket fails the ADR-0007 contract.
    Invalid(String),
}

impl std::fmt::Display for TicketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse(m) => write!(f, "cannot parse ticket: {m}"),
            Self::Invalid(m) => write!(f, "ticket invalid: {m}"),
        }
    }
}

impl std::error::Error for TicketError {}

/// Tickets directory for a repo.
#[must_use]
pub fn tickets_dir(root: &Path) -> PathBuf {
    root.join("tickets")
}

/// Discover all `TICKET-*.md` files, sorted by id.
///
/// # Errors
///
/// Returns [`TicketError::Io`] when the tickets directory cannot be read.
pub fn list_tickets(root: &Path) -> Result<Vec<Ticket>, TicketError> {
    let dir = tickets_dir(root);
    let entries = fs::read_dir(&dir).map_err(TicketError::Io)?;
    let mut tickets = Vec::new();
    for entry in entries {
        let entry = entry.map_err(TicketError::Io)?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("TICKET-"))
            && path.extension().is_some_and(|e| e == "md")
        {
            match load_ticket(&path) {
                Ok(ticket) => tickets.push(ticket),
                Err(e) => eprintln!(
                    "warning: {} unreadable ({e}) — skipping it (its number stays reserved)",
                    path.display()
                ),
            }
        }
    }
    tickets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tickets)
}

/// Highest `TICKET-<n>.md` number present in the tickets directory.
///
/// Counts EVERY matching file regardless of parseability: a corrupt
/// ticket file must still reserve its number, or `create_case_ticket`
/// would pick a colliding id and truncate it away.
///
/// Returns `Ok(None)` when the directory is empty or the highest match
/// has no numeric prefix; `Err` when the directory cannot be read —
/// callers must FAIL CLOSED (never fall back to `TICKET-1`, which would
/// truncate an existing `TICKET-1.md`).
///
/// # Errors
///
/// Returns [`std::io::Error`] when the tickets directory cannot be read.
pub fn next_ticket_number(root: &Path) -> std::io::Result<Option<u32>> {
    let dir = tickets_dir(root);
    let entries = fs::read_dir(&dir)?;
    let mut max = None;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let Some(rest) = name.strip_prefix("TICKET-") else {
            continue;
        };
        let Some(ext) = rest.strip_suffix(".md") else {
            continue;
        };
        let Some(digits) = ext.split(|c: char| !c.is_ascii_digit()).next() else {
            continue;
        };
        let Ok(n) = digits.parse::<u32>() else {
            continue;
        };
        if max.is_none_or(|m| n > m) {
            max = Some(n);
        }
    }
    Ok(max)
}

/// True when `id`'s remainder (past the numeric prefix) is path-safe:
/// only alphanumerics and dashes — no `/`, `\\`, `.`, `:`, or `..`
/// (a caller-supplied suffixed id must never escape `tickets/`).
fn id_suffix_is_path_safe(id: &str, prefix_len: usize) -> bool {
    id[prefix_len..]
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Load one ticket by id (`TICKET-7` or `7`), searching `tickets/`.
///
/// # Errors
///
/// Returns [`TicketError::Io`] on filesystem failure or
/// [`TicketError::Parse`]/[`TicketError::Invalid`] for bad files.
pub fn find_ticket(root: &Path, id: &str) -> Result<Ticket, TicketError> {
    let digits = id
        .strip_prefix("TICKET-")
        .or_else(|| id.strip_prefix("ticket-"))
        .unwrap_or(id);
    // Numeric prefix only — never paths (no traversal); lookups stay inside
    // tickets/. A suffix (TICKET-001-v2) resolves via prefix scan.
    let prefix: String = digits.chars().take_while(char::is_ascii_digit).collect();
    if prefix.is_empty() {
        return Err(TicketError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid ticket id '{id}': expected TICKET-<number>"),
        )));
    }
    let dir = tickets_dir(root);
    // Path-safe suffix: a caller-supplied id (`TICKET-1-/../../x.md`)
    // must never escape `tickets/` via the join below.
    if !id_suffix_is_path_safe(digits, prefix.len()) {
        return Err(TicketError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid ticket id '{id}': suffix must be path-safe"),
        )));
    }
    // Prefer the EXACT id: `TICKET-006-v2` must resolve `TICKET-006-v2.md`
    // first — resolving `TICKET-006.md` when both exist would alias to the
    // wrong ticket (claim/close/graph disagree on identity). The plain
    // `TICKET-<digits>.md` is the fallback for a numeric-only id.
    let suffix = digits[prefix.len()..].trim_start_matches('-');
    let exact = if suffix.is_empty() {
        None
    } else {
        let p = dir.join(format!("TICKET-{prefix}-{suffix}.md"));
        p.is_file().then_some(p)
    };
    let path = if let Some(p) = exact {
        p
    } else {
        let mut path = dir.join(format!("TICKET-{prefix}.md"));
        if !path.is_file() {
            // SORTED candidates, matching resolve_dep's documented
            // "first in sorted order" — read_dir order is filesystem
            // dependent and would make claim/close disagree with the
            // graph validator on the same repo.
            let mut cands: Vec<PathBuf> = dir
                .read_dir()
                .map_err(TicketError::Io)?
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().is_some_and(|e| e == "md")
                        && p.file_name().is_some_and(|n| {
                            let n = n.to_string_lossy();
                            n.starts_with(&format!("TICKET-{prefix}-"))
                        })
                })
                .collect();
            cands.sort();
            path = cands.into_iter().next().ok_or_else(|| {
                TicketError::Io(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no ticket {id} in {}", dir.display()),
                ))
            })?;
        }
        path
    };
    let mut t = load_ticket(&path)?;
    t.path = path;
    Ok(t)
}

/// Load and validate a ticket file (JSON or markdown frontmatter).
///
/// # Errors
///
/// Returns [`TicketError::Parse`] for unreadable/unparseable files or
/// [`TicketError::Invalid`] when the ticket fails the contract.
pub fn load_ticket(path: &Path) -> Result<Ticket, TicketError> {
    let text = fs::read_to_string(path).map_err(TicketError::Io)?;
    parse_ticket(&text).map_err(|e| match e {
        TicketError::Parse(m) => TicketError::Parse(format!("{}: {m}", path.display())),
        other => other,
    })
}

/// Parse ticket text (JSON or markdown frontmatter) and validate it.
///
/// # Errors
///
/// Returns [`TicketError::Parse`] for unparseable content or
/// [`TicketError::Invalid`] when the contract is violated.
pub fn parse_ticket(text: &str) -> Result<Ticket, TicketError> {
    let trimmed = text.trim();
    let ticket = if trimmed.starts_with('{') {
        let value: serde_json::Value =
            serde_json::from_str(trimmed).map_err(|e| TicketError::Parse(e.to_string()))?;
        serde_json::from_value(value).map_err(|e| TicketError::Parse(e.to_string()))?
    } else {
        // Markdown tickets keep the PoC bullet/frontmatter forms where
        // scope is OPTIONAL (deliberate: `parse_bullet_ticket` documents
        // "Scope is optional in markdown tickets") — do not apply the
        // JSON contract's required-scope rule here.
        parse_markdown_ticket(trimmed)?
    };
    if !ticket.id.starts_with("TICKET-") {
        return Err(TicketError::Invalid(format!(
            "id '{}' must match ^TICKET-[0-9]+",
            ticket.id
        )));
    }
    // Contract pattern is `^TICKET-[0-9]+` via re.search: at least one
    // digit must follow the dash; a suffix (e.g. `TICKET-001-v2`) is
    // allowed, a non-digit like `TICKET-x` is not.
    let suffix = &ticket.id["TICKET-".len()..];
    if !suffix.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(TicketError::Invalid(format!(
            "id '{}' must match ^TICKET-[0-9]+",
            ticket.id
        )));
    }
    // PATH-SAFE id: the suffix may carry `-<name>` but NEVER a path
    // separator, `..`, or any non-alphanumeric/dash char. A repo-resident
    // ticket with `id: TICKET-1/../../tmp/x` would otherwise flow into
    // `find_ticket`'s join and `write_spec`'s artifacts path — an
    // arbitrary write outside `artifacts/` (traversal sink).
    if !ticket
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(TicketError::Invalid(format!(
            "id '{}' must be path-safe (^TICKET-[0-9][A-Za-z0-9-]*$)",
            ticket.id
        )));
    }
    Ok(ticket)
}

fn parse_markdown_ticket(text: &str) -> Result<Ticket, TicketError> {
    if text.starts_with("---") {
        return parse_frontmatter_ticket(text);
    }
    parse_bullet_ticket(text)
}

/// Frontmatter form (`id:`/`title:`/`goal:`/`scope:` list).
fn parse_frontmatter_ticket(text: &str) -> Result<Ticket, TicketError> {
    let rest = text
        .strip_prefix("---")
        .ok_or_else(|| TicketError::Parse("expected --- frontmatter or JSON".into()))?;
    let block = rest
        .find("\n---")
        .ok_or_else(|| TicketError::Parse("missing closing ---".into()))?;
    let frontmatter = &rest[..block];
    let mut fields = std::collections::HashMap::new();
    let mut scope_items: Vec<String> = Vec::new();
    let mut blocked_items: Vec<String> = Vec::new();
    let mut in_scope_list = false;
    let mut in_blocked_list = false;
    for line in frontmatter.lines() {
        if in_scope_list {
            if let Some(item) = line.trim_start().strip_prefix("- ") {
                let item = item.trim().to_string();
                if !item.is_empty() {
                    scope_items.push(item);
                }
                continue;
            }
            in_scope_list = false;
        }
        if in_blocked_list {
            if let Some(item) = line.trim_start().strip_prefix("- ") {
                let item = item.trim().to_string();
                if !item.is_empty() {
                    blocked_items.push(item);
                }
                continue;
            }
            in_blocked_list = false;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim().to_string();
        let v = v.trim().to_string();
        if k == "scope" && v.is_empty() {
            in_scope_list = true;
            continue;
        }
        if k == "blocked_by" && v.is_empty() {
            in_blocked_list = true;
            continue;
        }
        fields.insert(k, v);
    }
    let id = fields
        .get("id")
        .ok_or_else(|| TicketError::Parse("missing frontmatter id".into()))?;
    let title = fields
        .get("title")
        .ok_or_else(|| TicketError::Parse("missing frontmatter title".into()))?;
    let goal = fields
        .get("goal")
        .ok_or_else(|| TicketError::Parse("missing frontmatter goal".into()))?;
    let scope = fields
        .get("scope")
        .map(|s| {
            s.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or(scope_items);
    let blocked_by = fields
        .get("blocked_by")
        .map(|s| {
            s.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or(blocked_items);
    let status = fields
        .get("status")
        .map_or_else(|| "OPEN".to_string(), |s| s.trim().to_uppercase());
    Ok(Ticket {
        id: id.clone(),
        title: title.clone(),
        goal: goal.clone(),
        scope,
        blocked_by,
        status,
        path: PathBuf::new(),
    })
}

/// `PoC` bullet form (`- id:`, `- title:`, `- goal (...):`, `- scope:` or a
/// `- scope-exceptions:` block). Scope is optional in markdown tickets.
fn parse_bullet_ticket(text: &str) -> Result<Ticket, TicketError> {
    let mut id = None;
    let mut title = None;
    let mut goal = None;
    let mut scope: Vec<String> = Vec::new();
    let mut blocked_by: Vec<String> = Vec::new();
    let mut status: Option<String> = None;
    let mut in_scope_block = false;
    for line in text.lines() {
        let line = line.trim();
        if in_scope_block {
            if let Some(item) = line.strip_prefix("- ") {
                let item = item.split(" (").next().unwrap_or("").trim().to_string();
                if !item.is_empty() {
                    scope.push(item);
                }
                continue;
            }
            in_scope_block = false;
        }
        let Some(rest) = line.strip_prefix("- ") else {
            continue;
        };
        let Some((k, v)) = rest.split_once(':') else {
            continue;
        };
        let k = k.trim().to_lowercase();
        let v = v.trim().to_string();
        if k.starts_with("scope-exceptions") {
            // Header with empty value starts the block; a value on the same
            // line is honored as inline scope. Must be checked BEFORE the
            // empty-value guard, or every block would be skipped.
            if v.is_empty() {
                in_scope_block = true;
                continue;
            }
            in_scope_block = false;
        }
        if v.is_empty() {
            continue;
        }
        if k == "id" {
            id = Some(v);
        } else if k == "title" {
            title = Some(v);
        } else if k.starts_with("goal") {
            goal = Some(v);
        } else if k == "scope" {
            scope = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        } else if k == "blocked_by" {
            blocked_by = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        } else if k == "status" {
            // Also matched from plain body lines (`Status: CLOSED` form).
            if status.is_none() {
                status = v.split_whitespace().next().map(str::to_uppercase);
            }
        }
    }
    if status.is_none() {
        // Plain body scan: `Status: CLOSED` lines (no dash prefix).
        for line in text.lines() {
            let line = line.trim();
            if let Some(v) = line
                .strip_prefix("Status:")
                .or_else(|| line.strip_prefix("status:"))
            {
                let v = v.trim();
                if !v.is_empty() {
                    status = v.split_whitespace().next().map(str::to_uppercase);
                    break;
                }
            }
        }
    }
    Ok(Ticket {
        id: id.ok_or_else(|| TicketError::Parse("missing - id: line".into()))?,
        title: title.ok_or_else(|| TicketError::Parse("missing - title: line".into()))?,
        goal: goal.ok_or_else(|| TicketError::Parse("missing - goal line".into()))?,
        scope,
        blocked_by,
        status: status.unwrap_or_else(|| "OPEN".to_string()),
        path: PathBuf::new(),
    })
}

/// Claims registry path (ADR-0008): `tickets/claims.md`, written only by
/// `claim_ticket`/`release_ticket`, never hand-edited.
#[must_use]
pub fn claims_path(root: &Path) -> PathBuf {
    root.join("tickets/claims.md")
}

const LOCK_STALE_SECS: u64 = 30;
const LOCK_MAX_WAIT_MS: u64 = 10_000;

/// Exclusive lock over the claims registry: `O_EXCL` create + retry +
/// stale-steal, so parallel agents cannot lose a lease (`TOCTOU`
/// review finding). Removed on drop.
#[derive(Debug)]
pub struct ClaimsLock {
    path: PathBuf,
    /// Unique ownership token written into the lock file at creation.
    /// `drop` removes the file ONLY if it still carries OUR token — after
    /// a stale-steal, a slow original holder must not delete the stealing
    /// waiter's live lock (that would let two writers proceed).
    token: String,
}

impl Drop for ClaimsLock {
    fn drop(&mut self) {
        if fs::read_to_string(&self.path).is_ok_and(|c| c.trim() == self.token) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn lock_token() -> String {
    format!(
        "{}:{}:{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
        crate::worker::worker_token()
    )
}

/// Acquire the claims lock, retrying until `LOCK_MAX_WAIT_MS`; a stale
/// lock (mtime older than `LOCK_STALE_SECS`) is stolen.
///
/// # Errors
///
/// Returns an io error when the lock cannot be acquired in time.
pub fn lock_claims(root: &Path) -> io::Result<ClaimsLock> {
    lock_file(&root.join("tickets/.claims.lock"), LOCK_STALE_SECS)
}

/// Exclusive lock over an ARBITRARY lock file (claims or harness) with a
/// caller-chosen stale threshold: `O_EXCL` create + atomic rename-steal +
/// ownership token, so parallel agents cannot lose a lease and a slow
/// holder cannot be double-stolen. The harness uses a LONGER threshold
/// because its critical section spans a capped gate run.
///
/// # Errors
///
/// Returns an io error when the lock cannot be acquired in time.
pub(crate) fn lock_file(path: &Path, stale_secs: u64) -> io::Result<ClaimsLock> {
    let path = path.to_path_buf();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let started = std::time::Instant::now();
    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => {
                // Write OUR ownership token into the lock file so `drop`
                // can verify it still holds OUR lock.
                let token = lock_token();
                if let Err(e) = fs::write(&path, &token) {
                    // The lock file is now unowned + tokenless — remove it
                    // so no one is denied for the stale threshold.
                    let _ = fs::remove_file(&path);
                    return Err(e);
                }
                return Ok(ClaimsLock { path, token });
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&path).is_ok_and(|m| {
                    m.modified().is_ok_and(|mt| {
                        // A FUTURE mtime (clock stepped back) is NOT
                        // stale — a fresh lock must not be stolen.
                        mt.elapsed().is_ok_and(|d| d.as_secs() > stale_secs)
                    })
                });
                if stale {
                    // Atomic steal: RENAME the stale lock to a unique name
                    // instead of remove_file + create_new. Two waiters that
                    // both observe a stale lock must not remove each other's
                    // FRESH lock: only one `rename` of the same inode can
                    // succeed; the loser retries and then waits on the
                    // winner's new lock (or steals a genuinely stale one).
                    // The renamed inode is discarded best-effort.
                    // PID + a monotonic counter: two steals in one process
                    // must not overwrite each other's steal file.
                    static STEAL_SEQ: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(0);
                    let seq = STEAL_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let steal =
                        path.with_extension(format!("lock.steal.{}.{seq}", std::process::id()));
                    if fs::rename(&path, &steal).is_ok() {
                        // POST-RENAME verify: between observing "stale"
                        // and the rename, the original holder may have
                        // released (drop) and another waiter acquired a
                        // FRESH lock — our rename then moved a LIVE lock
                        // aside. If the renamed inode is NOT stale, put it
                        // back and wait, so no double-holder arises.
                        let still_stale = fs::metadata(&steal).is_ok_and(|m| {
                            m.modified().is_ok_and(|mt| {
                                mt.elapsed().is_ok_and(|d| d.as_secs() > stale_secs)
                            })
                        });
                        if still_stale {
                            let _ = fs::remove_file(&steal);
                            continue;
                        }
                        if fs::metadata(&steal).is_err() {
                            // The inode vanished between rename and metadata
                            // (the original holder's drop removed it): it is
                            // NEITHER stale nor live — discard the orphaned
                            // steal file and retry (do NOT run the "fresh
                            // lock, put it back" branch on an error).
                            let _ = fs::remove_file(&steal);
                            continue;
                        }
                        // The renamed inode is a FRESH lock (stolen mid-
                        // window) — put it back and wait on it. Use
                        // hard_link (fails if `path` was re-taken): a
                        // plain rename would OVERWRITE a concurrently
                        // created fresh lock, leaving its holder running
                        // without an exclusive lock file.
                        if fs::hard_link(&steal, &path).is_ok() {
                            let _ = fs::remove_file(&steal);
                        }
                        // hard_link failed: the path was re-taken. This
                        // inode is a LIVE holder's lock — NEVER delete it
                        // (that would leave its critical section
                        // unprotected while the path holder proceeds).
                        // Leave it as an orphaned steal file for manual
                        // recovery instead.
                    } else {
                        // Another waiter stole it first — retry the loop.
                        continue;
                    }
                }
                if started.elapsed().as_millis() > LOCK_MAX_WAIT_MS.into() {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        format!("lock {} busy (held > {LOCK_MAX_WAIT_MS}ms)", path.display()),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(e),
        }
    }
}

/// One held claim: a lease on a ticket by a named claimant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claim {
    /// `TICKET-<n>` id (unique in the registry).
    pub ticket: String,
    /// Claimant identity (session, agent name, or human).
    pub claimant: String,
    /// ISO timestamp of the claim.
    pub since: String,
}

/// Read the claims registry; an absent file is an empty registry.
///
/// # Errors
///
/// Returns the underlying filesystem error on unreadable files.
pub fn read_claims(root: &Path) -> io::Result<Vec<Claim>> {
    let text = match fs::read_to_string(claims_path(root)) {
        Ok(t) => t,
        // Absent registry = empty (the first claim creates it). An
        // EXISTING but unreadable/corrupt registry is a hard error — it
        // must never be silently treated as empty (the next write would
        // erase every lease).
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        // A corrupt line is a HARD error, never a silently dropped lease
        // — the next write would rewrite the registry without it.
        let claim = serde_json::from_str::<Claim>(line).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("corrupt claims line: {e}"),
            )
        })?;
        out.push(claim);
    }
    Ok(out)
}

/// Claim a ticket (lease semantics, ADR-0008). Fails when the ticket is
/// already claimed by another claimant, or (unless `force`) when the
/// ticket has unresolved `blocked_by` deps.
///
/// # Errors
///
/// Returns [`TicketError::Invalid`] for lease violations or
/// [`TicketError::Io`] for registry failures.
pub fn claim_ticket(
    root: &Path,
    id: &str,
    claimant: &str,
    force: bool,
) -> Result<Claim, TicketError> {
    let ticket = find_ticket(root, id)?;
    let _lock = lock_claims(root).map_err(TicketError::Io)?;
    let mut claims = read_claims(root).map_err(TicketError::Io)?;
    if let Some(existing) = claims.iter().find(|c| c.ticket == ticket.id) {
        if existing.claimant != claimant {
            return Err(TicketError::Invalid(format!(
                "ticket {} already claimed by {} since {}",
                ticket.id, existing.claimant, existing.since
            )));
        }
        return Ok(existing.clone());
    }
    if !force {
        for dep in &ticket.blocked_by {
            let dep_ticket = find_ticket(root, dep)?;
            if dep_ticket.status != "CLOSED" {
                return Err(TicketError::Invalid(format!(
                    "ticket {} is blocked by {} (status {}) — close the dependency first or --force",
                    ticket.id, dep, dep_ticket.status
                )));
            }
        }
    }
    let claim = Claim {
        ticket: ticket.id,
        claimant: claimant.to_string(),
        since: crate::memory::utc_now_stamp(),
    };
    claims.push(claim.clone());
    claims.sort_by(|a, b| a.ticket.cmp(&b.ticket));
    write_claims(root, &claims).map_err(TicketError::Io)?;
    Ok(claim)
}

/// Release a claim; releasing a ticket you do not hold fails.
///
/// # Errors
///
/// Returns [`TicketError::Invalid`] for lease violations or
/// [`TicketError::Io`] for registry failures.
pub fn release_ticket(root: &Path, id: &str, claimant: &str) -> Result<(), TicketError> {
    let _lock = lock_claims(root).map_err(TicketError::Io)?;
    release_ticket_locked(root, id, claimant)
}

/// `release_ticket` for a caller that ALREADY holds the claims lock
/// (the loop's atomic close and the exhaustion mark must not re-acquire
/// the `O_EXCL` lock — a nested acquire would block then time out).
///
/// # Errors
///
/// Returns [`TicketError::Invalid`] for lease violations or
/// [`TicketError::Io`] for registry failures.
pub fn release_ticket_locked(root: &Path, id: &str, claimant: &str) -> Result<(), TicketError> {
    let ticket = find_ticket(root, id)?;
    let mut claims = read_claims(root).map_err(TicketError::Io)?;
    let Some(pos) = claims.iter().position(|c| c.ticket == ticket.id) else {
        return Err(TicketError::Invalid(format!(
            "ticket {} is not claimed",
            ticket.id
        )));
    };
    if claims[pos].claimant != claimant {
        return Err(TicketError::Invalid(format!(
            "ticket {} is claimed by {}, not {claimant}",
            ticket.id, claims[pos].claimant
        )));
    }
    claims.remove(pos);
    write_claims(root, &claims).map_err(TicketError::Io)?;
    Ok(())
}

/// Unique temp suffix so a crash mid-write cannot leave a stray tmp that
/// permanently blocks the next write (`create_new` on a fixed tmp name
/// would fail with `AlreadyExists` forever).
#[must_use]
pub fn tmp_unique(base: &Path, tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let name = format!(
        "{}.{tag}.{}.{nanos}.tmp",
        base.file_name().map_or("x", |s| s.to_str().unwrap_or("x")),
        std::process::id()
    );
    base.with_file_name(name)
}

fn write_claims(root: &Path, claims: &[Claim]) -> io::Result<()> {
    let path = claims_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = String::from(
        "# CLAIMS REGISTRY (ADR-0008 — written by `ticket claim`/`ticket release`, never hand-edit)\n",
    );
    for claim in claims {
        let line = serde_json::to_string(claim).map_err(io::Error::other)?;
        out.push_str(&line);
        out.push('\n');
    }
    // Atomic (temp + rename): a crash mid-write must not leave a
    // truncated registry — every live lease would silently vanish and
    // tickets become double-claimable. Matches the ledger/ticket-status
    // write discipline.
    let tmp = tmp_unique(&path, "claims");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| std::io::Write::write_all(&mut f, out.as_bytes()))?;
    sync_then_rename(&tmp, &path)
}

/// Rename after syncing the temp file — a crash OR power loss must not
/// deliver an empty destination (ext4 delayed allocation can zero a file
/// renamed without fsync).
///
/// # Errors
///
/// Returns the underlying io error on sync or rename failure.
pub fn sync_then_rename(tmp: &Path, dest: &Path) -> io::Result<()> {
    fs::File::open(tmp)?.sync_all()?;
    fs::rename(tmp, dest)
}

/// Rewrite the claims registry with an explicit set.
///
/// Callers MUST already hold the claims lock — used by the loop's atomic
/// close (one lock for ledger + claims + ticket status).
///
/// # Errors
///
/// Returns the underlying filesystem error on write failure.
pub fn write_claims_registry(root: &Path, claims: &[Claim]) -> io::Result<()> {
    write_claims(root, claims)
}

/// Set a ticket's lifecycle status (`OPEN`/`CLOSED`/...), rewriting the
/// ticket file atomically (temp + rename).
///
/// Callers MUST already hold the claims lock so the status change is part
/// of one atomic close transaction.
///
/// # Errors
///
/// Returns [`TicketError::Io`] on filesystem failure or
/// [`TicketError::Parse`] when the ticket file cannot be read.
///
/// Append a note line to a ticket file (under the caller's claims lock).
/// Used by the loop's exhaustion mark ("ticket left OPEN with a note",
/// ARCHITECTURE-CONDENSED 5.2).
///
/// # Errors
///
/// Returns [`TicketError::Io`] on filesystem failure.
pub fn append_ticket_note(root: &Path, id: &str, note: &str) -> Result<(), TicketError> {
    // Append to the EXACT file find_ticket loaded (Ticket.path): a
    // name/id mismatch (TICKET-006.md containing id TICKET-006-v2) must
    // not route the note to a different file.
    let t = find_ticket(root, id)?;
    let path = t.path;
    // Flatten the note: a newline would inject a `- blocked_by:`/`- status:`
    // line into the ticket file.
    let note = note.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = fs::read_to_string(&path).map_err(TicketError::Io)?;
    if text.trim_start().starts_with('{') {
        // JSON ticket: append a `note` field (a `- note:` line would make
        // the file invalid JSON and the ticket vanish from list_tickets).
        let mut value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| TicketError::Parse(e.to_string()))?;
        value["note"] = serde_json::Value::String(note);
        let tmp = tmp_unique(&path, "note");
        fs::write(
            &tmp,
            serde_json::to_string_pretty(&value).unwrap_or_default(),
        )
        .map_err(TicketError::Io)?;
        return sync_then_rename(&tmp, &path).map_err(TicketError::Io);
    }
    // Atomic temp+rename, same as the JSON branch (caller holds the
    // claims lock, but a crash mid-`append(true)` could leave a partial
    // `- note:` line; the rewritten file cannot).
    let updated = format!("{text}\n- note: {note}\n");
    let tmp = tmp_unique(&path, "note");
    fs::write(&tmp, updated).map_err(TicketError::Io)?;
    sync_then_rename(&tmp, &path).map_err(TicketError::Io)
}

/// Set a ticket's lifecycle status (`OPEN`/`CLOSED`/...), rewriting the
/// ticket file atomically (temp + rename).
///
/// Callers MUST already hold the claims lock so the status change is part
/// of one atomic close transaction.
///
/// # Errors
///
/// Returns [`TicketError::Io`] on filesystem failure or
/// [`TicketError::Parse`] when the ticket file cannot be read.
/// Locate the status line to overwrite. For a FRONTMATTER ticket the scan
/// stays INSIDE the `---` block (a `- status:` bullet in the BODY is not
/// a frontmatter key — overwriting it would never be re-read).
fn find_status_line(text: &str, frontmatter: bool) -> Option<usize> {
    let lines: Vec<&str> = text.lines().collect();
    for (i, raw) in lines.iter().enumerate() {
        let l = raw.trim();
        if frontmatter {
            if l.starts_with("---") && i > 0 {
                return None; // past the frontmatter block
            }
            if l.starts_with("status:") {
                return Some(i);
            }
        } else if l.starts_with("- status:") {
            // ONLY the bullet form the parser reads: a plain `status:`
            // body line is invisible to parse_bullet_ticket, so rewriting
            // it would close the ledger while the ticket stays OPEN (a
            // dead close).
            return Some(i);
        }
    }
    None
}

/// Set a ticket's lifecycle status (`OPEN`/`CLOSED`/...), rewriting the
/// ticket file atomically (temp + rename).
///
/// Callers MUST already hold the claims lock so the status change is part
/// of one atomic close transaction.
///
/// # Errors
///
/// Returns [`TicketError::Io`] on filesystem failure or
/// [`TicketError::Parse`] when the ticket file cannot be read.
pub fn set_ticket_status(root: &Path, id: &str, status: &str) -> Result<(), TicketError> {
    // Same traversal guard as find_ticket: only `TICKET-<digits>` ids
    // (a suffix like `TICKET-001-v2` resolves via prefix scan); a
    // caller-supplied id can never escape `tickets/`.
    let digits = id
        .strip_prefix("TICKET-")
        .or_else(|| id.strip_prefix("ticket-"))
        .unwrap_or(id);
    let prefix: String = digits.chars().take_while(char::is_ascii_digit).collect();
    if prefix.is_empty() {
        return Err(TicketError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid ticket id '{id}': expected TICKET-<number>"),
        )));
    }
    // Resolve the EXACT file first (parity with find_ticket): a suffixed
    // id must update `TICKET-006-v2.md`, never a plain twin that happens
    // to exist — the close transaction would otherwise mark the wrong
    // ticket while the claimed one stays OPEN.
    if !id_suffix_is_path_safe(digits, prefix.len()) {
        return Err(TicketError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid ticket id '{id}': suffix must be path-safe"),
        )));
    }
    let t = find_ticket(root, id)?;
    let path = t.path;
    let text = fs::read_to_string(&path).map_err(TicketError::Io)?;
    let frontmatter = text.trim_start().starts_with("---");
    let status_line = if frontmatter {
        format!("status: {status}")
    } else {
        format!("- status: {status}")
    };
    let updated = if text.trim_start().starts_with('{') {
        let mut value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| TicketError::Parse(e.to_string()))?;
        value["status"] = serde_json::Value::String(status.to_string());
        serde_json::to_string_pretty(&value).map_err(|e| TicketError::Parse(e.to_string()))?
    } else if let Some(pos) = find_status_line(&text, frontmatter) {
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        lines[pos] = status_line;
        lines.join("\n")
    } else if frontmatter {
        // Frontmatter keys live INSIDE the `---` block: appending outside
        // would leave the key unreadable (`- status:` parses as a
        // different key). Insert before the closing `---`.
        let close = text.find("\n---").map_or(text.len(), |i| i + 1);
        format!("{}{}\n{}", &text[..close], status_line, &text[close..])
    } else {
        format!("{text}\n{status_line}\n")
    };
    let tmp = tmp_unique(&path, "status");
    fs::write(&tmp, updated).map_err(TicketError::Io)?;
    sync_then_rename(&tmp, &path).map_err(TicketError::Io)
}

/// Resolve a `blocked_by` reference to a ticket id deterministically:
/// exact `TICKET-<digits>` first, then the first `TICKET-<digits>-...`
/// suffix in sorted order (`find_ticket` semantics; a bare prefix match
/// would resolve `TICKET-1` to `TICKET-10` — review finding).
fn resolve_dep<'a>(tickets: &'a [Ticket], dep: &str) -> Option<&'a str> {
    let dep_id = dep
        .strip_prefix("TICKET-")
        .or_else(|| dep.strip_prefix("ticket-"))
        .unwrap_or(dep);
    let digits: String = dep_id.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let exact = format!("TICKET-{digits}");
    if let Some(t) = tickets.iter().find(|t| t.id == exact) {
        return Some(t.id.as_str());
    }
    let suffix = format!("TICKET-{digits}-");
    tickets
        .iter()
        .find(|t| t.id.starts_with(&suffix))
        .map(|t| t.id.as_str())
}

/// Validate the dependency graph: every `blocked_by` id must resolve and
/// the graph must be acyclic. Returns one message per problem.
///
/// # Errors
///
/// Returns [`TicketError::Io`] when the tickets directory cannot be read.
pub fn validate_graph(root: &Path) -> Result<Vec<String>, TicketError> {
    let tickets = list_tickets(root)?;
    let mut problems = Vec::new();
    for ticket in &tickets {
        for dep in &ticket.blocked_by {
            let resolved = resolve_dep(&tickets, dep);
            match resolved {
                None => problems.push(format!(
                    "{}: blocked_by {} does not resolve to any ticket",
                    ticket.id, dep
                )),
                Some(dep_full) if dep_full == ticket.id => {
                    problems.push(format!("{}: blocked_by itself (self-cycle)", ticket.id));
                }
                _ => {}
            }
        }
    }
    // DFS cycle detection over the resolved edge set.
    let mut edges: Vec<(String, String)> = Vec::new();
    for t in &tickets {
        for dep in &t.blocked_by {
            if let Some(dep_full) = resolve_dep(&tickets, dep) {
                edges.push((dep_full.to_string(), t.id.clone()));
            }
        }
    }
    let mut adj: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for (from, to) in &edges {
        adj.entry(from.as_str()).or_default().push(to.as_str());
    }
    for ticket in &tickets {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![ticket.id.as_str()];
        while let Some(node) = stack.pop() {
            if !visited.insert(node) {
                continue;
            }
            for next in adj.get(node).into_iter().flatten() {
                if *next == ticket.id.as_str() {
                    problems.push(format!("{}: dependency cycle detected", ticket.id));
                    break;
                }
                stack.push(next);
            }
        }
    }
    problems.sort();
    problems.dedup();
    Ok(problems)
}

#[cfg(test)]
mod lock_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn lock_claims_steals_a_stale_lock_atomically() {
        let root = std::env::temp_dir().join(format!("mag-lock-race-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tickets")).unwrap();
        // Force steal contention: a STALE lock file.
        let lock_path = root.join("tickets/.claims.lock");
        fs::write(&lock_path, b"").unwrap();
        let old =
            std::time::SystemTime::now() - std::time::Duration::from_secs(LOCK_STALE_SECS * 4);
        fs::File::open(&lock_path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
        // 8 threads contend on the stale lock; mutual exclusion must hold:
        // at most ONE holder at a time (a remove+create steal lets two
        // holders overlap and trips the shared counter).
        let held = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let root = Arc::new(root);
        let mut handles = Vec::new();
        for _ in 0..8 {
            let root = root.clone();
            let held = held.clone();
            let max_seen = max_seen.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..5 {
                    let _l = lock_claims(&root).unwrap();
                    let n = held.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(n, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    held.fetch_sub(1, Ordering::SeqCst);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(
            max_seen.load(Ordering::SeqCst) <= 1,
            "two holders overlapped: {} concurrent",
            max_seen.load(Ordering::SeqCst)
        );
        assert!(!lock_path.exists(), "no lock file survives");
        let _ = fs::remove_dir_all(&*root);
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;
    use std::fs;

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("mag-tkt-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tickets")).unwrap();
        root
    }

    #[test]
    fn bullet_ticket_with_a_plain_status_body_line_does_not_dead_close() {
        // A bullet ticket carrying BOTH a plain `status: OPEN` body line
        // (which parse_bullet_ticket never reads) and no `- status:`
        // bullet: the close must land on a line the parser reads, else
        // the ledger closes while the ticket stays OPEN.
        let root = tmp_root("deadclose");
        let path = root.join("tickets/TICKET-1.md");
        fs::write(
            &path,
            "- id: TICKET-1\n- title: t\nstatus: OPEN\n- goal: g\n",
        )
        .unwrap();
        set_ticket_status(&root, "TICKET-1", "CLOSED").unwrap();
        let t = find_ticket(&root, "TICKET-1").unwrap();
        assert_eq!(
            t.status, "CLOSED",
            "the bullet path must close on a `- status:` line the parser reads"
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("- status: CLOSED"),
            "an appended `- status:` bullet must exist"
        );
        assert!(
            text.contains("status: OPEN"),
            "the plain body line is left untouched (parser ignores it)"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn frontmatter_ticket_status_roundtrips() {
        let root = tmp_root("fm");
        let path = root.join("tickets/TICKET-1.md");
        fs::write(&path, "---\nid: TICKET-1\ntitle: t\ngoal: g\n---\n").unwrap();
        set_ticket_status(&root, "TICKET-1", "CLOSED").unwrap();
        let t = find_ticket(&root, "TICKET-1").unwrap();
        assert_eq!(t.status, "CLOSED", "frontmatter status must re-parse");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ticket_ids_with_path_separators_are_rejected() {
        for bad in [
            "TICKET-1/../../evil",
            "TICKET-1/../x",
            "TICKET-1\\..\\x",
            "TICKET-1:..",
        ] {
            let text = format!("---\nid: {bad}\ntitle: t\ngoal: g\n---\n");
            assert!(
                parse_ticket(&text).is_err(),
                "traversal id {bad} must be rejected"
            );
        }
    }

    #[test]
    fn corrupt_claims_line_is_a_hard_error() {
        let root = tmp_root("cc");
        fs::write(root.join("tickets/claims.md"), "not-json-at-all\n").unwrap();
        assert!(
            read_claims(&root).is_err(),
            "a corrupt line must not parse as an empty registry"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
