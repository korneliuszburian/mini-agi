//! Verifiable skills registry.
//!
//! A skill is a directory under `.agents/skills/<name>/SKILL.md` with
//! frontmatter: `name`, `description`, optional `verify` (a shell command,
//! run from the repo root, that self-tests the skill), optional
//! `disable-model-invocation`, optional `argument-hint`.
//!
//! Contract (ADR-0002): skills live in `.agents/skills/`, never in a bare
//! `skills/` directory. A skill without a `verify` hook is `reference`
//! only; the deterministic gate only checks `verify`-carrying skills.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A discovered skill.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Skill name (frontmatter `name`; directory name must match).
    pub name: String,
    /// Short description (frontmatter `description`).
    pub description: String,
    /// Verify hook: shell command run from the repo root.
    pub verify: Option<String>,
    /// True when the model must not invoke the skill by itself.
    pub disable_model_invocation: bool,
    /// Hint for the argument the user should pass to the skill.
    pub argument_hint: Option<String>,
    /// Sandbox policy for the skill (production-readiness D.2): `None`/
    /// `"write"` = write-containment (Landlock, default); `"read-only"` =
    /// the worker runs with NO workdir write access (least authority).
    pub sandbox: Option<String>,
    /// True when the skill's verify hook failed and the skill was marked
    /// disabled (HARDENING P2-14): a broken dynamic skill must not keep
    /// running silently.
    pub disabled: bool,
    /// Absolute path of the skill's `SKILL.md`.
    pub path: PathBuf,
    /// Frontmatter `version` (semver) — the skill's own version, for
    /// install-diff and provenance.
    pub version: Option<String>,
    /// Frontmatter `source` — where the skill came from (origin path
    /// or URL).
    pub source: Option<String>,
    /// Frontmatter `type` — "procedural" (default) or "mode" (a mode
    /// skill like caveman is not a procedure; exempt from the lint).
    pub kind: String,
}

/// Result of running a skill's verify hook.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// True when the hook exited 0.
    pub passed: bool,
    /// Hook stdout+stderr.
    pub output: String,
    /// Exit code, when the process exited.
    pub exit_code: Option<i32>,
}

/// Skill registry errors.
#[derive(Debug)]
pub enum SkillError {
    /// Filesystem error.
    Io(io::Error),
    /// Malformed `SKILL.md` frontmatter.
    Parse(String),
    /// Required frontmatter key missing.
    MissingField(String),
    /// No skill with this name in the registry.
    Unknown(String),
    /// The skill has no `verify` hook.
    NoVerifyHook(String),
    /// The verify hook ran but failed (exit != 0) or could not start.
    VerifyFailed(String),
}

impl fmt::Display for SkillError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Parse(m) => write!(f, "malformed SKILL.md: {m}"),
            Self::MissingField(k) => write!(f, "missing frontmatter key '{k}'"),
            Self::Unknown(n) => write!(f, "no skill named '{n}'"),
            Self::NoVerifyHook(n) => write!(f, "skill '{n}' has no verify hook"),
            Self::VerifyFailed(m) => write!(f, "verify hook failed: {m}"),
        }
    }
}

impl std::error::Error for SkillError {}

/// Parse a `SKILL.md` document into a [`Skill`].
///
/// # Errors
///
/// Returns [`SkillError::Parse`] for malformed frontmatter or
/// [`SkillError::MissingField`] when `name` or `description` is absent.
pub fn parse_skill_md(content: &str) -> Result<Skill, SkillError> {
    let frontmatter = frontmatter_block(content)?;
    let fields = parse_frontmatter(frontmatter)?;
    let name = required(&fields, "name")?;
    let description = required(&fields, "description")?;
    let verify = fields.get("verify").cloned();
    let disable_model_invocation = matches!(
        fields.get("disable-model-invocation").map(String::as_str),
        Some("true" | "True" | "yes")
    );
    let argument_hint = fields.get("argument-hint").cloned();
    let sandbox = fields.get("sandbox").cloned();
    let disabled = matches!(
        fields.get("disabled").map(String::as_str),
        Some("true" | "True" | "yes")
    );
    let version = fields.get("version").cloned();
    let source = fields.get("source").cloned();
    let kind = fields
        .get("type")
        .cloned()
        .unwrap_or_else(|| "procedural".into());
    Ok(Skill {
        name,
        description,
        verify,
        disable_model_invocation,
        argument_hint,
        sandbox,
        disabled,
        path: PathBuf::new(),
        version,
        source,
        kind,
    })
}

