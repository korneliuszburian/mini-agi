//! Landlock worker sandbox (ADR-0012): write-containment for the
//! codex/hitl worker.
//!
//! Linux-only (Landlock LSM, kernel 5.13+). The policy grants read +
//! execute across the whole tree (a coding agent must inspect context
//! and run tools) but confines write/create to an explicit allow-set
//! (the workdir, codex's own state dir, and any `--allow-write` dirs).
//!
//! Applied from a dedicated wrapper process (`mini-agi exec-sandbox`):
//! the wrapper applies the ruleset to itself, spawns the worker (which
//! inherits the restrictions), waits, and forwards the exit code. No
//! `unsafe` is used — the workspace `unsafe_code = "forbid"` stays
//! intact.
//!
//! Degradation is explicit: when the kernel lacks Landlock the wrapper
//! prints a warning and runs the worker unsandboxed — a missing sandbox
//! is reported, never silent.

use std::path::Path;

use landlock::{
    ABI, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus,
};

/// The sandbox policy: `write_dirs` are the only writable subtrees.
#[derive(Debug, Default, Clone)]
pub struct SandboxPolicy {
    /// Canonical directories granted write access (and their subtrees).
    pub write_dirs: Vec<std::path::PathBuf>,
}

impl SandboxPolicy {
    /// An empty policy: read+execute everywhere, writes denied.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant write access to `dir` and its subtree (best-effort — a
    /// missing dir is skipped so an absent `~/.codex` does not abort).
    pub fn allow_write(&mut self, dir: &Path) {
        if dir.exists()
            && let Ok(canon) = std::fs::canonicalize(dir)
            && !self.write_dirs.contains(&canon)
        {
            self.write_dirs.push(canon);
        }
    }

    /// Apply the policy to the current process. All descendants inherit
    /// it. Returns `Ok(())` only when Landlock is actually enforced;
    /// otherwise an error the caller must surface (never fail silently).
    pub fn apply(&self) -> Result<(), String> {
        let abi = ABI::V9;
        let handled = AccessFs::from_read(abi) | AccessFs::from_write(abi);
        let ruleset = Ruleset::default()
            .handle_access(handled)
            .map_err(|e| format!("landlock: cannot handle access: {e}"))?;
        let created = ruleset
            .create()
            .map_err(|e| format!("landlock: cannot create ruleset: {e}"))?;
        // Read + execute across the whole tree.
        let root_fd = PathFd::new("/").map_err(|e| format!("landlock: {e}"))?;
        let created = created
            .add_rule(PathBeneath::new(root_fd, AccessFs::from_read(abi)))
            .map_err(|e| format!("landlock: cannot grant read/execute on /: {e}"))?;
        // Write confined to the allow-set (rule on a dir covers its
        // whole subtree).
        let mut created = created;
        for dir in &self.write_dirs {
            let fd = PathFd::new(dir).map_err(|e| format!("landlock: {}: {e}", dir.display()))?;
            created = created
                .add_rule(PathBeneath::new(fd, AccessFs::from_write(abi)))
                .map_err(|e| format!("landlock: cannot grant write on {}: {e}", dir.display()))?;
        }
        let status = created
            .restrict_self()
            .map_err(|e| format!("landlock: cannot restrict self: {e}"))?;
        if status.ruleset == RulesetStatus::NotEnforced {
            return Err("landlock: ruleset not enforced by the running kernel".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn empty_policy_has_no_write_dirs() {
        let p = SandboxPolicy::new();
        assert!(p.write_dirs.is_empty());
    }

    #[test]
    fn allow_write_skips_missing_dirs_and_dedupes() {
        let root = std::env::temp_dir().join(format!("mag-sandbox-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut p = SandboxPolicy::new();
        p.allow_write(&root);
        p.allow_write(&root);
        p.allow_write(&root.join("does-not-exist"));
        assert_eq!(p.write_dirs.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
        let _ = PathBuf::new();
    }

    #[test]
    fn allow_write_canonicalizes_aliases() {
        let root = std::env::temp_dir().join(format!("mag-sandbox-canon-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        // A symlink alias of the same dir must dedup with the real path
        // (both canonicalize to the same entry — the landlock rule then
        // covers the subtree exactly once). NB: never chdir in tests —
        // the cwd is process-global and breaks sibling tests.
        let alias = root.join("alias");
        std::os::unix::fs::symlink(root.join("sub"), &alias).unwrap();
        let mut p = SandboxPolicy::new();
        p.allow_write(&root.join("sub"));
        p.allow_write(&alias);
        assert_eq!(p.write_dirs.len(), 1, "symlink alias dedups to one rule");
        assert!(
            p.write_dirs[0].is_absolute(),
            "stored paths are canonical/absolute"
        );
        let _ = std::fs::remove_dir_all(&root);
        let _ = PathBuf::new();
    }
}
