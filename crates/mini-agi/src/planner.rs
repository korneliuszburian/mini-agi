//! Parallel-planner manifest + fail-closed validation (AFK v4).
//!
//! The planner pass (a read-only codex session) emits a STRICT versioned
//! JSON manifest; the kernel parses it FAIL-CLOSED — any violation
//! (unknown field, duplicate id, empty goal/scope, absolute or
//! traversing paths, protected paths in scope, overlapping scopes,
//! out-of-shape verifier) fails the whole batch before anything is
//! dispatched. This is deliberately NOT the tolerant review parser:
//! a review verdict is a recorded disposition, a dispatch manifest is
//! authority (second opinion, finding 3).

use std::path::Path;

/// Manifest schema version (bump on any breaking shape change).
pub const MANIFEST_VERSION: u32 = 1;

/// Paths the parallel template NEVER lets a ticket touch: the gate and
/// its inputs must stay immutable across a batch, or the final truth
/// could be self-modified (second opinion, finding 1).
pub const PROTECTED_PATHS: &[&str] = &[
    "scripts/verify.sh",
    "scripts/gate-lib.sh",
    "evals",
    "memory",
    "tickets",
    "docs/adr",
];

/// One ticket of a parallel batch.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlannerTicket {
    /// Unique ticket id (the merge order is the id order).
    pub id: String,
    /// The ticket goal (the worker prompt).
    pub goal: String,
    /// Repo-relative paths the ticket may touch (dirs or files),
    /// mutually disjoint across the batch.
    pub scope: Vec<String>,
    /// The deterministic verifier command, WORKTREE-RELATIVE (runs
    /// with cwd = the ticket worktree root).
    pub verify: String,
    /// Worktree-relative verify target (default ".").
    #[serde(default = "default_target")]
    pub verify_target: String,
}

fn default_target() -> String {
    ".".to_string()
}

/// The strict planner manifest.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlannerManifest {
    pub version: u32,
    pub tickets: Vec<PlannerTicket>,
}

/// A relative path is valid when it stays inside the repo: not
/// absolute, no drive letters, no `..`, no leading `~`, and never the
/// empty path.
fn valid_relative_path(p: &str) -> bool {
    if p.is_empty() || p.starts_with('/') || p.starts_with('~') || p.starts_with('\\') {
        return false;
    }
    if p.split(['/', '\\']).any(|seg| seg == "..") {
        return false;
    }
    if p.contains(':') {
        return false;
    }
    true
}

/// Verifier-shape allowlist (second opinion, finding 5): the command
/// must not reference absolute paths, home-relative paths, traversal,
/// or protected paths — it runs inside the ticket worktree only.
pub fn valid_verifier_shape(verify: &str) -> bool {
    if verify.trim().is_empty() {
        return false;
    }
    // Tokenize on whitespace and shell quotes; every token that looks
    // like a path must be relative and safe.
    for token in verify.split_whitespace() {
        let token = token.trim_matches(['\'', '"', '(', ')', ';', '&', '|', '$', '`', '>', '<']);
        if token.is_empty() {
            continue;
        }
        if token.starts_with('/') || token.starts_with('~') || token.starts_with("$HOME") {
            return false;
        }
        if token.split('/').any(|seg| seg == "..") {
            return false;
        }
        if PROTECTED_PATHS
            .iter()
            .any(|p| token == *p || token.starts_with(&format!("{p}/")))
        {
            return false;
        }
    }
    true
}