/// Extract the `---` delimited frontmatter block.
fn frontmatter_block(content: &str) -> Result<&str, SkillError> {
    let rest = content
        .strip_prefix("---\n")
        .ok_or_else(|| SkillError::Parse("file must start with '---' line".into()))?;
    let end = rest
        .find("\n---")
        .ok_or_else(|| SkillError::Parse("missing closing '---' line".into()))?;
    Ok(&rest[..end])
}

/// Parse `key: value` lines, expanding `>` multi-line values.
fn parse_frontmatter(block: &str) -> Result<HashMap<String, String>, SkillError> {
    let mut fields = HashMap::new();
    let mut key = String::new();
    let mut multiline: Option<String> = None;
    for (i, raw) in block.lines().enumerate() {
        let line_no = i + 1;
        if let Some(acc) = multiline.as_mut() {
            if raw.starts_with("  ") || raw.is_empty() {
                let piece = raw.trim();
                if !piece.is_empty() {
                    if !acc.is_empty() {
                        acc.push(' ');
                    }
                    acc.push_str(piece);
                }
                continue;
            }
            fields.insert(key.clone(), acc.clone());
            multiline = None;
        }
        let Some((k, v)) = raw.split_once(':') else {
            return Err(SkillError::Parse(format!(
                "line {line_no}: expected 'key: value'"
            )));
        };
        let k = k.trim();
        let v = strip_quotes(v.trim());
        if k.is_empty() || v.is_empty() {
            return Err(SkillError::Parse(format!(
                "line {line_no}: empty key or value"
            )));
        }
        if v == ">" {
            key = k.to_string();
            multiline = Some(String::new());
        } else {
            fields.insert(k.to_string(), v);
        }
    }
    if let Some(acc) = multiline {
        fields.insert(key, acc);
    }
    Ok(fields)
}

/// Strip a matching pair of surrounding `'` or `"` quotes.
fn strip_quotes(v: &str) -> String {
    let bytes = v.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'\'' || bytes[0] == b'"')
        && bytes[0] == bytes[bytes.len() - 1]
    {
        return String::from_utf8_lossy(&bytes[1..bytes.len() - 1]).into_owned();
    }
    v.to_string()
}

fn required(fields: &HashMap<String, String>, key: &str) -> Result<String, SkillError> {
    fields
        .get(key)
        .cloned()
        .ok_or_else(|| SkillError::MissingField(key.to_string()))
}

/// A skill name must be a single plain path segment (no separators,
/// no `..`/`.` traversal, no leading dot) so `agents_dir(root).join(name)`
/// can never escape `.agents/skills/`.
#[must_use]
fn name_is_plain_segment(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
}

/// The registry root for a repo: `<root>/.agents/skills` (ADR-0002).
#[must_use]
pub fn agents_dir(root: &Path) -> PathBuf {
    root.join(".agents").join("skills")
}

