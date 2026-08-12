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
            && let Ok(ticket) = load_ticket(&path)
        {
            tickets.push(ticket);
        }
    }
    tickets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tickets)
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
    let mut path = dir.join(format!("TICKET-{prefix}.md"));
    if !path.is_file() {
        path = dir
            .read_dir()
            .map_err(TicketError::Io)?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.extension().is_some_and(|e| e == "md")
                    && p.file_name().is_some_and(|n| {
                        let n = n.to_string_lossy();
                        n.starts_with(&format!("TICKET-{prefix}-"))
                    })
            })
            .ok_or_else(|| {
                TicketError::Io(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no ticket {id} in {}", dir.display()),
                ))
            })?;
    }
    load_ticket(&path)
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
}

impl Drop for ClaimsLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Acquire the claims lock, retrying until `LOCK_MAX_WAIT_MS`; a stale
/// lock (mtime older than `LOCK_STALE_SECS`) is stolen.
///
/// # Errors
///
/// Returns an io error when the lock cannot be acquired in time.
pub fn lock_claims(root: &Path) -> io::Result<ClaimsLock> {
    let path = root.join("tickets/.claims.lock");
    let started = std::time::Instant::now();
    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Ok(ClaimsLock { path }),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&path).is_ok_and(|m| {
                    m.modified().is_ok_and(|mt| {
                        mt.elapsed().map_or(true, |d| d.as_secs() > LOCK_STALE_SECS)
                    })
                });
                if stale {
                    let _ = fs::remove_file(&path);
                    continue;
                }
                if started.elapsed().as_millis() > LOCK_MAX_WAIT_MS.into() {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        format!(
                            "claims lock {} busy (held > {LOCK_MAX_WAIT_MS}ms)",
                            path.display()
                        ),
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
    let text = fs::read_to_string(claims_path(root))?;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Ok(claim) = serde_json::from_str::<Claim>(line) {
            out.push(claim);
        }
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
    let mut claims = read_claims(root).unwrap_or_default();
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
    let ticket = find_ticket(root, id)?;
    let _lock = lock_claims(root).map_err(TicketError::Io)?;
    let mut claims = read_claims(root).unwrap_or_default();
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
    fs::write(&path, out)
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
pub fn set_ticket_status(root: &Path, id: &str, status: &str) -> Result<(), TicketError> {
    let dir = tickets_dir(root);
    let mut path = dir.join(format!("{id}.md"));
    if !path.is_file() {
        let found = dir
            .read_dir()
            .map_err(TicketError::Io)?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy();
                    n.starts_with(&format!("{id}-")) || n == format!("{id}.md")
                })
            });
        path = found.ok_or_else(|| {
            TicketError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no ticket file for {id}"),
            ))
        })?;
    }
    let text = fs::read_to_string(&path).map_err(TicketError::Io)?;
    let updated = if text.trim_start().starts_with('{') {
        let mut value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| TicketError::Parse(e.to_string()))?;
        value["status"] = serde_json::Value::String(status.to_string());
        serde_json::to_string_pretty(&value).map_err(|e| TicketError::Parse(e.to_string()))?
    } else if let Some(pos) = text.lines().position(|l| {
        let l = l.trim();
        l.starts_with("- status:") || l.starts_with("status:")
    }) {
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        lines[pos] = format!("- status: {status}");
        lines.join("\n")
    } else {
        format!("{text}\n- status: {status}\n")
    };
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, updated).map_err(TicketError::Io)?;
    fs::rename(&tmp, &path).map_err(TicketError::Io)
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
