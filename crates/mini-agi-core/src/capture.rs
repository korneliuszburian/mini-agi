//! Codex capture hook (Phase 8 slice 4, EXP-003 — Sandcastle-style
//! integration): a `codex exec` transcript becomes a truthful
//! trajectory (exec/write/read steps with line provenance).
//!
//! Commands become `exec` steps, file-write lines become `write` steps
//! with paths, completion markers and structured `<result>` payloads
//! are extracted. The parser never invents a step it cannot see.

/// One captured step from a transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedStep {
    /// 1-based position in the transcript.
    pub line: usize,
    /// `exec` | `write` | `read`.
    pub tool: String,
    /// The action text (command or file operation).
    pub action: String,
    /// Paths touched (for write steps; best-effort).
    pub paths: Vec<String>,
}

/// Lines that look like executed commands (shell prompts, bare command
/// starts, or `/usr/bin/bash -lc 'cmd'` invocations — the form codex
/// transcripts use). Heuristic, intentionally conservative: a line that
/// merely mentions a command is not executed.
fn looks_like_command(line: &str) -> bool {
    let trimmed = line.trim();
    let bare = trimmed
        .strip_prefix("$ ")
        .or_else(|| trimmed.strip_prefix("> "))
        .unwrap_or(trimmed);
    let first = bare.split_whitespace().next().unwrap_or("");
    matches!(
        first,
        "python3"
            | "python"
            | "make"
            | "npx"
            | "npm"
            | "git"
            | "ls"
            | "cat"
            | "mkdir"
            | "touch"
            | "rm"
            | "cp"
            | "mv"
            | "chmod"
            | "sh"
            | "bash"
            | "node"
            | "tsc"
            | "which"
            | "head"
            | "wc"
            | "grep"
            | "find"
            | "cargo"
            | "codex"
    ) || trimmed.starts_with("$ ")
        || trimmed.starts_with("> ")
        || trimmed.starts_with("/usr/bin/bash -lc ")
        || trimmed.starts_with("/bin/bash -lc ")
}

/// Parse a codex transcript into captured steps.
///
/// Commands (exec), explicit write/create lines (write, path extracted
/// best-effort), and read lines (read) are captured with their line
/// numbers. Everything else is ignored — the parser never invents a
/// step it cannot see.
#[must_use]
pub fn parse_transcript(text: &str) -> Vec<CapturedStep> {
    let mut steps = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if looks_like_command(line) {
            steps.push(CapturedStep {
                line: idx + 1,
                tool: "exec".into(),
                action: line.to_string(),
                paths: Vec::new(),
            });
            continue;
        }
        let lower = line.to_lowercase();
        for prefix in ["wrote ", "created ", "updated ", "writes ", "creating "] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                let path = line[line.len() - rest.len()..].trim();
                steps.push(CapturedStep {
                    line: idx + 1,
                    tool: "write".into(),
                    action: line.to_string(),
                    paths: vec![path.trim_matches('`').trim().to_string()],
                });
                break;
            }
        }
        if line.starts_with("read:") || lower.starts_with("reading ") {
            steps.push(CapturedStep {
                line: idx + 1,
                tool: "read".into(),
                action: line.to_string(),
                paths: Vec::new(),
            });
        }
    }
    steps
}

/// Extract structured output from a transcript: content of the LAST
/// `<result>...</result>` block (Sandcastle-style completion payload),
/// trimmed.
#[must_use]
pub fn extract_result(text: &str) -> Option<String> {
    let mut last: Option<String> = None;
    for cap in text.match_indices("<result>") {
        let start = cap.0 + "<result>".len();
        if let Some(end) = text[start..].find("</result>") {
            last = Some(text[start..start + end].trim().to_string());
        }
    }
    last.filter(|s| !s.is_empty())
}

/// Detect the completion marker `<promise>COMPLETE</promise>`.
#[must_use]
pub fn completed(text: &str) -> bool {
    text.contains("<promise>COMPLETE</promise>")
}

/// A captured codex run: transcript log path + parsed steps + markers.
#[derive(Debug)]
pub struct CaptureOutcome {
    /// Where the raw transcript was stored.
    pub log_path: std::path::PathBuf,
    /// Parsed steps (exec/write/read), ordered by line.
    pub steps: Vec<CapturedStep>,
    /// Whether the completion marker was present.
    pub completed: bool,
    /// Structured `<result>` payload, if any.
    pub result: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSCRIPT: &str = r#"$ ls
src/
$ cat src/auth.ts
export function authenticate() {}
> python3 -m unittest discover -s tests
Wrote tests/test_auth.py
Created src/auth.py
read: README.md
plain prose line is ignored
$ make verify
verify: ALL GREEN
<result>{"files": ["src/auth.py"], "tests": 5}</result>
no completion marker here
"#;

    #[test]
    fn parses_commands_writes_and_reads_in_order() {
        let steps = parse_transcript(TRANSCRIPT);
        let kinds: Vec<&str> = steps.iter().map(|s| s.tool.as_str()).collect();
        assert_eq!(
            kinds,
            vec!["exec", "exec", "exec", "write", "write", "read", "exec"]
        );
        assert_eq!(steps[0].line, 1);
        assert_eq!(steps[3].paths, vec!["tests/test_auth.py"]);
        assert_eq!(steps[4].paths, vec!["src/auth.py"]);
        assert!(
            steps
                .iter()
                .all(|s| s.action != "plain prose line is ignored")
        );
    }

    #[test]
    fn extracts_last_result_block() {
        assert_eq!(
            extract_result(TRANSCRIPT).as_deref(),
            Some(r#"{"files": ["src/auth.py"], "tests": 5}"#)
        );
        assert_eq!(extract_result("no results here"), None);
    }

    #[test]
    fn completion_marker_detection() {
        assert!(!completed(TRANSCRIPT));
        assert!(completed("done\n<promise>COMPLETE</promise>\n"));
    }

    #[test]
    fn parses_real_exp002_transcript() {
        // The actual EXP-002 transcript (codex-exp2 run): the parser must
        // find the unittest and make invocations that are provably in it.
        let text = std::fs::read_to_string("/tmp/opencode/codex-exp2/codex.log").unwrap();
        let steps = parse_transcript(&text);
        let execs: Vec<&str> = steps
            .iter()
            .filter(|s| s.tool == "exec")
            .map(|s| s.action.as_str())
            .collect();
        assert!(
            execs.iter().any(|a| a.contains("unittest")),
            "unittest invocations must be captured: {execs:?}"
        );
        assert!(
            execs.iter().any(|a| a.contains("make")),
            "make invocations must be captured: {execs:?}"
        );
    }
}