/// Discover all skills in a repo's `.agents/skills/`, sorted by name.
///
/// # Errors
///
/// Returns [`SkillError::Io`] on filesystem failure or
/// [`SkillError::Parse`] when a `SKILL.md` is malformed or its
/// frontmatter `name` does not match the directory name (the same
/// contract [`install_skills`] enforces at install).
pub fn discover_skills(root: &Path) -> Result<Vec<Skill>, SkillError> {
    let dir = agents_dir(root);
    let entries = fs::read_dir(&dir).map_err(SkillError::Io)?;
    let mut skills = Vec::new();
    for entry in entries {
        let entry = entry.map_err(SkillError::Io)?;
        let dir_path = entry.path();
        let meta = std::fs::symlink_metadata(&dir_path).map_err(SkillError::Io)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        let skill_md = dir_path.join("SKILL.md");
        let md = match std::fs::symlink_metadata(&skill_md) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(SkillError::Io(e)),
        };
        if !md.is_file() || md.file_type().is_symlink() {
            continue;
        }
        let content = fs::read_to_string(&skill_md).map_err(SkillError::Io)?;
        match parse_skill_md(&content) {
            Ok(mut skill) => {
                // The frontmatter name must match the directory name
                // (same contract install_one enforces): a mismatched
                // registration is malformed, not silently droppable —
                // the registry gate must see the violation.
                let dir_name = entry.file_name().to_string_lossy().into_owned();
                if skill.name != dir_name {
                    return Err(SkillError::Parse(format!(
                        "name '{name}' does not match directory '{dir_name}'",
                        name = skill.name
                    )));
                }
                skill.path = skill_md;
                skills.push(skill);
            }
            Err(e) => return Err(SkillError::Parse(format!("{}: {e}", dir_path.display()))),
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// The 2% context budget for the skills registry listing, in chars
/// (TICKET-14 moved the bound here so the enforcement point owns it).
pub const SKILLS_BUDGET_CHARS: usize = 8000;

/// The skills registry as a BOUNDED working set (TICKET-14).
///
/// `metrics::budget` used to measure the unbounded registry: every
/// `SKILL.md` frontmatter counted, no ranking, no cap — the "2% budget"
/// was a report, not a limit (the TICKET-13 `MAX_BRIEF_BYTES` dead-code
/// pattern). This is the listing agents actually see: deterministic
/// ranking (enabled-with-verify-hook, enabled-without, disabled; then
/// alphabetical), filled to `cap_chars` with the same frontmatter-char
/// accounting as the budget report, and a truncation notice. The
/// accounting is strictly <= `cap_chars`.
#[derive(Debug, Clone, Default)]
pub struct BudgetedList {
    /// Rendered listing lines (the working set; the last line is the
    /// truncation notice when the registry exceeds the cap).
    pub entries: Vec<String>,
    /// Total skills in the registry (all discovered, incl. disabled).
    pub total: usize,
    /// Skills rendered into `entries` (excludes truncated ones).
    pub shown: usize,
    /// Frontmatter chars of the shown skills (cap accounting).
    pub chars: usize,
}

/// Build the bounded skills listing (TICKET-14).
#[must_use]
pub fn budgeted_list(root: &Path, cap_chars: usize) -> BudgetedList {
    // Reserve room for the notice so the accounting stays <= cap
    // (mirror of the brief-cap notice reservation).
    const NOTICE_RESERVE: usize = 96;
    let Ok(mut skills) = discover_skills(root) else {
        return BudgetedList::default();
    };
    let budget = cap_chars.saturating_sub(NOTICE_RESERVE);
    let total = skills.len();
    skills.sort_by(|a, b| {
        skill_rank(a)
            .cmp(&skill_rank(b))
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut chars = 0usize;
    let mut entries = Vec::new();
    let mut shown = 0usize;
    for skill in &skills {
        let Some(block_chars) = frontmatter_chars(&skill.path) else {
            continue;
        };
        if chars + block_chars > budget {
            break;
        }
        chars += block_chars;
        let hook = if skill.verify.is_some() {
            "verify"
        } else {
            "ref"
        };
        entries.push(format!("{}  [{hook}]  {}", skill.name, skill.description));
        shown += 1;
    }
    if shown < total {
        entries.push(format!(
            "... {} more skills in .agents/skills/",
            total - shown
        ));
    }
    BudgetedList {
        entries,
        total,
        shown,
        chars,
    }
}

/// Deterministic listing rank: enabled-with-verify first, enabled-only
/// second, disabled last.
const fn skill_rank(s: &Skill) -> u8 {
    match (s.disabled, s.verify.is_some()) {
        (false, true) => 0,
        (false, false) => 1,
        (true, _) => 2,
    }
}

/// Frontmatter char count of a `SKILL.md` (the budget accounting unit).
fn frontmatter_chars(path: &Path) -> Option<usize> {
    let text = fs::read_to_string(path).ok()?;
    let block = frontmatter_block(&text).ok()?;
    Some(block.chars().count())
}

/// Find one skill by name.
///
/// # Errors
///
/// Returns [`SkillError::Unknown`] when no `SKILL.md` exists for the name,
/// or when the name is not path-safe (separators/traversal would escape
/// `.agents/skills/`).
pub fn find_skill(root: &Path, name: &str) -> Result<Skill, SkillError> {
    // Path safety (mirrors derive-snapshot validation): a raw `join`
    // would let `skill show ../../x` read SKILL.md outside
    // `.agents/skills/` (MCP-amplified read). Symlink check below.
    let dir = if name_is_plain_segment(name) {
        agents_dir(root).join(name)
    } else {
        return Err(SkillError::Unknown(name.to_string()));
    };
    // The skill DIR must not be a symlink: traversing
    // `.agents/skills/<name>` would resolve an outside target.
    match std::fs::symlink_metadata(&dir) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        Ok(_) => return Err(SkillError::Unknown(name.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SkillError::Unknown(name.to_string()));
        }
        Err(e) => return Err(SkillError::Io(e)),
    }
    let skill_md = dir.join("SKILL.md");
    // symlink_metadata: a symlinked skill dir or manifest must not be
    // followed by the single-skill verify path. A missing manifest is
    // Unknown; any other metadata error is an Io failure.
    let md = match std::fs::symlink_metadata(&skill_md) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SkillError::Unknown(name.to_string()));
        }
        Err(e) => return Err(SkillError::Io(e)),
    };
    if !md.is_file() || md.file_type().is_symlink() {
        return Err(SkillError::Unknown(name.to_string()));
    }
    let content = fs::read_to_string(&skill_md).map_err(SkillError::Io)?;
    let mut skill = parse_skill_md(&content)?;
    skill.path = skill_md;
    Ok(skill)
}

