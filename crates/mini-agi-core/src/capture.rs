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
    /// `true` when the transcript carries exit-0 evidence for a bash
    /// invocation; `null` (None) when unknown — never invented.
    pub ok: Option<bool>,
}

/// Transcript noise that must never be captured as a step (codex
/// review: `npm notice`, bare `codex` tool labels, help text and the
/// "Reading additional input" preamble are prose, not actions).
fn is_transcript_noise(trimmed: &str) -> bool {
    trimmed.starts_with("npm notice")
        || trimmed.starts_with("Reading additional input")
        || trimmed == "codex"
        || trimmed.starts_with("OpenAI Codex v")
        || trimmed.starts_with("workdir:")
        || trimmed.starts_with("model:")
        || trimmed.starts_with("provider:")
        || trimmed.starts_with("approval:")
        || trimmed.starts_with("sandbox:")
        || trimmed.starts_with("reasoning")
        || trimmed.starts_with("session id:")
}

/// Extract the exit code from a `/usr/bin/bash -lc "cmd"` transcript
/// tool-result line — codex logs `(exit N)` at the end. `None` when the
/// line carries no exit evidence.
/// Extract the exit code from a codex tool-result line — codex logs
/// ` exited 2 in 0ms:` (or `(exit 2)`) on the line AFTER the command.
/// `None` when the line carries no exit evidence.
fn bash_exit(trimmed: &str) -> Option<i32> {
    let t = trimmed.trim();
    if let Some(rest) = t.strip_prefix("exited ") {
        return rest
            .split_whitespace()
            .next()
            .and_then(|n| n.trim_end_matches(':').parse().ok());
    }
    // The tool-result form is "succeeded in <duration>:" — a prose line
    // merely STARTING with "succeeded" (the model narrating) is not
    // exit-0 evidence; ok must never be invented (honest capture).
    if t.starts_with("succeeded in ") && t.ends_with(':') {
        return Some(0);
    }
    if t.starts_with("(exit") {
        return t
            .split_whitespace()
            .nth(1)
            .and_then(|n| n.trim_end_matches(')').parse().ok());
    }
    None
}

/// Look-ahead (hardening audit P2-12): bind the ` exited N in ...:`
/// tool-result header that follows a command to that command's exit
/// code. The command line itself carries no exit evidence; the NEXT
/// line does. Without this the honest capture records `ok: None` for
/// every command.
fn next_line_exit(lines: &[&str], idx: usize) -> Option<i32> {
    lines.get(idx + 1).and_then(|next| bash_exit(next))
}