/// Parse + validate the planner manifest fail-closed.
///
/// # Errors
///
/// Returns the first violation as a message; the batch must NOT start.
pub fn parse_manifest(text: &str) -> Result<PlannerManifest, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("manifest is not valid JSON: {e}"))?;
    // Strict schema: reject unknown fields at the top level.
    let obj = value
        .as_object()
        .ok_or_else(|| "manifest must be a JSON object".to_string())?;
    for key in obj.keys() {
        if key != "version" && key != "tickets" {
            return Err(format!("unknown manifest field '{key}'"));
        }
    }
    let version = obj
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "manifest.version must be an integer".to_string())?;
    if version != MANIFEST_VERSION as u64 {
        return Err(format!(
            "manifest.version {version} != supported {MANIFEST_VERSION}"
        ));
    }
    let tickets = obj
        .get("tickets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "manifest.tickets must be an array".to_string())?;
    if tickets.is_empty() {
        return Err("manifest.tickets must not be empty".to_string());
    }
    if tickets.len() > 16 {
        return Err(format!(
            "manifest.tickets has {} tickets; the batch admission cap is 16",
            tickets.len()
        ));
    }
    let mut manifest = PlannerManifest {
        version: version as u32,
        tickets: Vec::new(),
    };
    let mut seen_ids = std::collections::HashSet::new();
    let mut all_scopes: Vec<(String, Vec<String>)> = Vec::new();
    for (i, t) in tickets.iter().enumerate() {
        let t = t
            .as_object()
            .ok_or_else(|| format!("ticket {i} must be an object"))?;
        for key in t.keys() {
            if !["id", "goal", "scope", "verify", "verify_target"].contains(&key.as_str()) {
                return Err(format!("ticket {i}: unknown field '{key}'"));
            }
        }
        let id = t
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("ticket {i}: id must be a string"))?;
        if id.trim().is_empty() {
            return Err(format!("ticket {i}: id must not be empty"));
        }
        if !seen_ids.insert(id.to_string()) {
            return Err(format!("duplicate ticket id '{id}'"));
        }
        let goal = t
            .get("goal")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("ticket {id}: goal must be a string"))?;
        if goal.trim().is_empty() {
            return Err(format!("ticket {id}: goal must not be empty"));
        }
        let scope = t
            .get("scope")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("ticket {id}: scope must be an array"))?;
        if scope.is_empty() {
            return Err(format!("ticket {id}: scope must not be empty"));
        }
        let mut scope_paths = Vec::new();
        for s in scope {
            let s = s
                .as_str()
                .ok_or_else(|| format!("ticket {id}: scope entries must be strings"))?;
            if !valid_relative_path(s) {
                return Err(format!(
                    "ticket {id}: scope entry '{s}' must be a safe repo-relative path"
                ));
            }
            if PROTECTED_PATHS
                .iter()
                .any(|p| s == *p || s.starts_with(&format!("{p}/")))
            {
                return Err(format!(
                    "ticket {id}: scope entry '{s}' touches a protected path (the gate must stay immutable)"
                ));
            }
            scope_paths.push(s.to_string());
        }
        let verify = t
            .get("verify")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("ticket {id}: verify must be a string"))?;
        if !valid_verifier_shape(verify) {
            return Err(format!(
                "ticket {id}: verifier must be worktree-relative with no absolute/traversing/protected paths"
            ));
        }
        let verify_target = t
            .get("verify_target")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(".");
        if !valid_relative_path(verify_target) {
            return Err(format!(
                "ticket {id}: verify_target must be a safe relative path"
            ));
        }
        manifest.tickets.push(PlannerTicket {
            id: id.to_string(),
            goal: goal.to_string(),
            scope: scope_paths.clone(),
            verify: verify.to_string(),
            verify_target: verify_target.to_string(),
        });
        all_scopes.push((id.to_string(), scope_paths));
    }
    // Scope disjointness across the batch (second opinion, finding 4):
    // overlapping scopes are refused BEFORE dispatch.
    for (i, (id_a, scopes_a)) in all_scopes.iter().enumerate() {
        for (id_b, scopes_b) in all_scopes.iter().skip(i + 1) {
            for a in scopes_a {
                for b in scopes_b {
                    if a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
                    {
                        return Err(format!(
                            "tickets {id_a} and {id_b} have overlapping scopes ('{a}' vs '{b}')"
                        ));
                    }
                }
            }
        }
    }
    Ok(manifest)
}

/// Whether a path is under a protected root (used by the finalize
/// containment check too).
pub fn touches_protected(path: &str) -> bool {
    PROTECTED_PATHS
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{p}/")))
}