/// Run a skill's verify hook from `cwd` (the repo root).
///
/// # Errors
///
/// Returns [`SkillError::NoVerifyHook`] when the skill has no hook, or
/// [`SkillError::VerifyFailed`] when the command cannot be started.
/// Aggregate verification of every skill hook (`skill verify --all`).
#[derive(Debug, Default)]
pub struct VerifyAllReport {
    /// Skills whose hook passed.
    pub passed: Vec<String>,
    /// Skills whose hook FAILED (the gate fails on these).
    pub failed: Vec<(String, i32)>,
    /// PROCEDURAL skills without a verify hook (FATAL — the gate
    /// fails; a mode skill like caveman is legitimately hook-less).
    pub no_hook: Vec<String>,
    /// Skills missing `version` or `source` frontmatter (D5).
    pub no_version: Vec<String>,
    /// Skills failing the structural lint (D1 — every skill is a
    /// contract): the SKILL.md must carry the four frontmatter keys
    /// AND a checkable-criteria marker ("Done when" / "Completion
    /// criteria" in the body).
    pub lint_failed: Vec<String>,
}

/// Dual-registration drift (D4): local vs global content divergence.
///
/// A skill registered in BOTH `.agents/skills/` and the user-global
/// dir (`~/.agents/skills/`, overridable via `MINIAGI_GLOBAL_SKILLS`)
/// must have one owner; different content is reported by the gate so
/// divergence is never silent.
#[derive(Debug, Default)]
pub struct DriftReport {
    /// (name, local sha256[:16], global sha256[:16]).
    pub drifted: Vec<(String, String, String)>,
}

/// Deterministic content hash of a skill dir (the SKILL.md plus any
/// auxiliary files — the whole installable unit), 16-hex.
fn skill_hash(dir: &Path) -> Option<String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(dir, &mut files)?;
    files.sort();
    let mut acc: Vec<u8> = Vec::new();
    for f in files {
        let rel = f.strip_prefix(dir).ok()?;
        let rel_bytes = rel.as_os_str().as_encoded_bytes();
        // FRAMED hashing: length-prefix every piece (rel path, then
        // raw content) so `a`+`bcde` cannot collide with `abc`+`de`.
        acc.extend_from_slice(&(rel_bytes.len() as u64).to_le_bytes());
        acc.extend_from_slice(rel_bytes);
        let bytes = std::fs::read(&f).ok()?;
        acc.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        acc.extend_from_slice(&bytes);
    }
    Some(crate::hash::source_sha256_bytes(&acc))
}

