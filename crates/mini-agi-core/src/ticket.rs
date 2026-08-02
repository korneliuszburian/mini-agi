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
    let mut name = format!("TICKET-{digits}.md");
    let dir = tickets_dir(root);
    if !dir.join(&name).is_file() {
        // id may already include the prefix
        name = format!("{digits}.md");
        if !dir.join(&name).is_file() {
            return Err(TicketError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("no ticket {id} in {}", dir.display()),
            )));
        }
    }
    load_ticket(&dir.join(name))
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
        crate::contract::validate_contract_value(crate::contract::Contract::Ticket, &value)
            .map_err(|e| TicketError::Invalid(e.to_string()))?;
        serde_json::from_value(value).map_err(|e| TicketError::Parse(e.to_string()))?
    } else {
        parse_markdown_ticket(trimmed)?
    };
    if !ticket.id.starts_with("TICKET-") {
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
    for line in frontmatter.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        fields.insert(k.trim().to_string(), v.trim().to_string());
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
        .unwrap_or_default();
    Ok(Ticket {
        id: id.clone(),
        title: title.clone(),
        goal: goal.clone(),
        scope,
    })
}

/// `PoC` bullet form (`- id:`, `- title:`, `- goal (...):`, `- scope:` or a
/// `- scope-exceptions:` block). Scope is optional in markdown tickets.
fn parse_bullet_ticket(text: &str) -> Result<Ticket, TicketError> {
    let mut id = None;
    let mut title = None;
    let mut goal = None;
    let mut scope: Vec<String> = Vec::new();
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
        } else if k.starts_with("scope-exceptions") && v.is_empty() {
            in_scope_block = true;
        }
    }
    Ok(Ticket {
        id: id.ok_or_else(|| TicketError::Parse("missing - id: line".into()))?,
        title: title.ok_or_else(|| TicketError::Parse("missing - title: line".into()))?,
        goal: goal.ok_or_else(|| TicketError::Parse("missing - goal line".into()))?,
        scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON_TICKET: &str =
        r#"{"id":"TICKET-001","title":"gates","goal":"wire gates","scope":["scripts/","crates/"]}"#;

    const MD_TICKET: &str = r"---
id: TICKET-002
title: memory derive
goal: regenerate derived views
scope: memory/derived, scripts
---

# TICKET-002 — memory derive

Body.
";

    #[test]
    fn parses_json_ticket() {
        let ticket = parse_ticket(JSON_TICKET).unwrap();
        assert_eq!(ticket.id, "TICKET-001");
        assert_eq!(ticket.title, "gates");
        assert_eq!(ticket.scope, vec!["scripts/", "crates/"]);
    }

    #[test]
    fn parses_markdown_ticket() {
        let ticket = parse_ticket(MD_TICKET).unwrap();
        assert_eq!(ticket.id, "TICKET-002");
        assert_eq!(ticket.scope, vec!["memory/derived", "scripts"]);
    }

    #[test]
    fn rejects_ticket_without_scope() {
        let err =
            parse_ticket(r#"{"id":"TICKET-003","title":"t","goal":"g","scope":[]}"#).unwrap_err();
        assert!(matches!(err, TicketError::Invalid(_)));
    }

    #[test]
    fn rejects_bad_id() {
        let err =
            parse_ticket(r#"{"id":"FOO-1","title":"t","goal":"g","scope":["x"]}"#).unwrap_err();
        assert!(matches!(err, TicketError::Invalid(_)));
    }

    #[test]
    fn rejects_garbage() {
        let err = parse_ticket("this is not a ticket").unwrap_err();
        assert!(matches!(err, TicketError::Parse(_)));
    }

    #[test]
    fn discovers_and_finds_tickets() {
        let root = std::env::temp_dir().join(format!("mag-ticket-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("tickets")).unwrap();
        fs::write(root.join("tickets/TICKET-001.md"), JSON_TICKET).unwrap();
        fs::write(root.join("tickets/TICKET-002.md"), MD_TICKET).unwrap();
        fs::write(root.join("tickets/README.md"), "not a ticket").unwrap();
        let all = list_tickets(&root).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "TICKET-001");
        assert_eq!(all[1].id, "TICKET-002");
        let found = find_ticket(&root, "TICKET-002").unwrap();
        assert_eq!(found.title, "memory derive");
        let by_number = find_ticket(&root, "001").unwrap();
        assert_eq!(by_number.id, "TICKET-001");
        let _ = fs::remove_dir_all(&root);
    }
}