/// The ticket worktree root for a batch: `<base>/.batch/<id>`.
#[must_use]
pub fn ticket_worktree(base: &Path, id: &str) -> std::path::PathBuf {
    base.join(".batch").join(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{
        "version": 1,
        "tickets": [
            {"id": "t1", "goal": "g1", "scope": ["crates/a"], "verify": "cargo test -p a"},
            {"id": "t2", "goal": "g2", "scope": ["crates/b"], "verify": "cargo test -p b", "verify_target": "."}
        ]
    }"#;

    fn expect_ok(text: &str) -> PlannerManifest {
        match parse_manifest(text) {
            Ok(m) => m,
            Err(e) => panic!("expected a valid manifest, got: {e}"),
        }
    }

    fn expect_err(text: &str, needle: &str) {
        match parse_manifest(text) {
            Ok(_) => panic!("expected a violation containing '{needle}'"),
            Err(e) => assert!(e.contains(needle), "expected '{needle}' in error, got: {e}"),
        }
    }

    #[test]
    fn valid_manifest_parses() {
        let m = expect_ok(VALID);
        assert_eq!(m.version, 1);
        assert_eq!(m.tickets.len(), 2);
        assert_eq!(m.tickets[0].verify_target, ".");
    }

    #[test]
    fn unknown_fields_fail_closed() {
        expect_err(
            r#"{"version":1,"tickets":[],"extra":1}"#,
            "unknown manifest field",
        );
        expect_err(
            r#"{"version":1,"tickets":[{"id":"t","goal":"g","scope":["a"],"verify":"x","bogus":1}]}"#,
            "ticket 0: unknown field",
        );
    }

    #[test]
    fn version_and_count_gates() {
        expect_err(
            r#"{"version":2,"tickets":[{"id":"t","goal":"g","scope":["a"],"verify":"x"}]}"#,
            "version 2",
        );
        expect_err(r#"{"version":1,"tickets":[]}"#, "must not be empty");
    }

    #[test]
    fn duplicate_and_empty_tickets_fail() {
        expect_err(
            r#"{"version":1,"tickets":[{"id":"t","goal":"g","scope":["a"],"verify":"x"},{"id":"t","goal":"g","scope":["b"],"verify":"x"}]}"#,
            "duplicate ticket id",
        );
        expect_err(
            r#"{"version":1,"tickets":[{"id":"","goal":"g","scope":["a"],"verify":"x"}]}"#,
            "id must not be empty",
        );
        expect_err(
            r#"{"version":1,"tickets":[{"id":"t","goal":"","scope":["a"],"verify":"x"}]}"#,
            "goal must not be empty",
        );
        expect_err(
            r#"{"version":1,"tickets":[{"id":"t","goal":"g","scope":[],"verify":"x"}]}"#,
            "scope must not be empty",
        );
    }

    #[test]
    fn unsafe_paths_fail_closed() {
        for bad in ["/abs", "~/home", "../up", "a/../../up", "a:b"] {
            let text = format!(
                r#"{{"version":1,"tickets":[{{"id":"t","goal":"g","scope":["{bad}"],"verify":"x"}}]}}"#
            );
            expect_err(&text, "safe repo-relative");
        }
    }

    #[test]
    fn protected_paths_are_refused() {
        for p in [
            "scripts/verify.sh",
            "evals",
            "evals/cases/x",
            "memory",
            "tickets",
        ] {
            let text = format!(
                r#"{{"version":1,"tickets":[{{"id":"t","goal":"g","scope":["{p}"],"verify":"x"}}]}}"#
            );
            expect_err(&text, "protected");
        }
    }

    #[test]
    fn overlapping_scopes_fail_before_dispatch() {
        expect_err(
            r#"{"version":1,"tickets":[{"id":"a","goal":"g","scope":["crates/x"],"verify":"x"},{"id":"b","goal":"g","scope":["crates/x/src"],"verify":"x"}]}"#,
            "overlapping scopes",
        );
    }

    #[test]
    fn verifier_shape_allowlist() {
        assert!(valid_verifier_shape("cargo test"));
        assert!(valid_verifier_shape("sh -c 'python3 tests/run.py'"));
        assert!(!valid_verifier_shape(""));
        assert!(!valid_verifier_shape("/usr/bin/make verify"));
        assert!(!valid_verifier_shape("make -C /abs verify"));
        assert!(!valid_verifier_shape("python3 ~/scripts/check.py"));
        assert!(!valid_verifier_shape("sh ../../evil.sh"));
        assert!(!valid_verifier_shape("sh scripts/verify.sh"));
    }

    #[test]
    fn ticket_worktree_is_scoped_under_batch() {
        let wt = ticket_worktree(Path::new("/repo"), "t1");
        assert_eq!(wt, Path::new("/repo/.batch/t1"));
    }
}