/// Recursive file collector for `skill_hash`; returns None on a
/// symlinked directory (unbounded traversal from a user-controlled
/// tree must not be followed).
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Option<()> {
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        let meta = std::fs::symlink_metadata(&p).ok()?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_files(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Some(())
}

/// The contract hash: the SKILL.md content alone (16-hex). A skill's
/// CONTRACT is its SKILL.md — aux assets differ legitimately between
/// the local and distributed copies, and that is not drift.
fn skill_contract_hash(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    Some(crate::hash::source_sha256(&text))
}

/// Compare repo-local vs user-global registrations for CONTRACT drift.
#[must_use]
pub fn dual_registration_drift(root: &Path) -> DriftReport {
    let global = std::env::var("MINIAGI_GLOBAL_SKILLS").ok().map_or_else(
        || {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|h| h.join(".agents/skills"))
                .unwrap_or_default()
        },
        std::path::PathBuf::from,
    );
    dual_registration_drift_with(root, &global)
}

/// The global-dir parameterized form (testable without env mutation).
#[must_use]
pub fn dual_registration_drift_with(root: &Path, global: &Path) -> DriftReport {
    let mut report = DriftReport::default();
    let Ok(entries) = std::fs::read_dir(root.join(".agents/skills")) else {
        return report;
    };
    for e in entries.flatten() {
        let local = e.path();
        if !local.join("SKILL.md").is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let global_dir = global.join(&name);
        if !global_dir.join("SKILL.md").is_file() {
            continue;
        }
        // One-owner enforcement: ANY dual registration is a violation
        // (identical or drifted) — the local is canonical.
        let (Some(lh), Some(gh)) = (
            skill_contract_hash(&local),
            skill_contract_hash(&global_dir),
        ) else {
            report
                .drifted
                .push((name, "unreadable".into(), "unreadable".into()));
            continue;
        };
        report.drifted.push((name, lh, gh));
    }
    report
}

/// Verify every skill's hook. A missing hook on a PROCEDURAL skill is
/// reported in `no_hook` and FAILS the caller's gate (mode skills are
/// exempt); version/lint/drift violations are also reported.
///
/// DISABLED skills (P2-14) are QUARANTINED: they are skipped entirely.
/// The disable is the gate's own remedy for a failing hook — re-running
/// a disabled skill's hook (or linting it) would keep the gate red
/// forever and the remedy could never clear it.
///
/// # Errors
///
/// Returns when the skill registry cannot be discovered.
pub fn verify_all_skills(root: &Path) -> Result<VerifyAllReport, SkillError> {
    let registry = discover_skills(root)?;
    let mut report = VerifyAllReport::default();
    for skill in &registry {
        if skill.disabled {
            continue;
        }
        if skill.version.is_none() || skill.source.is_none() {
            report.no_version.push(skill.name.clone());
        }
        if !lint_skill(skill) {
            report.lint_failed.push(skill.name.clone());
        }
        match skill.verify.as_deref() {
            // A PROCEDURAL skill without a hook is a violation (fatal);
            // a mode skill (caveman) legitimately has none.
            None if skill.kind != "mode" => report.no_hook.push(skill.name.clone()),
            None => {}
            Some(_) => match verify_skill(skill, root) {
                Ok(r) if r.passed => report.passed.push(skill.name.clone()),
                Ok(r) => report
                    .failed
                    .push((skill.name.clone(), r.exit_code.unwrap_or(-1))),
                Err(_) => report.failed.push((skill.name.clone(), -1)),
            },
        }
    }
    Ok(report)
}

