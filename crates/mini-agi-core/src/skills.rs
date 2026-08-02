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
    /// Absolute path of the skill's `SKILL.md`.
    pub path: PathBuf,
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
    Ok(Skill {
        name,
        description,
        verify,
        disable_model_invocation,
        argument_hint,
        path: PathBuf::new(),
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
/// [`SkillError::Parse`] when a `SKILL.md` is malformed.
pub fn discover_skills(root: &Path) -> Result<Vec<Skill>, SkillError> {
    let dir = agents_dir(root);
    let entries = fs::read_dir(&dir).map_err(SkillError::Io)?;
    let mut skills = Vec::new();
    for entry in entries {
        let entry = entry.map_err(SkillError::Io)?;
        let dir_path = entry.path();
        if !dir_path.is_dir() {
            continue;
        }
        let skill_md = dir_path.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        let content = fs::read_to_string(&skill_md).map_err(SkillError::Io)?;
        match parse_skill_md(&content) {
            Ok(mut skill) => {
                if skill.name == entry.file_name().to_string_lossy() {
                    skill.path = skill_md;
                    skills.push(skill);
                }
            }
            Err(e) => return Err(SkillError::Parse(format!("{}: {e}", dir_path.display()))),
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Find one skill by name.
///
/// # Errors
///
/// Returns [`SkillError::Unknown`] when no `SKILL.md` exists for the name.
pub fn find_skill(root: &Path, name: &str) -> Result<Skill, SkillError> {
    let dir = agents_dir(root).join(name);
    let skill_md = dir.join("SKILL.md");
    if !skill_md.is_file() {
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
    if repo_skills.is_dir() {
        let entries = fs::read_dir(&repo_skills).map_err(SkillError::Io)?;
        for entry in entries {
            let entry = entry.map_err(SkillError::Io)?;
            if !entry.path().is_dir() {
                continue;
            }
            let skill_md = entry.path().join("SKILL.md");
            if skill_md.is_file() {
                let name = install_one(root, &skill_md)?;
                installed.push(name);
            }
        }
    } else if staging.join("SKILL.md").is_file() {
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
    let _ = fs::remove_dir_all(&dest);
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
        } else {
            fs::copy(entry.path(), &target).map_err(SkillError::Io)?;
        }
    }
    Ok(skill.name)
}

fn copy_dir(src: &Path, dest: &Path) -> Result<(), SkillError> {
    fs::create_dir_all(dest).map_err(SkillError::Io)?;
    for entry in fs::read_dir(src).map_err(SkillError::Io)? {
        let entry = entry.map_err(SkillError::Io)?;
        let target = dest.join(entry.file_name());
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            copy_dir(&entry.path(), &target)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"---
name: verify
description: Deterministic verification contract.
verify: scripts/verify.sh
disable-model-invocation: true
argument-hint: "what changed"
---

# Verify

Body text.
"#;

    #[test]
    fn parses_full_frontmatter() {
        let skill = parse_skill_md(VALID).unwrap();
        assert_eq!(skill.name, "verify");
        assert_eq!(skill.description, "Deterministic verification contract.");
        assert_eq!(skill.verify.as_deref(), Some("scripts/verify.sh"));
        assert!(skill.disable_model_invocation);
        assert_eq!(skill.argument_hint.as_deref(), Some("what changed"));
    }

    #[test]
    fn parses_multiline_description() {
        let md = r#"---
name: caveman
description: >
  Ultra-compressed communication mode. Cuts token usage ~75% by dropping
  filler, articles, and pleasantries while keeping full technical accuracy.
  Use when user says "caveman mode".
---

# Caveman
"#;
        let skill = parse_skill_md(md).unwrap();
        assert_eq!(
            skill.description,
            "Ultra-compressed communication mode. Cuts token usage ~75% by dropping filler, articles, and pleasantries while keeping full technical accuracy. Use when user says \"caveman mode\"."
        );
    }

    #[test]
    fn rejects_missing_name() {
        let md = "---\ndescription: no name here\n---\n";
        let err = parse_skill_md(md).unwrap_err();
        assert!(matches!(err, SkillError::MissingField(k) if k == "name"));
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let err = parse_skill_md("# no frontmatter\n").unwrap_err();
        assert!(matches!(err, SkillError::Parse(_)));
    }

    #[test]
    fn rejects_bad_line() {
        let err = parse_skill_md("---\nname only no colon\n---\n").unwrap_err();
        assert!(matches!(err, SkillError::Parse(_)));
    }

    #[test]
    fn discovers_skills_in_agents_dir() {
        let root = tempfile_dir("discover");
        let skills = root.join(".agents").join("skills");
        fs::create_dir_all(skills.join("verify")).unwrap();
        fs::create_dir_all(skills.join("review")).unwrap();
        fs::create_dir_all(skills.join("not-a-skill")).unwrap();
        fs::write(skills.join("verify/SKILL.md"), VALID).unwrap();
        fs::write(
            skills.join("review/SKILL.md"),
            "---\nname: review\ndescription: Rubric review.\n---\n",
        )
        .unwrap();
        fs::write(skills.join("not-a-skill/README.md"), "no frontmatter here").unwrap();
        let found = discover_skills(&root).unwrap();
        let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["review", "verify"]);
        assert!(found.iter().all(|s| s.path.is_file()));
    }

    #[test]
    fn find_skill_returns_unknown() {
        let root = tempfile_dir("find-unknown");
        let err = find_skill(&root, "missing").unwrap_err();
        assert!(matches!(err, SkillError::Unknown(_)));
    }

    #[test]
    fn verify_hook_reports_pass_and_fail() {
        let root = tempfile_dir("verify-hook");
        fs::create_dir_all(root.join(".agents/skills/pass")).unwrap();
        fs::create_dir_all(root.join(".agents/skills/fail")).unwrap();
        fs::write(
            root.join(".agents/skills/pass/SKILL.md"),
            "---\nname: pass\ndescription: pass\nverify: 'true'\n---\n",
        )
        .unwrap();
        fs::write(
            root.join(".agents/skills/fail/SKILL.md"),
            "---\nname: fail\ndescription: fail\nverify: 'exit 3'\n---\n",
        )
        .unwrap();
        let pass = find_skill(&root, "pass").unwrap();
        let ok = verify_skill(&pass, &root).unwrap();
        assert!(ok.passed);
        assert_eq!(ok.exit_code, Some(0));
        let fail = find_skill(&root, "fail").unwrap();
        let bad = verify_skill(&fail, &root).unwrap();
        assert!(!bad.passed);
        assert_eq!(bad.exit_code, Some(3));
    }

    #[test]
    fn verify_without_hook_is_error() {
        let root = tempfile_dir("no-hook");
        fs::create_dir_all(root.join(".agents/skills/plain")).unwrap();
        fs::write(
            root.join(".agents/skills/plain/SKILL.md"),
            "---\nname: plain\ndescription: plain\n---\n",
        )
        .unwrap();
        let plain = find_skill(&root, "plain").unwrap();
        let err = verify_skill(&plain, &root).unwrap_err();
        assert!(matches!(err, SkillError::NoVerifyHook(_)));
    }

    fn tempfile_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("skills-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