/// Lines that look like executed commands (shell prompts, bare command
/// starts, or `/usr/bin/bash -lc 'cmd'` invocations — the form codex
/// transcripts use). Heuristic, intentionally conservative: a line that
/// merely mentions a command is not executed.
fn looks_like_command(line: &str) -> bool {
    let trimmed = line.trim();
    if is_transcript_noise(trimmed) {
        return false;
    }
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
    let lines: Vec<&str> = text.lines().collect();
    let mut steps = Vec::new();
    for (idx, raw) in lines.iter().enumerate() {
        let trimmed = raw.trim();
        let line = trimmed;
        if line.is_empty() || is_transcript_noise(line) {
            continue;
        }
        if looks_like_command(line) {
            steps.push(CapturedStep {
                line: idx + 1,
                tool: "exec".into(),
                action: line.to_string(),
                paths: Vec::new(),
                ok: next_line_exit(&lines, idx).map(|code| code == 0),
            });
            continue;
        }
        let lower = line.to_lowercase();
        // Case-folding can EXPAND byte length (e.g. 'İ' -> "i̇"): the
        // suffix math below slices the original line by the lowercased
        // rest's length, which would go negative and panic on such a
        // line (real defect found by falsifier — transcripts are
        // untrusted input). Only length-preserving lines take the
        // write-detection path.
        if lower.len() == line.len() {
            for prefix in ["wrote ", "created ", "updated ", "writes ", "creating "] {
                if let Some(rest) = lower.strip_prefix(prefix) {
                    let path = line[line.len() - rest.len()..].trim();
                    steps.push(CapturedStep {
                        line: idx + 1,
                        tool: "write".into(),
                        action: line.to_string(),
                        paths: vec![path.trim_matches('`').trim().to_string()],
                        ok: None,
                    });
                    break;
                }
            }
        }
        if line.starts_with("read:") || lower.starts_with("reading ") {
            steps.push(CapturedStep {
                line: idx + 1,
                tool: "read".into(),
                action: line.to_string(),
                paths: Vec::new(),
                ok: None,
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
///
/// The marker must appear in the LAST 20% of the transcript (codex
/// review: the prompt echo embeds the marker at the start — the
/// assistant's completion lands at the end). The heuristic is honest
/// for reparse; `cmd_codex` additionally strips the prompt text before
/// checking.
#[must_use]
pub fn completed(text: &str) -> bool {
    let marker = "<promise>COMPLETE</promise>";
    let Some(pos) = text.rfind(marker) else {
        return false;
    };
    pos >= text.len() * 4 / 5
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
    fn prose_succeeded_is_not_exit_evidence() {
        // The honest-capture contract: ok is NEVER invented. A prose
        // line starting "succeeded..." (the model narrating) following
        // a command is not a tool-result header — only the timed form
        // "succeeded in <dur>:" is exit-0 evidence.
        let text = "/usr/bin/bash -lc \"build\" in /tmp/w\nsucceeded in fixing the bug\n";
        let steps = parse_transcript(text);
        assert_eq!(
            steps[0].ok, None,
            "prose 'succeeded...' must not fabricate success: {steps:?}"
        );
        // The real timed header still binds exit 0.
        let real = "/usr/bin/bash -lc \"build\" in /tmp/w\nsucceeded in 0ms:\n";
        let steps = parse_transcript(real);
        assert_eq!(steps[0].ok, Some(true));
    }

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
    fn parse_is_total_on_empty_malformed_and_partial_input() {
        // Cycle-33 review: the transcript parser is production code that
        // runs on arbitrary log files — it must never panic on hostile
        // input, and empty/garbage/partial data must yield empty steps
        // (no fabricated commands).
        assert!(parse_transcript("").is_empty());
        assert!(parse_transcript("\n\n\n").is_empty());
        assert!(parse_transcript("total garbage, no markers at all").is_empty());
        // A truncated JSON event (unclosed object) must not panic.
        let truncated = r#"{"type":"text","part":{"text":"cmd: ls -la"#;
        assert!(parse_transcript(truncated).is_empty());
        // extract_result/completed are total too.
        assert!(extract_result("").is_none());
        assert!(extract_result("no result block here").is_none());
        assert!(!completed(""));
        assert!(!completed("partial: exited 0 in 1.0s")); // missing the success marker
    }

    #[test]
    fn look_ahead_binds_exit_code_to_the_command() {
        // P2-12 (hardening audit): the ` exited N in ...:` / ` succeeded`
        // header on the line AFTER a command becomes that command's ok.
        let text = "/usr/bin/bash -lc \"make verify\" in /tmp/w\n exited 0 in 42ms:\n...\n/usr/bin/bash -lc \"probe missing\" in /tmp/w\n exited 2 in 0ms:\n...\n/usr/bin/bash -lc \"build\" in /tmp/w\n succeeded in 0ms:\n...\n/usr/bin/bash -lc \"slow thing\" in /tmp/w\n(no exit header)\n";
        let steps = parse_transcript(text);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].ok, Some(true));
        assert_eq!(steps[1].ok, Some(false));
        assert_eq!(steps[2].ok, Some(true), "succeeded maps to exit 0");
        // No exit header -> unknown, never invented.
        assert_eq!(steps[3].ok, None);
    }

    #[test]
    fn look_ahead_binds_exits_on_the_real_exp003_log() {
        let path = std::path::Path::new("/tmp/opencode/exp003-work/codex.log");
        if !path.is_file() {
            eprintln!("skipping: {} absent on this host", path.display());
            return;
        }
        let text = std::fs::read_to_string(path).unwrap();
        let steps = parse_transcript(&text);
        let with_ok = steps.iter().filter(|s| s.ok.is_some()).count();
        assert!(
            with_ok > 0,
            "the real exp003 transcript carries exited/succeeded headers — {} exec steps should have ok bound",
            steps.iter().filter(|s| s.tool == "exec").count()
        );
        // The exit-2 probe from the review finding must surface as a
        // failed step (ok Some(false)), not be zeroed as unknown.
        assert!(
            steps.iter().any(|s| s.ok == Some(false)),
            "at least one real probe failure must be captured"
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
        // The marker counts only near the END of the transcript — the
        // prompt echo embeds it at the start.
        let padding = "x".repeat(200);
        assert!(completed(&format!(
            "{padding}done\n<promise>COMPLETE</promise>\n"
        )));
        assert!(!completed(&format!(
            "<promise>COMPLETE</promise>\n{padding}done\n"
        )));
    }

    #[test]
    fn parses_exp003_transcript_when_present() {
        // The EXP-003 transcript lives outside the repo (a scratch dir);
        // a clean host (CI) must not fail on it — conditional, same
        // discipline as the exp002 test below.
        let path = std::path::Path::new("/tmp/opencode/exp003-work/codex.log");
        if !path.is_file() {
            eprintln!("skipping: {} absent on this host", path.display());
            return;
        }
        let text = std::fs::read_to_string(path).unwrap();
        let steps = parse_transcript(&text);
        assert!(
            !steps.is_empty(),
            "must capture steps from the exp003 transcript"
        );
        let execs = steps.iter().filter(|s| s.tool == "exec").count();
        assert!(execs > 0, "bash -lc invocations must be captured");
        // The noise filters must keep npm-notice/help text OUT of the steps.
        assert!(
            steps
                .iter()
                .all(|s| !s.action.starts_with("npm notice") && s.action != "codex"),
            "noise must be filtered"
        );
    }

    #[test]
    fn parses_real_exp002_transcript_when_present() {
        // The EXP-002 transcript lives outside the repo (a scratch dir);
        // a clean host must not fail on it — the test is conditional.
        let path = std::path::Path::new("/tmp/opencode/codex-exp2/codex.log");
        if !path.is_file() {
            eprintln!("skipping: {} absent on this host", path.display());
            return;
        }
        let text = std::fs::read_to_string(path).unwrap();
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
            execs
                .iter()
                .any(|a| a.contains("make") || a.contains("bash -lc")),
            "make invocations must be captured: {execs:?}"
        );
    }

    #[test]
    fn unicode_case_folding_lines_do_not_panic_the_parser() {
        // 'İ' lowercases to "i̇" — two bytes from one. The old suffix
        // math indexed the ORIGINAL line with the LOWERCASED rest's
        // length and panicked on lines where the folding grew past the
        // prefix. Transcripts are untrusted input: never crash.
        let steps = parse_transcript("Wrote İİİİİİİ.txt\n");
        assert!(steps.is_empty(), "unmappable line is skipped: {steps:?}");
        // ASCII write lines still parse after the guard.
        let steps = parse_transcript("wrote src/main.rs\n");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].tool, "write");
        assert_eq!(steps[0].paths, vec!["src/main.rs"]);
    }
}