/// Structural lint (D1): every skill is a CONTRACT — the frontmatter
/// must carry name/description/version/source and the body must mark
/// checkable criteria ("Done when" or "Completion criteria"), so the
/// gate enforces a contract shape on skills without a shell hook.
/// Artifact anchors a completion criterion must reference to be
/// auditable (quoted output / commit / diff / file path) — the lint
/// rejects self-reports with no artifact anchor.
const ARTIFACT_ANCHORS: [&str; 9] = [
    "quoted",
    "git ",
    "git:",
    "commit",
    "sha",
    "mini-agi ",
    "checkpoint.sh",
    "knowledge/",
    "path",
];

fn lint_skill(skill: &Skill) -> bool {
    if skill.kind == "mode" {
        return true;
    }
    if skill.name.is_empty() || skill.description.is_empty() {
        return false;
    }
    if skill.version.is_none() || skill.source.is_none() {
        return false;
    }
    let Ok(content) = std::fs::read_to_string(&skill.path) else {
        return false;
    };
    // The criteria marker must exist AND a CHECKBOX LINE inside the
    // criteria section must reference an artifact (quoted output /
    // commit / diff / file path) — a self-report with no anchor fails
    // the contract lint. The anchor is NOT accepted anywhere in the
    // doc (a prose `path` outside the criteria must not satisfy it).
    // The criteria SECTION (from the "Done when"/"Completion
    // criteria" heading down, STOPPING at the next same-or-higher
    // Markdown heading) must contain a checkbox AND reference an
    // artifact anchor. Wrapped lines count; an anchor under a LATER
    // heading, or prose above the section, never satisfies it.
    let mut in_criteria = false;
    let mut has_checkbox = false;
    let mut anchored = false;
    let mut section_depth = 2usize;
    for line in content.lines() {
        let l = line.to_lowercase();
        // A Markdown heading is a `#` run followed by whitespace or
        // EOL — `#define` (code) is NOT one. The same rule drives the
        // marker detection and the section-termination boundary.
        let hash_count = line.bytes().take_while(|&c| c == b'#').count();
        let is_heading = hash_count > 0
            && (hash_count == line.len() || line.as_bytes()[hash_count].is_ascii_whitespace());
        if is_heading && (l.contains("done when") || l.contains("completion criteria")) {
            in_criteria = true;
            section_depth = hash_count.max(1);
            continue;
        }
        if in_criteria {
            // Only a HEADING line can terminate the section — a plain
            // content line has hash_count 0 and must never break it.
            if is_heading {
                // The section stops at the next SAME-OR-HIGHER heading
                // (depth <= section_depth). A DEEPER heading (### under
                // ##) is still part of the section — the doc contract
                // above says "same-or-higher", not "same-or-deeper".
                if hash_count <= section_depth {
                    break;
                }
            }
            if l.trim_start().starts_with("- [ ]") {
                has_checkbox = true;
            }
            if ARTIFACT_ANCHORS.iter().any(|a| l.contains(a)) {
                anchored = true;
            }
        }
    }
    has_checkbox && anchored
}

/// Verify a skill's hook: run the `verify` frontmatter command from
/// the given cwd; a missing hook is an error.
///
/// # Errors
///
/// Returns when the hook is missing or cannot be executed.
pub fn verify_skill(skill: &Skill, cwd: &Path) -> Result<VerifyResult, SkillError> {
    let Some(cmd) = skill.verify.as_deref() else {
        return Err(SkillError::NoVerifyHook(skill.name.clone()));
    };
    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .output()
        .map_err(|e| SkillError::VerifyFailed(format!("{}: {e}", skill.name)))?;
    Ok(VerifyResult {
        passed: output.status.success(),
        output: String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr),
        exit_code: output.status.code(),
    })
}

/// Mark a skill enabled/disabled (HARDENING P2-14): rewrite the
/// skill's `SKILL.md` frontmatter so the disabled state persists.
///
/// # Errors
///
/// Returns [`SkillError::Io`] on filesystem failure or when the skill
/// cannot be found.
pub fn set_disabled(root: &Path, name: &str, disabled: bool) -> Result<(), SkillError> {
    let skill = find_skill(root, name)?;
    let text = fs::read_to_string(&skill.path).map_err(SkillError::Io)?;
    let marker = "disabled: true";
    // Strip every existing `disabled:` line first — on BOTH paths. The
    // disable path must not leave a stale `disabled: false` later in
    // the frontmatter (the parse takes the LAST duplicate key, so an
    // insert-at-top `disabled: true` would silently lose). The line
    // filter joins without a trailing newline; restore it so enable/
    // disable round-trips byte-stably.
    let stripped = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("disabled:"))
        .collect::<Vec<_>>()
        .join("\n");
    let stripped = if text.ends_with('\n') && !stripped.ends_with('\n') {
        format!("{stripped}\n")
    } else {
        stripped
    };
    let updated = if disabled {
        // Insert the marker right after the frontmatter opening line.
        let insert_at = stripped
            .find("---\n")
            .map(|i| i + "---\n".len())
            .ok_or_else(|| SkillError::Parse("no frontmatter".into()))?;
        let mut t = stripped;
        t.insert_str(insert_at, &format!("{marker}\n"));
        t
    } else {
        stripped
    };
    fs::write(&skill.path, updated).map_err(SkillError::Io)?;
    Ok(())
}

/// Install skills from a source into the registry.
///
/// The source is a git repository (URL, `owner/repo` GitHub shorthand, or a
/// local path). Skills are taken from the repo's own `.agents/skills/`, or
/// from the repo root when the repo itself is a skill (has `SKILL.md`).
/// The frontmatter must parse and `name` must match the directory name,
/// otherwise installation fails.
///
/// # Errors
///
/// Returns [`SkillError::Io`] on filesystem failure, [`SkillError::Parse`]
/// for malformed `SKILL.md`, or [`SkillError::VerifyFailed`] when git
/// cannot clone the source.
///
/// # Panics
///
/// Panics if the staging path under the temp root is not valid UTF-8 —
/// the staging directory is always created by this function, so this is
/// not reachable in practice.
pub fn install_skills(root: &Path, source: &str) -> Result<Vec<String>, SkillError> {
    let normalized = if source.contains('/') && !source.starts_with("http") {
        if source.starts_with("github.com") || source.matches('/').count() == 1 {
            format!("https://github.com/{source}")
        } else {
            source.to_string()
        }
    } else {
        source.to_string()
    };
    let staging = std::env::temp_dir().join(format!(
        "mag-skill-src-{}-{}",
        std::process::id(),
        hash_tail(&normalized)
    ));
    let _ = fs::remove_dir_all(&staging);
    let status = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "-q",
            &normalized,
            staging.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| SkillError::VerifyFailed(format!("git clone: {e}")))?;
    if !status.success() {
        return Err(SkillError::VerifyFailed(format!(
            "git clone failed for '{source}'"
        )));
    }
    let repo_skills = staging.join(".agents").join("skills");
    let mut installed = Vec::new();
    let repo_skills_ok = std::fs::symlink_metadata(&repo_skills)
        .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink());
    if repo_skills_ok {
        let entries = fs::read_dir(&repo_skills).map_err(SkillError::Io)?;
        for entry in entries {
            let entry = entry.map_err(SkillError::Io)?;
            // symlink_metadata: a symlinked skill dir or manifest must
            // not be followed at install.
            let meta = std::fs::symlink_metadata(entry.path()).map_err(SkillError::Io)?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                continue;
            }
            let skill_md = entry.path().join("SKILL.md");
            let md = std::fs::symlink_metadata(&skill_md);
            if md.is_ok_and(|m| m.is_file() && !m.file_type().is_symlink()) {
                let name = install_one(root, &skill_md)?;
                installed.push(name);
            }
        }
    } else if std::fs::symlink_metadata(staging.join("SKILL.md"))
        .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        installed.push(install_one(root, &staging.join("SKILL.md"))?);
    } else {
        return Err(SkillError::Parse(
            "source has neither .agents/skills/ nor a root SKILL.md".into(),
        ));
    }
    if installed.is_empty() {
        return Err(SkillError::Parse("source contains no skills".into()));
    }
    let _ = fs::remove_dir_all(&staging);
    Ok(installed)
}

fn install_one(root: &Path, skill_md: &Path) -> Result<String, SkillError> {
    let content = fs::read_to_string(skill_md).map_err(SkillError::Io)?;
    let skill = parse_skill_md(&content)?;
    let dir_name = skill_md
        .parent()
        .ok_or_else(|| SkillError::Parse("skill has no directory".into()))?
        .file_name()
        .ok_or_else(|| SkillError::Parse("skill has no directory name".into()))?
        .to_string_lossy()
        .into_owned();
    if skill.name != dir_name {
        return Err(SkillError::Parse(format!(
            "name '{name}' does not match directory '{dir_name}'",
            name = skill.name
        )));
    }
    let dest = agents_dir(root).join(&skill.name);
    // Install-diff (D5): compare the WHOLE installable unit (SKILL.md
    // + aux files) and copy only when it differs. On a difference the
    // existing dir is RENAMED aside as a backup FIRST (a backup written
    // inside dest would be destroyed by the fresh install); nothing is
    // ever destroyed by an install.
    let src_dir = skill_md
        .parent()
        .ok_or_else(|| SkillError::Parse("skill has no directory".into()))?;
    if skill_hash(src_dir) == skill_hash(&dest) {
        return Ok(skill.name);
    }
    if dest.exists() {
        let backup = next_backup_path(&dest);
        fs::rename(&dest, &backup).map_err(SkillError::Io)?;
    }
    fs::create_dir_all(&dest).map_err(SkillError::Io)?;
    fs::copy(skill_md, dest.join("SKILL.md")).map_err(SkillError::Io)?;
    for entry in fs::read_dir(skill_md.parent().unwrap()).map_err(SkillError::Io)? {
        let entry = entry.map_err(SkillError::Io)?;
        if entry.file_name() == "SKILL.md" {
            continue;
        }
        let target = dest.join(entry.file_name());
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            copy_dir(&entry.path(), &target)?;
        } else if entry.file_type().is_ok_and(|t| t.is_symlink()) {
            // Symlinks are NOT followed at install (skill_hash ignores
            // them too): copying a file symlink would pull in content
            // outside the installable unit.
        } else {
            fs::copy(entry.path(), &target).map_err(SkillError::Io)?;
        }
    }
    Ok(skill.name)
}

/// Collision-free backup path for a skill dir being replaced: the
/// base name + `.local-before-<nanos>` + an incrementing counter when
/// the name already exists. An existing backup is NEVER deleted.
/// The base backup name for a skill dir + stamp (pure, testable).
fn backup_name(dest: &Path, stamp: u128) -> PathBuf {
    let base = dest
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    dest.with_file_name(format!("{base}.local-before-{stamp}"))
}

/// Collision-free backup path for a skill dir being replaced: the
/// base name + `.local-before-<nanos>` + an incrementing counter when
/// the name already exists. An existing backup is NEVER deleted.
fn next_backup_path(dest: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    next_backup_path_for(dest, stamp)
}

/// The collision-free name for a given stamp (the PRODUCTION seam the
/// collision test exercises): an existing name is never deleted, the
/// counter increments.
fn next_backup_path_for(dest: &Path, stamp: u128) -> PathBuf {
    let mut backup = backup_name(dest, stamp);
    let mut n = 0u32;
    while backup.exists() {
        n += 1;
        let base = backup_name(dest, stamp);
        backup = base.with_extension(format!(
            "{}-{n}",
            base.extension().unwrap_or_default().to_string_lossy()
        ));
    }
    backup
}

fn copy_dir(src: &Path, dest: &Path) -> Result<(), SkillError> {
    fs::create_dir_all(dest).map_err(SkillError::Io)?;
    for entry in fs::read_dir(src).map_err(SkillError::Io)? {
        let entry = entry.map_err(SkillError::Io)?;
        let target = dest.join(entry.file_name());
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            copy_dir(&entry.path(), &target)?;
        } else if entry.file_type().is_ok_and(|t| t.is_symlink()) {
        } else {
            fs::copy(entry.path(), &target).map_err(SkillError::Io)?;
        }
    }
    Ok(())
}

fn hash_tail(s: &str) -> String {
    let h = crate::hash::source_sha256(s);
    h[..12].to_string()
}
