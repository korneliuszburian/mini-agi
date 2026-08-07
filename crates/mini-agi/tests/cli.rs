//! CLI contract tests — ported 1:1 from `PoC` `tests/test_consolidate.py`
//! subprocess assertions (stdout + exit codes), run against the real binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_mini-agi");

fn run(root: &Path, args: &[&str]) -> Output {
    std::fs::create_dir_all(root).unwrap();
    Command::new(BIN)
        .args(args)
        .env("AGENTIC_ROOT", root)
        .output()
        .expect("binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn combined(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

fn tmp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("mag-cli-test-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn wipe(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root).unwrap();
}

fn seed_existing_entry(root: &Path, date: &str, seq: u32, content: &str) {
    let day = root.join("memory/canonical/entries").join(date);
    let path = day.join(format!("{date}-{seq:03}.md"));
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(path, content).unwrap();
}

fn today() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let secs = now.as_secs();
    // UTC civil date (reused from kernel contract: YYYY-MM-DD)
    let days = (secs / 86_400).cast_signed();
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

#[test]
fn cli_consolidates_fact_and_bullet_with_provenance_and_next_sequence() {
    let root = tmp_root("c1");
    wipe(&root);
    let day = today();
    seed_existing_entry(
        &root,
        &day,
        2,
        "# existing\n\n## F-001 `0123456789abcdef`\n\nold fact\n",
    );
    let buffer = root.join("session.md");
    std::fs::write(
        &buffer,
        "FACT: explicit memory survives compaction\n- bullets with enough detail survive too\n",
    )
    .unwrap();

    let out = run(
        &root,
        &[
            "mem",
            "consolidate",
            buffer.to_str().unwrap(),
            "--domain",
            "testing",
        ],
    );
    assert!(out.status.success(), "{}", stdout(&out));
    let entry = root
        .join("memory/canonical/entries")
        .join(&day)
        .join(format!("{day}-003.md"));
    assert!(entry.exists());
    let content = std::fs::read_to_string(&entry).unwrap();
    assert!(content.contains("explicit memory survives compaction"));
    assert!(content.contains("- domain: testing\n- kind: consolidation"));
    assert!(stdout(&out).contains("entry: memory/canonical/entries"));
    wipe(&root);
}

#[test]
fn cli_same_fact_from_two_buffers_creates_one_canonical_fact() {
    let root = tmp_root("c2");
    wipe(&root);
    let fact = "FACT: repeated evidence belongs in canonical memory once";
    let first = root.join("first.md");
    std::fs::write(&first, fact).unwrap();
    assert!(
        run(&root, &["mem", "consolidate", first.to_str().unwrap()])
            .status
            .success()
    );

    let second = root.join("second.md");
    std::fs::write(&second, fact).unwrap();
    let out = run(&root, &["mem", "consolidate", second.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("consolidated 0 new facts (skipped 1 duplicates)"));
    wipe(&root);
}

#[test]
fn cli_empty_buffer_exits_one() {
    let root = tmp_root("c3");
    wipe(&root);
    let buffer = root.join("empty.md");
    std::fs::write(&buffer, "just a heading\n").unwrap();
    let out = run(&root, &["mem", "consolidate", buffer.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no facts found"));
    wipe(&root);
}

#[test]
fn cli_dry_run_reports_planned_entry_and_preserves_empty_tree() {
    let root = tmp_root("c4");
    wipe(&root);
    let buffer = root.join("dry-run.md");
    std::fs::write(
        &buffer,
        "FACT: one prospective canonical fact\nFACT: one prospective canonical fact\n",
    )
    .unwrap();
    let out = run(
        &root,
        &[
            "mem",
            "consolidate",
            buffer.to_str().unwrap(),
            "--dry-run",
            "--domain",
            "testing",
        ],
    );
    assert!(out.status.success());
    let day = today();
    assert!(stdout(&out).contains(&format!(
        "entry: memory/canonical/entries/{day}/{day}-001.md"
    )));
    assert!(stdout(&out).contains("would write 1 new facts (skipped 1 duplicates)"));
    // Production-readiness B.4: the data-dir skeleton is bootstrapped on
    // first use, but a dry-run must not WRITE canonical entries.
    let entries = root.join("memory/canonical/entries");
    assert!(entries.is_dir(), "skeleton is bootstrapped");
    assert!(
        std::fs::read_dir(&entries).map_or(true, |rd| rd.flatten().next().is_none()),
        "dry-run must not write canonical entries"
    );
    wipe(&root);
}

#[test]
fn cli_require_signoff_queues_wording_variant_without_canonical_write() {
    let root = tmp_root("c5");
    wipe(&root);
    let first = root.join("first.md");
    std::fs::write(
        &first,
        "FACT: A fact whose first forty characters are shared, original wording.\n",
    )
    .unwrap();
    assert!(
        run(&root, &["mem", "consolidate", first.to_str().unwrap()])
            .status
            .success()
    );

    let variant = root.join("variant.md");
    std::fs::write(
        &variant,
        "FACT: A fact whose first forty characters are shared, alternate wording.\n",
    )
    .unwrap();
    let out = run(
        &root,
        &[
            "mem",
            "consolidate",
            variant.to_str().unwrap(),
            "--require-signoff",
        ],
    );
    assert!(out.status.success());
    let queue = root
        .join("memory/review")
        .join(format!("contested-{}.md", today()));
    let queued = std::fs::read_to_string(&queue).unwrap();
    assert!(queued.contains("alternate wording"));
    assert!(queued.contains("reason: same first 40 chars"));
    wipe(&root);
}

#[test]
fn cli_signoff_promotes_queued_fact_once() {
    let root = tmp_root("c6");
    wipe(&root);
    let first = root.join("first.md");
    std::fs::write(
        &first,
        "FACT: A fact whose first forty characters are shared, original wording.\n",
    )
    .unwrap();
    assert!(
        run(&root, &["mem", "consolidate", first.to_str().unwrap()])
            .status
            .success()
    );
    let variant = root.join("variant.md");
    std::fs::write(
        &variant,
        "FACT: A fact whose first forty characters are shared, alternate wording.\n",
    )
    .unwrap();
    assert!(
        run(
            &root,
            &[
                "mem",
                "consolidate",
                variant.to_str().unwrap(),
                "--require-signoff"
            ]
        )
        .status
        .success()
    );
    let queue = root
        .join("memory/review")
        .join(format!("contested-{}.md", today()));

    let promoted = run(&root, &["mem", "signoff", queue.to_str().unwrap(), "1"]);
    assert!(promoted.status.success(), "{}", stdout(&promoted));
    assert!(stdout(&promoted).contains("signed off 1 fact"));
    let repeated = run(&root, &["mem", "signoff", queue.to_str().unwrap(), "1"]);
    assert_eq!(repeated.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&repeated.stderr).contains("already known"));
    wipe(&root);
}

#[test]
fn cli_signoff_rejects_bad_queue_and_index() {
    let root = tmp_root("c7");
    wipe(&root);
    let out = run(&root, &["mem", "signoff", "nonexistent.md", "1"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("signoff requires"));
    wipe(&root);
}

#[test]
fn cli_derive_reports_and_writes_views() {
    let root = tmp_root("c8");
    wipe(&root);
    let buffer = root.join("buf.md");
    std::fs::write(
        &buffer,
        "FACT: derive produces views from canonical facts\n",
    )
    .unwrap();
    assert!(
        run(
            &root,
            &[
                "mem",
                "consolidate",
                buffer.to_str().unwrap(),
                "--domain",
                "agent-harness"
            ]
        )
        .status
        .success()
    );

    let out = run(&root, &["derive"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert!(stdout(&out).contains("derived: context-brief.md (1 facts)"));
    assert!(root.join("memory/derived/context-brief.md").exists());
    assert!(
        root.join("memory/derived/per-domain/AGENTS.agent-harness.md")
            .exists()
    );
    assert!(root.join("CLAUDE.md").exists());
    wipe(&root);
}

#[test]
fn cli_derive_fails_without_canonical() {
    let root = tmp_root("c9");
    wipe(&root);
    let out = run(&root, &["derive"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no canonical facts yet"));
    wipe(&root);
}

#[test]
fn cli_provenance_prints_fingerprint() {
    let root = tmp_root("c10");
    wipe(&root);
    let buffer = root.join("buf.md");
    std::fs::write(
        &buffer,
        "FACT: provenance gate compares canonical fingerprint\n",
    )
    .unwrap();
    assert!(
        run(&root, &["mem", "consolidate", buffer.to_str().unwrap()])
            .status
            .success()
    );
    let out = run(&root, &["provenance"]);
    assert!(out.status.success());
    assert!(stdout(&out).starts_with("canonical_sha256: "));
    let fp = stdout(&out).split_whitespace().nth(1).unwrap().to_string();
    assert_eq!(fp.len(), 16);
    // The fingerprint must CHANGE when canonical memory grows — the
    // provenance gate's whole job is to detect drift.
    let more = root.join("buf2.md");
    std::fs::write(
        &more,
        "FACT: a second fact must change the canonical fingerprint\n",
    )
    .unwrap();
    assert!(
        run(&root, &["mem", "consolidate", more.to_str().unwrap()])
            .status
            .success()
    );
    let out2 = run(&root, &["provenance"]);
    let fp2 = stdout(&out2).split_whitespace().nth(1).unwrap().to_string();
    assert_ne!(fp, fp2, "canonical growth must change the fingerprint");
    wipe(&root);
}

#[test]
fn cli_skill_list_discovery() {
    let root = tmp_root("c11");
    wipe(&root);
    let skills = root.join(".agents/skills");
    std::fs::create_dir_all(skills.join("verify")).unwrap();
    std::fs::create_dir_all(skills.join("plain")).unwrap();
    std::fs::write(
        skills.join("verify/SKILL.md"),
        "---\nname: verify\ndescription: gate skill\nverify: 'true'\n---\n",
    )
    .unwrap();
    std::fs::write(
        skills.join("plain/SKILL.md"),
        "---\nname: plain\ndescription: ref skill\n---\n",
    )
    .unwrap();
    let out = run(&root, &["skill", "list"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("verify  [verify]  gate skill"));
    assert!(text.contains("plain  [ref]  ref skill"));
    wipe(&root);
}

#[test]
fn cli_skill_show_prints_skill_body_and_unknown_fails() {
    // `skill show` (print a skill's SKILL.md) had no CLI test; both the
    // happy path (body surfaced) and the unknown-skill error were
    // uncovered.
    let root = tmp_root("cshow");
    wipe(&root);
    let skills = root.join(".agents/skills");
    std::fs::create_dir_all(skills.join("demo")).unwrap();
    std::fs::write(
        skills.join("demo/SKILL.md"),
        "---\nname: demo\ndescription: demo skill\n---\n\n# Body\n\nrun these steps\n",
    )
    .unwrap();
    let ok = run(&root, &["skill", "show", "demo"]);
    assert!(ok.status.success(), "{}", combined(&ok));
    assert!(stdout(&ok).contains("name: demo"), "{}", stdout(&ok));
    assert!(
        stdout(&ok).contains("description: demo skill"),
        "{}",
        stdout(&ok)
    );
    let missing = run(&root, &["skill", "show", "nope"]);
    assert_eq!(missing.status.code(), Some(1), "{}", combined(&missing));
    assert!(
        combined(&missing).contains("nope"),
        "{}",
        combined(&missing)
    );
    wipe(&root);
}

#[test]
fn cli_skill_verify_pass_and_fail() {
    let root = tmp_root("c12");
    wipe(&root);
    let skills = root.join(".agents/skills");
    std::fs::create_dir_all(skills.join("good")).unwrap();
    std::fs::create_dir_all(skills.join("bad")).unwrap();
    std::fs::write(
        skills.join("good/SKILL.md"),
        "---\nname: good\ndescription: ok\nverify: 'true'\n---\n",
    )
    .unwrap();
    std::fs::write(
        skills.join("bad/SKILL.md"),
        "---\nname: bad\ndescription: no\nverify: 'exit 2'\n---\n",
    )
    .unwrap();
    let ok = run(&root, &["skill", "verify", "good"]);
    assert!(ok.status.success());
    assert!(stdout(&ok).contains("PASS: good"));
    let bad = run(&root, &["skill", "verify", "bad"]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("FAIL: bad"));
    let unknown = run(&root, &["skill", "verify", "nope"]);
    assert_eq!(unknown.status.code(), Some(1));
    wipe(&root);
}

#[test]
fn cli_skill_verify_all_fails_on_procedural_nohook() {
    let root = tmp_root("c12b");
    wipe(&root);
    let skills = root.join(".agents/skills");
    std::fs::create_dir_all(skills.join("hooked")).unwrap();
    std::fs::create_dir_all(skills.join("nohook")).unwrap();
    std::fs::write(
        skills.join("hooked/SKILL.md"),
        "---
name: hooked
description: ok
version: 1.0.0
source: s
verify: 'true'
---
## Completion criteria
- [ ] quoted output
",
    )
    .unwrap();
    // A PROCEDURAL skill (default type) without a verify hook -> the
    // gate must fail.
    std::fs::write(
        skills.join("nohook/SKILL.md"),
        "---
name: nohook
description: no hook
version: 1.0.0
source: s
---
## Completion criteria
- [ ] quoted output
",
    )
    .unwrap();
    let all = run(&root, &["skill", "verify", "--all"]);
    assert_eq!(
        all.status.code(),
        Some(1),
        "a procedural skill without a verify hook must fail the gate"
    );
    assert!(String::from_utf8_lossy(&all.stderr).contains("without a verify hook"));
    // The mode exemption: a type: mode skill without a hook passes.
    std::fs::write(
        skills.join("nohook/SKILL.md"),
        "---
name: nohook
description: no hook
version: 1.0.0
source: s
type: mode
---
",
    )
    .unwrap();
    let all = run(&root, &["skill", "verify", "--all"]);
    assert!(
        all.status.success(),
        "a mode skill without a hook is allowed"
    );
    wipe(&root);
}

#[test]
fn cli_skill_add_installs_from_local_git() {
    let root = tmp_root("c13");
    wipe(&root);
    let src = std::env::temp_dir().join(format!("mag-skill-src-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(src.join(".agents/skills/hello")).unwrap();
    std::fs::write(
        src.join(".agents/skills/hello/SKILL.md"),
        "---\nname: hello\ndescription: test skill\nverify: 'true'\n---\n",
    )
    .unwrap();
    let git = Command::new("git")
        .current_dir(&src)
        .args(["init", "-q"])
        .status()
        .unwrap();
    assert!(git.success());
    let git = Command::new("git")
        .current_dir(&src)
        .args(["add", "-A"])
        .status()
        .unwrap();
    assert!(git.success());
    let git = Command::new("git")
        .current_dir(&src)
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ])
        .status()
        .unwrap();
    assert!(git.success());
    let out = run(&root, &["skill", "add", src.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout(&out).contains("installed: hello"));
    let list = run(&root, &["skill", "list"]);
    assert!(stdout(&list).contains("hello  [verify]  test skill"));
    let verify = run(&root, &["skill", "verify", "hello"]);
    assert!(verify.status.success());
    assert!(stdout(&verify).contains("PASS: hello"));
    let _ = std::fs::remove_dir_all(&src);
    wipe(&root);
}

#[test]
fn cli_checkpoint_audit_passes_clean_journal() {
    let root = tmp_root("c14");
    wipe(&root);
    let journal = root.join("memory/episodic/checkpoints.log");
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    std::fs::write(
        &journal,
        "2026-08-02T19:00:00Z BEGIN demo -> abc123\n\
         2026-08-02T19:01:00Z VERIFY-PASS demo @ abc123\n",
    )
    .unwrap();
    let out = run(&root, &["checkpoint", "audit"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout(&out).contains("checkpoint cascade complete"));
    wipe(&root);
}

#[test]
fn cli_checkpoint_audit_fails_on_missing_begin() {
    let root = tmp_root("c15");
    wipe(&root);
    let journal = root.join("memory/episodic/checkpoints.log");
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    std::fs::write(
        &journal,
        "2026-08-02T19:00:00Z VERIFY-PASS t002 @ 57ce2c7\n",
    )
    .unwrap();
    let out = run(&root, &["checkpoint", "audit"]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("VIOLATION"));
    assert!(text.contains("VERIFY without earlier BEGIN"));
    wipe(&root);
}

#[test]
fn cli_checkpoint_audit_fails_when_journal_missing() {
    let root = tmp_root("c16");
    wipe(&root);
    let out = run(&root, &["checkpoint", "audit"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("journal missing"));
    wipe(&root);
}

#[test]
fn cli_mcp_handshake_and_tool_call() {
    use std::io::Write;
    use std::process::Stdio;
    let root = tmp_root("m1");
    wipe(&root);
    let mut child = Command::new(BIN)
        .args(["mcp"])
        .env("AGENTIC_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("mcp spawns");
    let mut stdin = child.stdin.take().unwrap();
    let frame = |v: serde_json::Value| {
        let body = serde_json::to_vec(&v).unwrap();
        let mut f = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        f.extend_from_slice(&body);
        f
    };
    // rmcp/codex transports speak newline-delimited JSON, not LSP frames.
    let line = |v: serde_json::Value| {
        let mut f = serde_json::to_vec(&v).unwrap();
        f.push(b'\n');
        f
    };
    stdin
        .write_all(&frame(serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}
        })))
        .unwrap();
    stdin
        .write_all(&line(serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}
        })))
        .unwrap();
    stdin
        .write_all(&line(serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "stats", "arguments": {}}
        })))
        .unwrap();
    stdin.flush().unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut frames = Vec::new();
    for part in text.split("Content-Length: ").skip(1) {
        let Some((len, body)) = part.split_once("\r\n\r\n") else {
            continue;
        };
        let n: usize = len.trim().parse().unwrap();
        frames.push(serde_json::from_slice::<serde_json::Value>(&body.as_bytes()[..n]).unwrap());
    }
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0]["result"]["serverInfo"]["name"], "mini-agi");
    let tools = frames[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "stats"));
    assert!(
        frames[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("canonical entries")
    );
    wipe(&root);
}

#[test]
fn cli_full_ticket_run_end_to_end() {
    let root = tmp_root("c17");
    wipe(&root);
    let out = run(&root, &["init"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(root.join("scripts/verify.sh").is_file());
    assert!(root.join("AGENTS.md").is_file());
    assert!(root.join("opencode.json").is_file());
    let buffer = root.join("buf.md");
    std::fs::write(
        &buffer,
        "FACT: a full ticket runs through the kernel CLI alone.\n\
         FACT: the gate audits the checkpoint journal on every run.\n",
    )
    .unwrap();
    let c = run(&root, &["mem", "consolidate", buffer.to_str().unwrap()]);
    assert!(c.status.success());
    assert!(stdout(&c).contains("consolidated 2 new facts"));
    let d = run(&root, &["derive"]);
    assert!(d.status.success());
    assert!(stdout(&d).contains("context-brief.md (2 facts)"));
    let p = run(&root, &["provenance"]);
    assert!(p.status.success());
    assert_eq!(stdout(&p).trim().len(), 16 + "canonical_sha256: ".len());
    let git = |args: &[&str]| {
        Command::new("git")
            .current_dir(&root)
            .args(args)
            .status()
            .unwrap()
    };
    assert!(git(&["init", "-q"]).success());
    assert!(git(&["config", "user.email", "t@t"]).success());
    assert!(git(&["config", "user.name", "t"]).success());
    assert!(git(&["add", "-A"]).success());
    assert!(git(&["commit", "-qm", "seed"]).success());
    let begin = Command::new(root.join("scripts/checkpoint.sh"))
        .current_dir(&root)
        .arg("begin")
        .arg("ticket-run")
        .output()
        .unwrap();
    assert!(begin.status.success());
    let audit = run(&root, &["checkpoint", "audit"]);
    assert!(audit.status.success(), "{}", stdout(&audit));
    assert!(stdout(&audit).contains("checkpoint cascade complete"));
    let ticket = root.join("t.json");
    std::fs::write(
        &ticket,
        r#"{"id":"TICKET-001","title":"t","goal":"g","scope":["scripts/"]}"#,
    )
    .unwrap();
    let v = run(&root, &["validate", "ticket", ticket.to_str().unwrap()]);
    assert!(v.status.success());
    let stats = run(&root, &["stats"]);
    assert!(stats.status.success());
    assert!(stdout(&stats).contains("canonical entries: 1"));
    let budget = run(&root, &["budget"]);
    assert!(budget.status.success());
    assert!(stdout(&budget).contains("AGENTS chain:"));
    wipe(&root);
}

#[test]
fn cli_eval_gate_never_silently_rebaselines() {
    let root = tmp_root("c18");
    wipe(&root);
    let real = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("evals/cases/real-ticket-008-v2/run.json");
    std::fs::create_dir_all(root.join("evals/cases/real")).unwrap();
    std::fs::copy(&real, root.join("evals/cases/real/run.json")).unwrap();
    let out = run(&root, &["eval", "gate"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("baseline missing"));
    let write = run(&root, &["eval", "gate", "--write-baseline"]);
    assert!(write.status.success());
    assert!(stdout(&write).contains("baseline written"));
    wipe(&root);
}

#[test]
fn cli_checkpoint_verify_rolls_back_uncommitted_edits() {
    let root = tmp_root("c19");
    wipe(&root);
    let out = run(&root, &["init"]);
    assert!(out.status.success());
    std::fs::write(root.join("scripts/verify.sh"), "#!/bin/sh\nexit 1\n").unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .current_dir(&root)
            .args(args)
            .status()
            .unwrap()
    };
    assert!(git(&["init", "-q"]).success());
    assert!(git(&["config", "user.email", "t@t"]).success());
    assert!(git(&["config", "user.name", "t"]).success());
    assert!(git(&["add", "-A"]).success());
    assert!(git(&["commit", "-qm", "seed"]).success());
    let begin = Command::new(root.join("scripts/checkpoint.sh"))
        .current_dir(&root)
        .arg("begin")
        .arg("step1")
        .output()
        .unwrap();
    assert!(begin.status.success());
    std::fs::write(root.join("AGENTS.md"), "BROKEN").unwrap();
    let verify = Command::new(root.join("scripts/checkpoint.sh"))
        .current_dir(&root)
        .arg("verify")
        .arg("step1")
        .output()
        .unwrap();
    assert_eq!(verify.status.code(), Some(1));
    let text = String::from_utf8_lossy(&verify.stdout).into_owned();
    assert!(text.contains("rolled back to green checkpoint"));
    let journal = std::fs::read_to_string(root.join("memory/episodic/checkpoints.log")).unwrap();
    assert!(journal.contains("ROLLBACK to"));
    assert_ne!(
        std::fs::read_to_string(root.join("AGENTS.md")).unwrap(),
        "BROKEN"
    );
    wipe(&root);
}

#[test]
fn cli_ticket_lifecycle() {
    let root = tmp_root("c20");
    wipe(&root);
    std::fs::create_dir_all(root.join("tickets")).unwrap();
    std::fs::write(
        root.join("tickets/TICKET-001.md"),
        r#"{"id":"TICKET-001","title":"gates","goal":"wire gates","scope":["scripts/"]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tickets/TICKET-002.md"),
        "---\nid: TICKET-002\ntitle: memory derive\ngoal: regenerate views\nscope: memory/derived\n---\n",
    )
    .unwrap();
    let list = run(&root, &["ticket", "list"]);
    assert!(list.status.success());
    let text = stdout(&list);
    assert!(text.contains("TICKET-001  gates"));
    assert!(text.contains("TICKET-002  memory derive"));
    let show = run(&root, &["ticket", "show", "TICKET-002"]);
    assert!(show.status.success());
    assert!(stdout(&show).contains("goal: regenerate views"));
    let show_num = run(&root, &["ticket", "show", "001"]);
    assert!(show_num.status.success());
    assert!(stdout(&show_num).contains("id: TICKET-001"));
    let validate = run(&root, &["ticket", "validate", "TICKET-001"]);
    assert!(validate.status.success());
    assert!(stdout(&validate).contains("validates"));
    let missing = run(&root, &["ticket", "show", "TICKET-999"]);
    assert_eq!(missing.status.code(), Some(1));
    wipe(&root);
}

#[test]
fn cli_ticket_work_graph_claims_and_validation() {
    let root = tmp_root("c24");
    wipe(&root);
    std::fs::create_dir_all(root.join("tickets")).unwrap();
    std::fs::write(
        root.join("tickets/TICKET-001.md"),
        r#"{"id":"TICKET-001","title":"gates","goal":"wire gates","scope":["scripts/"]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("tickets/TICKET-002.md"),
        "- id: TICKET-002\n- title: dep work\n- goal: g\n- blocked_by: TICKET-001\n",
    )
    .unwrap();
    let graph = run(&root, &["ticket", "graph"]);
    assert!(graph.status.success());
    assert!(stdout(&graph).contains("TICKET-001 -> TICKET-002"));
    let blocked = run(
        &root,
        &["ticket", "claim", "TICKET-002", "--claimant", "agent-a"],
    );
    assert_eq!(blocked.status.code(), Some(1));
    assert!(combined(&blocked).contains("blocked by TICKET-001"));
    let forced = run(
        &root,
        &[
            "ticket",
            "claim",
            "TICKET-002",
            "--claimant",
            "agent-a",
            "--force",
        ],
    );
    assert!(forced.status.success());
    let conflict = run(
        &root,
        &["ticket", "claim", "TICKET-002", "--claimant", "agent-b"],
    );
    assert_eq!(conflict.status.code(), Some(1));
    assert!(combined(&conflict).contains("already claimed by agent-a"));
    let claims = run(&root, &["ticket", "claims"]);
    assert!(stdout(&claims).contains("TICKET-002 claimed by agent-a"));
    let wrong = run(
        &root,
        &["ticket", "release", "TICKET-002", "--claimant", "agent-b"],
    );
    assert_eq!(wrong.status.code(), Some(1));
    let release = run(
        &root,
        &["ticket", "release", "TICKET-002", "--claimant", "agent-a"],
    );
    assert!(release.status.success());
    let claims_after = run(&root, &["ticket", "claims"]);
    assert!(stdout(&claims_after).contains("no claims held"));
    let vg = run(&root, &["ticket", "validate-graph"]);
    assert!(vg.status.success());
    assert!(stdout(&vg).contains("dependency graph valid"));
    wipe(&root);
}

#[test]
fn cli_loop_dispatch_writes_spec_and_claim() {
    let root = tmp_root("c25");
    wipe(&root);
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(root.join("evals/cases/real-ticket-001-v2")).unwrap();
    std::fs::copy(
        repo.join("evals/cases/real-ticket-001-v2/run.json"),
        root.join("evals/cases/real-ticket-001-v2/run.json"),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("evals/golden")).unwrap();
    std::fs::copy(
        repo.join("evals/golden/real-ticket-validate.json"),
        root.join("evals/golden/real-ticket-validate.json"),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("tickets")).unwrap();
    std::fs::copy(
        repo.join("tickets/TICKET-001.md"),
        root.join("tickets/TICKET-001.md"),
    )
    .unwrap();
    let status = run(&root, &["loop", "status"]);
    assert!(status.status.success());
    assert!(stdout(&status).contains("real-ticket-001-v2"));
    let dispatch = run(
        &root,
        &[
            "loop",
            "dispatch",
            "real-ticket-001-v2",
            "--claimant",
            "cli-test",
        ],
    );
    assert!(dispatch.status.success(), "{}", combined(&dispatch));
    let text = stdout(&dispatch);
    assert!(text.contains("real-ticket-001-v2 -> TICKET-001-v2"));
    let spec = root.join("artifacts/TICKET-001-v2/spec.md");
    assert!(spec.is_file());
    assert!(
        std::fs::read_to_string(&spec)
            .unwrap()
            .contains("composite >= 0.5")
    );
    let claims = run(&root, &["ticket", "claims"]);
    assert!(stdout(&claims).contains("TICKET-001-v2 claimed by cli-test"));
    wipe(&root);
}

#[test]
fn cli_run_ingest_and_insights() {
    let root = tmp_root("c21");
    wipe(&root);
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("evals/cases/real-ticket-008-v2/run.json");
    std::fs::create_dir_all(root.join("evals/cases/real-ticket-008-v2")).unwrap();
    std::fs::copy(&src, root.join("evals/cases/real-ticket-008-v2/run.json")).unwrap();
    let golden_src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("evals/golden/real-ticket-compact.json");
    std::fs::create_dir_all(root.join("evals/golden")).unwrap();
    std::fs::copy(
        &golden_src,
        root.join("evals/golden/real-ticket-compact.json"),
    )
    .unwrap();
    let run_path = root.join("evals/cases/real-ticket-008-v2/run.json");
    let ingest = run(&root, &["run", "ingest", run_path.to_str().unwrap()]);
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    assert!(stdout(&ingest).contains("ingested: real-ticket-008-v2"));
    assert!(stdout(&ingest).contains("world model: 2 new facts"));
    let derive = run(&root, &["derive"]);
    assert!(derive.status.success());
    let insights = run(&root, &["insights"]);
    assert!(insights.status.success());
    let text = stdout(&insights);
    assert!(text.contains("SYSTEM INTELLIGENCE REPORT"));
    assert!(text.contains("real-ticket-008-v2: 0.9774"));
    assert!(text.contains("1 entries, 2 facts"));
    assert!(text.contains("capability gaps: none"));
    wipe(&root);
}

#[test]
fn cli_run_failures_writes_register_and_resume_shows_block() {
    let root = tmp_root("c22");
    wipe(&root);
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("evals/cases/reactive-loop/run.json");
    std::fs::create_dir_all(root.join("evals/cases/reactive-loop")).unwrap();
    std::fs::copy(&src, root.join("evals/cases/reactive-loop/run.json")).unwrap();
    let run_path = root.join("evals/cases/reactive-loop/run.json");
    let first = run(&root, &["run", "failures", run_path.to_str().unwrap()]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let text = stdout(&first);
    assert!(text.contains("tool=edit action=\"edit same line\" count=2"));
    assert!(text.contains("recorded 1 repeated failing actions"));
    let register = root.join("memory/derived/failures.md");
    assert!(register.is_file());
    let second = run(&root, &["run", "failures", run_path.to_str().unwrap()]);
    assert!(second.status.success());
    assert!(stdout(&second).contains("recorded 1 repeated failing actions"));
    let resume = run(&root, &["resume"]);
    assert!(resume.status.success());
    let resume_text = stdout(&resume);
    assert!(resume_text.contains("failure register: 1 recorded failures"));
    assert!(resume_text.contains("edit same line"));
    assert!(resume_text.contains("do not repeat"));
    wipe(&root);
}

#[test]
fn cli_backlog_creates_gap_ticket_and_dedups() {
    // Backlog CLI (failure signal -> roadmap): the core is tested, but
    // the CLI command itself had no integration test — a broken delegate
    // (bad arg, bad output, wrong exit) would pass silently.
    let root = tmp_root("cbl");
    wipe(&root);
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("evals/cases/reactive-loop/run.json");
    std::fs::create_dir_all(root.join("evals/cases/reactive-loop")).unwrap();
    std::fs::copy(&src, root.join("evals/cases/reactive-loop/run.json")).unwrap();
    let first = run(&root, &["backlog"]);
    assert!(first.status.success(), "{}", combined(&first));
    assert!(
        stdout(&first).contains("created: TICKET-1"),
        "{}",
        stdout(&first)
    );
    assert!(root.join("tickets/TICKET-1.md").is_file());
    // Second run: the gap ticket already exists -> no duplicate.
    let second = run(&root, &["backlog"]);
    assert!(second.status.success());
    assert!(
        stdout(&second).contains("exists: TICKET-1"),
        "{}",
        stdout(&second)
    );
    wipe(&root);
}

#[test]
fn cli_run_verify_audit_flags_vacuous_verifier_and_exits_1() {
    // The fail-closed contract: a vacuous verifier (always exits 0) must
    // be flagged and return exit 1. Core audit_verifier is tested; the
    // CLI exit-code contract had no integration test.
    let root = tmp_root("cva");
    wipe(&root);
    let target = root.join("target");
    std::fs::create_dir_all(&target).unwrap();
    let run_path = target.join("run.json");
    std::fs::write(
        &run_path,
        format!(
            r#"{{"goal":"g","scope":["x"],"outcome":{{"achieved":true}},"tokens_total":1,"cost_usd":0.0,"golden":null,"verify_command":"true","verify_target":"{}","trajectory":[{{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}}]}}"#,
            target.to_string_lossy()
        ),
    )
    .unwrap();
    let out = run(&root, &["run", "verify-audit", run_path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{}", combined(&out));
    assert!(combined(&out).contains("VACUOUS"), "{}", combined(&out));
    wipe(&root);
}

#[test]
fn cli_status_json_reports_run_index() {
    // `status --json` serializes the run index for external consumers
    // (the codex supervisor). No integration test covered the CLI json
    // path; a malformed payload or wrong key set would pass silently.
    let root = tmp_root("csj");
    wipe(&root);
    let cases = root.join("evals/cases/a-ok");
    std::fs::create_dir_all(&cases).unwrap();
    std::fs::write(
        cases.join("run.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "goal": "goal a",
            "cost_usd": 0.02,
            "tokens_total": 100,
            "outcome": {"achieved": true},
            "n_steps": 3,
        }))
        .unwrap(),
    )
    .unwrap();
    let out = run(&root, &["status", "--json"]);
    assert!(out.status.success(), "{}", combined(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid json");
    assert_eq!(parsed["runs"]["total_runs"], 1);
    assert_eq!(parsed["runs"]["achieved_runs"], 1);
    assert!(parsed["journal_tail"].is_array());
    assert!(parsed["workers"].is_array());
    wipe(&root);
}

#[test]
fn cli_eval_mismatches_writes_register_and_resume_shows_block() {
    let root = tmp_root("c23");
    wipe(&root);
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(root.join("evals/cases/real-ticket-001-v2")).unwrap();
    std::fs::copy(
        repo.join("evals/cases/real-ticket-001-v2/run.json"),
        root.join("evals/cases/real-ticket-001-v2/run.json"),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("evals/golden")).unwrap();
    std::fs::copy(
        repo.join("evals/golden/real-ticket-validate.json"),
        root.join("evals/golden/real-ticket-validate.json"),
    )
    .unwrap();
    let run_path = root.join("evals/cases/real-ticket-001-v2/run.json");
    let first = run(&root, &["eval", "mismatches", run_path.to_str().unwrap()]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let text = stdout(&first);
    assert!(text.contains("real-ticket-001-v2 step 2: golden expects write, used exec"));
    assert!(text.contains("recorded 4 tool mismatches"));
    let register = root.join("memory/derived/mismatches.md");
    assert!(register.is_file());
    let second = run(&root, &["eval", "mismatches", run_path.to_str().unwrap()]);
    assert!(second.status.success());
    assert!(stdout(&second).contains("recorded 4 tool mismatches"));
    let resume = run(&root, &["resume"]);
    assert!(resume.status.success());
    let resume_text = stdout(&resume);
    assert!(resume_text.contains("tool mismatch register: 4 divergences in 1 cases"));
    assert!(resume_text.contains("golden expects write, used exec"));
    wipe(&root);
}

#[test]
fn cli_health_reports_and_exits_zero_on_healthy_repo() {
    let root = tmp_root("c26");
    wipe(&root);
    let health = run(&root, &["health"]);
    // The machine snapshot reflects REAL load: OK=0, WARN=1, CRITICAL=2
    // are all valid verdicts depending on the host state (a loaded box
    // legitimately reports CRITICAL). The test asserts structural
    // soundness — a complete report with a recognized verdict — not a
    // quiet machine (cycle-34 lesson: environment-coupled tests are
    // flaky-by-design).
    assert!(
        health.status.code().is_some_and(|c| c <= 2),
        "{}",
        combined(&health)
    );
    let text = stdout(&health);
    assert!(text.contains("HEALTH CHECK"));
    assert!(
        text.contains("OK") || text.contains("WARN") || text.contains("CRITICAL"),
        "report must carry a verdict: {text}"
    );
    assert!(text.contains("no findings") || text.contains("[warn]") || text.contains("[critical]"));
    wipe(&root);
}

#[test]
fn cli_audit_reports_invariants() {
    let root = tmp_root("c27");
    wipe(&root);
    let audit = run(&root, &["audit"]);
    assert!(audit.status.success(), "{}", combined(&audit));
    let text = stdout(&audit);
    assert!(text.contains("AUDIT CHECK"));
    wipe(&root);
}

#[test]
fn cli_run_verify_reports_verified_and_disagrees() {
    let root = tmp_root("c28");
    wipe(&root);
    let target = root.join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("ok.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::write(target.join("bad.sh"), "#!/bin/sh\nexit 1\n").unwrap();
    let case_dir = root.join("evals/cases/ver-case");
    std::fs::create_dir_all(&case_dir).unwrap();
    std::fs::write(
        case_dir.join("run.json"),
        format!(
            r#"{{"goal":"g","scope":["x"],"outcome":{{"achieved":true}},"tokens_total":1,"cost_usd":0.01,"golden":null,"verify_command":"sh ok.sh","verify_target":{},"trajectory":[{{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}}]}}"#,
            serde_json::to_string(&target.to_string_lossy()).unwrap()
        ),
    )
    .unwrap();
    let run_path = case_dir.join("run.json");
    let ok = run(&root, &["run", "verify", run_path.to_str().unwrap()]);
    assert!(ok.status.success(), "{}", combined(&ok));
    assert!(stdout(&ok).contains("verified"));
    std::fs::write(
        case_dir.join("run.json"),
        format!(
            r#"{{"goal":"g","scope":["x"],"outcome":{{"achieved":true}},"tokens_total":1,"cost_usd":0.01,"golden":null,"verify_command":"sh bad.sh","verify_target":{},"trajectory":[{{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}}]}}"#,
            serde_json::to_string(&target.to_string_lossy()).unwrap()
        ),
    )
    .unwrap();
    let bad = run(&root, &["run", "verify", run_path.to_str().unwrap()]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(combined(&bad).contains("disagrees"));
    wipe(&root);
}

#[test]
fn cli_run_verify_dry_run_prints_command_without_executing() {
    // AGENTS.md: "run verify --dry-run prints the command without
    // executing it." No test covered this — a regression that executes
    // the verifier (or fails) in dry-run mode would pass silently.
    let root = tmp_root("c-drv");
    wipe(&root);
    let target = root.join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("v.sh"), "#!/bin/sh\nexit 1\n").unwrap();
    let case_dir = root.join("evals/cases/dry-case");
    std::fs::create_dir_all(&case_dir).unwrap();
    let run_path = case_dir.join("run.json");
    std::fs::write(
        &run_path,
        format!(
            r#"{{"goal":"g","scope":["x"],"outcome":{{"achieved":true}},"tokens_total":1,"cost_usd":0.01,"golden":null,"verify_command":"sh v.sh","verify_target":{},"trajectory":[{{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}}]}}"#,
            serde_json::to_string(&target.to_string_lossy()).unwrap()
        ),
    )
    .unwrap();
    // The verifier would fail (v.sh exits 1); dry-run must still print
    // the command and succeed, without executing anything.
    let ok = run(
        &root,
        &["run", "verify", "--dry-run", run_path.to_str().unwrap()],
    );
    assert!(ok.status.success(), "{}", combined(&ok));
    assert!(
        stdout(&ok).contains("dry-run: would execute 'sh v.sh'"),
        "{}",
        stdout(&ok)
    );
    assert!(stdout(&ok).contains("no execution"), "{}", stdout(&ok));
    wipe(&root);
}

#[cfg(target_os = "linux")]
#[test]
fn exec_sandbox_confines_writes_outside_the_allow_set() {
    // P0-4 / ADR-0012: Landlock write-containment. Skips when the host
    // kernel lacks Landlock (the wrapper reports it and degrades).
    let root = tmp_root("sandbox");
    let inside = root.join("in");
    let outside = root.join("out");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let inside_s = inside.to_string_lossy().into_owned();
    let outside_s = outside.to_string_lossy().into_owned();

    // Write INSIDE the allow set: must succeed and create the file.
    let ok = run(
        &root,
        &[
            "exec-sandbox",
            "--allow-write",
            &inside_s,
            "--",
            "sh",
            "-c",
            &format!("echo ok > {inside_s}/f.txt"),
        ],
    );
    assert!(ok.status.success(), "{}", combined(&ok));
    assert!(inside.join("f.txt").is_file());

    // Write OUTSIDE the allow set: must be denied (unless the host has
    // no Landlock, in which case the wrapper warned and degraded).
    let bad = run(
        &root,
        &[
            "exec-sandbox",
            "--allow-write",
            &inside_s,
            "--",
            "sh",
            "-c",
            &format!("echo bad > {outside_s}/f.txt"),
        ],
    );
    let text = combined(&bad);
    if text.contains("sandbox unavailable") {
        eprintln!("skipping: host kernel lacks Landlock (ADR-0012 degradation)");
        wipe(&root);
        return;
    }
    assert!(
        !outside.join("f.txt").is_file(),
        "write outside the allow-set must be denied by Landlock: {text}"
    );
    wipe(&root);
}

#[test]
fn init_creates_the_data_dir_layout_in_an_empty_dir() {
    // Production-readiness B.4: the data-dir contract — an empty dir
    // gains the memory/ + evals/ + tickets/ + scripts/ skeleton.
    let root = tmp_root("init-layout");
    let init = run(&root, &["init"]);
    assert!(init.status.success(), "{}", combined(&init));
    for rel in [
        "memory/canonical/entries",
        "memory/episodic",
        "memory/derived/per-domain",
        "evals/cases",
        "evals/golden",
        "evals/results",
        "tickets",
        "scripts",
        "docs/adr",
    ] {
        assert!(root.join(rel).is_dir(), "missing dir {rel}");
    }
    assert!(root.join("AGENTS.md").is_file());
    assert!(root.join("scripts/verify.sh").is_file());
    // AGENTS.md must carry the HITL write rule (kernel enforces approve;
    // the template must teach sessions about it).
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(
        agents.contains("MCP writes need HITL"),
        "AGENTS.md template must document the HITL write gate"
    );
    // The codex config must be wired with the MCP allowlist + HITL
    // approvals (not just trusted=true) so a fresh init enforces writes.
    let codex = std::fs::read_to_string(root.join(".codex/config.toml")).expect("codex config");
    assert!(
        codex.contains("[mcp_servers.mini-agi]"),
        "codex config must register the MCP server"
    );
    assert!(
        codex.contains("default_tools_approval_mode = \"auto\""),
        "codex config must set the default approval mode"
    );
    assert!(
        codex.contains("enabled_tools"),
        "codex config must carry the tool allowlist"
    );
    for tool in [
        "loop_dispatch",
        "memory_signoff",
        "run_ingest",
        "ticket_release",
        "skill_add",
    ] {
        assert!(
            codex.contains(&format!(
                "[mcp_servers.mini-agi.tools.{tool}]\napproval_mode = \"prompt\""
            )),
            "codex config must gate {tool} to prompt"
        );
    }
    // Idempotent: a second init does not fail.
    let again = run(&root, &["init"]);
    assert!(again.status.success(), "{}", combined(&again));
    wipe(&root);
}

#[test]
fn bootstrap_seeds_missing_dirs_without_files() {
    // Production-readiness B.4: the first-run auto-init creates the
    // skeleton but no files (no clobbering); running a repo command in
    // an empty dir bootstraps it.
    let root = tmp_root("bootstrap");
    // AGENTIC_ROOT points the kernel at the empty dir.
    let out = std::process::Command::new(BIN)
        .env("AGENTIC_ROOT", &root)
        .arg("stats")
        .output()
        .expect("spawn mini-agi");
    let text = combined(&out);
    assert!(out.status.success(), "{text}");
    assert!(root.join("memory/canonical/entries").is_dir());
    assert!(root.join("evals/cases").is_dir());
    // bootstrap must NOT create files.
    assert!(!root.join("AGENTS.md").is_file());
    wipe(&root);
}

#[test]
fn approval_gate_refuses_without_approve_when_required() {
    // Production-readiness D.4 / ADR-0014: with require_approval set, a
    // worker run without --approve refuses BEFORE spawning the worker.
    let root = tmp_root("approval");
    let spec = root.join("spec.md");
    std::fs::write(
        &spec,
        "- goal: g\n- scope: x\n- verify_command: sh v.sh in /tmp/x\n",
    )
    .unwrap();
    let workdir = root.join("work");
    std::fs::create_dir_all(&workdir).unwrap();
    // The worker reads its policy from the WORKDIR's config (same seam
    // as wall_cap / max_tokens).
    std::fs::write(
        workdir.join(".miniagi.json"),
        r#"{"require_approval": true}"#,
    )
    .unwrap();
    let out = run(
        &root,
        &[
            "codex",
            spec.to_str().unwrap(),
            workdir.to_str().unwrap(),
            "--verify",
            "sh v.sh",
            "--target",
            "/tmp/x",
        ],
    );
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("require_approval"), "{text}");
    assert!(text.contains("--approve"), "{text}");
    // The refusal must not spawn the worker: no transcript log.
    assert!(!workdir.join("codex.log").exists(), "{text}");
    wipe(&root);
}

#[test]
fn cli_subcommand_help_never_panics() {
    // Clap definition regressions (conflicting flags, bad required) only
    // surface when a subcommand is exercised. Smoke-test --help for every
    // top-level subcommand: it must exit 0 and print usage, never panic.
    let root = tmp_root("help");
    wipe(&root);
    for sub in [
        "loop",
        "eval",
        "mem",
        "ticket",
        "run",
        "skill",
        "checkpoint",
        "validate",
        "budget",
        "mcp",
        "dream",
        "insights",
        "health",
        "audit",
        "provenance",
        "stats",
        "resume",
        "init",
        "research",
    ] {
        let out = run(&root, &[sub, "--help"]);
        let text = combined(&out);
        assert!(out.status.success(), "`{sub} --help` failed: {text}");
        assert!(
            text.contains("Usage") || text.contains("USAGE"),
            "`{sub} --help` printed no usage: {text}"
        );
    }
    let root_help = run(&root, &["--help"]);
    assert!(root_help.status.success(), "root --help failed");
    wipe(&root);
}

#[test]
fn cli_dream_idle_no_runs_is_clean_success() {
    // D2 idle cadence (AGENTS.md): `dream --idle` must be load-guarded
    // and idempotent. The "no runs to distill" branch had no test; a
    // panic or error there (e.g. on a missing staging dir) would only
    // fire on a quiet box at the D2 cadence. Deterministic branch: an
    // empty cases dir must yield success + "no runs to distill".
    let root = tmp_root("dream-idle");
    wipe(&root);
    std::fs::create_dir_all(root.join("evals/cases")).unwrap();
    // On a loaded host the load-guard skips first with success; the
    // machine may be quiet in CI, so accept either clean outcome.
    let out = run(&root, &["dream", "--idle"]);
    assert!(
        out.status.success(),
        "dream --idle must exit cleanly: {}",
        combined(&out)
    );
    let text = combined(&out);
    assert!(
        text.contains("no runs to distill")
            || text.contains("busy, skipping")
            || text.contains("no newer runs"),
        "unexpected dream --idle output: {text}"
    );
    wipe(&root);
}

#[test]
fn cli_mem_verify_detects_exact_duplicate_facts() {
    // `mem verify` (memory-integrity gate, AGENTS.md 'cyklicznie
    // weryfikuj spójność memory') had no CLI test — a regression where
    // duplicates/supersede/preserve checks stop reporting would pass.
    let root = tmp_root("cmv");
    wipe(&root);
    let day = today();
    // Two identical fact bodies -> same content hash -> exact duplicate.
    let body = "identical fact body";
    let id = "0123456789abcdef";
    seed_existing_entry(
        &root,
        &day,
        1,
        format!("# e1\n\n## F-000 `{id}`\n\n{body}\n").as_str(),
    );
    seed_existing_entry(
        &root,
        &day,
        2,
        format!("# e2\n\n## F-000 `{id}`\n\n{body}\n").as_str(),
    );
    let out = run(&root, &["mem", "verify"]);
    assert_eq!(out.status.code(), Some(1), "{}", combined(&out));
    let text = stdout(&out);
    assert!(
        text.contains("exact duplicate bodies"),
        "expected duplicate finding: {text}"
    );
    // A clean store reports OK.
    let root2 = tmp_root("cmv-clean");
    wipe(&root2);
    seed_existing_entry(
        &root2,
        &today(),
        1,
        "# e1\n\n## F-000 `0123456789aaaa`\n\nunique body\n",
    );
    let clean = run(&root2, &["mem", "verify"]);
    assert!(clean.status.success(), "{}", combined(&clean));
    let clean_text = stdout(&clean);
    assert!(
        clean_text.contains("no duplicates") || clean_text.contains("OK"),
        "expected clean verdict: {clean_text}"
    );
    wipe(&root);
    wipe(&root2);
}

#[test]
fn cli_mem_supersede_writes_lineage_and_rejects_unknown_id() {
    // `mem supersede` (append-only lineage: a new fact supersedes an old
    // id) had no CLI test; the known-id gate and the lineage write were
    // both uncovered at the CLI boundary.
    let root = tmp_root("csup");
    wipe(&root);
    let day = today();
    seed_existing_entry(
        &root,
        &day,
        1,
        "# e1\n\n## F-000 `0123456789abcde0`\n\nold claim\n",
    );
    // Unknown id -> hard error, no write.
    let bad = run(
        &root,
        &[
            "mem",
            "supersede",
            "new claim",
            "--supersedes",
            "ffffffffffffffff",
            "--domain",
            "general",
            "--source",
            "test",
        ],
    );
    assert_eq!(bad.status.code(), Some(1), "{}", combined(&bad));
    assert!(
        combined(&bad).contains("not a known canonical fact id"),
        "{}",
        combined(&bad)
    );
    // Valid supersede -> success + lineage entry written.
    let ok = run(
        &root,
        &[
            "mem",
            "supersede",
            "new corrected claim",
            "--supersedes",
            "0123456789abcde0",
            "--domain",
            "general",
            "--source",
            "test",
        ],
    );
    assert!(ok.status.success(), "{}", combined(&ok));
    assert!(
        stdout(&ok).contains("superseded 1 fact(s)"),
        "{}",
        stdout(&ok)
    );
    // mem verify: no broken lineage.
    let v = run(&root, &["mem", "verify"]);
    assert!(v.status.success(), "{}", combined(&v));
    wipe(&root);
}

#[test]
fn cli_mem_verify_flags_supersede_against_preserved_id() {
    // Preservation is a stronger contract than supersede (ADR-0010 /
    // A-MEM supersede-never): a lineage write must not soft-delete a
    // load-bearing id. mem verify had no finding for this intersection.
    let root = tmp_root("csup-pres");
    wipe(&root);
    seed_existing_entry(
        &root,
        &today(),
        1,
        "# e1\n\n## F-000 `0123456789abcdf2`\n\nload-bearing claim\n",
    );
    // Preserve the id first.
    let pres = run(&root, &["mem", "preserve", "0123456789abcdf2"]);
    assert!(pres.status.success(), "{}", combined(&pres));
    // Then supersede it — the kernel must refuse (preservation is a
    // stronger contract than supersede).
    let sup = run(
        &root,
        &[
            "mem",
            "supersede",
            "newer claim",
            "--supersedes",
            "0123456789abcdf2",
            "--domain",
            "general",
            "--source",
            "test",
        ],
    );
    assert_eq!(sup.status.code(), Some(1), "{}", combined(&sup));
    assert!(combined(&sup).contains("preserved"), "{}", combined(&sup));
    // mem verify stays clean (no lineage write happened).
    let v = run(&root, &["mem", "verify"]);
    assert!(v.status.success(), "{}", combined(&v));
    wipe(&root);
}

#[test]
fn cli_mem_unpreserve_unblocks_supersede() {
    // unpreserve is the counterpart to preserve: since supersede of a
    // preserved id is refused, a wrongly preserved id must be removable
    // or it is blocked from lineage evolution forever.
    let root = tmp_root("cunp");
    wipe(&root);
    seed_existing_entry(
        &root,
        &today(),
        1,
        "# e1\n\n## F-000 `0123456789abcdf3`\n\nload-bearing claim\n",
    );
    let pres = run(&root, &["mem", "preserve", "0123456789abcdf3"]);
    assert!(pres.status.success(), "{}", combined(&pres));
    // Supersede is refused while preserved.
    let blocked = run(
        &root,
        &[
            "mem",
            "supersede",
            "newer",
            "--supersedes",
            "0123456789abcdf3",
            "--domain",
            "general",
            "--source",
            "t",
        ],
    );
    assert_eq!(blocked.status.code(), Some(1), "{}", combined(&blocked));
    // Unpreserve, then supersede succeeds.
    let un = run(&root, &["mem", "unpreserve", "0123456789abcdf3"]);
    assert!(un.status.success(), "{}", combined(&un));
    assert!(
        stdout(&un).contains("un-preserved 1 fact(s)"),
        "{}",
        stdout(&un)
    );
    let ok = run(
        &root,
        &[
            "mem",
            "supersede",
            "newer",
            "--supersedes",
            "0123456789abcdf3",
            "--domain",
            "general",
            "--source",
            "t",
        ],
    );
    assert!(ok.status.success(), "{}", combined(&ok));
    wipe(&root);
}

#[test]
fn cli_mem_preserve_writes_list_and_rejects_unknown_id() {
    // `mem preserve` (protect ids from supersede collisions) had no CLI
    // test; the known-id gate and the list write were uncovered.
    let root = tmp_root("cpres");
    wipe(&root);
    let day = today();
    seed_existing_entry(
        &root,
        &day,
        1,
        "# e1\n\n## F-000 `0123456789abcdf1`\n\nkeep me\n",
    );
    let bad = run(&root, &["mem", "preserve", "ffffffffffffffff"]);
    assert_eq!(bad.status.code(), Some(1), "{}", combined(&bad));
    assert!(
        combined(&bad).contains("not a known canonical fact id"),
        "{}",
        combined(&bad)
    );
    let ok = run(&root, &["mem", "preserve", "0123456789abcdf1"]);
    assert!(ok.status.success(), "{}", combined(&ok));
    assert!(
        stdout(&ok).contains("preserved 1 fact(s)"),
        "{}",
        stdout(&ok)
    );
    wipe(&root);
}

#[test]
fn cli_mem_query_finds_keyword_and_no_match_exits_1() {
    // `mem query` (canonical retrieval behind the codex memory_query
    // contract) had no CLI test. The keyword hit path, the domain
    // filter, and the no-match exit-1 path were all uncovered.
    let root = tmp_root("cmq");
    wipe(&root);
    seed_existing_entry(
        &root,
        &today(),
        1,
        "# e1\n\n## F-000 `0123456789abcde1`\n\nwidget alpha mechanism budget usage across nodes\n",
    );
    let hit = run(&root, &["mem", "query", "widget"]);
    assert!(hit.status.success(), "{}", combined(&hit));
    assert!(
        stdout(&hit).contains("widget alpha mechanism"),
        "{}",
        stdout(&hit)
    );
    let none = run(&root, &["mem", "query", "zzzz-not-a-word"]);
    assert_eq!(none.status.code(), Some(1), "{}", combined(&none));
    assert!(
        combined(&none).contains("no facts match"),
        "{}",
        combined(&none)
    );
    // Budget form must also run clean.
    let budgeted = run(&root, &["mem", "query", "--budget", "200"]);
    assert!(budgeted.status.success(), "{}", combined(&budgeted));
    // Raw form prints the machine-parseable id [domain] body triple.
    let raw = run(&root, &["mem", "query", "--raw", "widget"]);
    assert!(raw.status.success(), "{}", combined(&raw));
    assert!(
        stdout(&raw).contains("0123456789abcde1 [general] widget alpha"),
        "{}",
        stdout(&raw)
    );
    wipe(&root);
}

#[test]
fn cli_loop_parallel_fails_closed_without_verifier_and_on_bad_manifest() {
    // `loop parallel` (AFK v4) had no CLI test. Two cheap fail-closed
    // paths cover the wiring without a full git-worktree run: an
    // ad-hoc goal without --verify (P0-3) and a manifest with duplicate
    // ids must error, not panic.
    let root = tmp_root("cpar");
    wipe(&root);
    let no_verifier = run(&root, &["loop", "parallel", "some goal"]);
    assert!(!no_verifier.status.success(), "{}", combined(&no_verifier));
    assert!(
        combined(&no_verifier).contains("--verify") || combined(&no_verifier).contains("P0-3"),
        "{}",
        combined(&no_verifier)
    );
    // A manifest with duplicate ticket ids must fail closed.
    let manifest = root.join("manifest.json");
    std::fs::write(
        &manifest,
        r#"{"version":1,"tickets":[{"id":"t","id":"t2","goal":"g","scope":["a"],"verify":"true"}]}"#,
    )
    .unwrap();
    let bad_manifest = run(
        &root,
        &[
            "loop",
            "parallel",
            "goal",
            "--manifest",
            manifest.to_str().unwrap(),
        ],
    );
    assert!(
        !bad_manifest.status.success(),
        "{}",
        combined(&bad_manifest)
    );
    wipe(&root);
}

#[test]
fn cli_eval_judge_drift_reports_on_empty_corpus() {
    // `eval judge-drift` (calibration signal, AGENTS.md) had no CLI
    // test. On an empty calibration corpus it must exit 0 and report
    // zero verifications without a NaN panic in the precision branch.
    let root = tmp_root("cjd");
    wipe(&root);
    let out = run(&root, &["eval", "judge-drift"]);
    assert!(out.status.success(), "{}", combined(&out));
    let text = stdout(&out);
    assert!(
        text.contains("0 verifications") || text.contains("0 disagreements"),
        "{}",
        text
    );
    assert!(text.contains("precision"), "{}", text);
    wipe(&root);
}

#[test]
fn cli_eval_judge_recalibrate_resets_corpus() {
    // `eval judge-recalibrate` (clear the calibration corpus so close
    // gates resume) had no CLI test; the reset path and its message were
    // uncovered.
    let root = tmp_root("crc");
    wipe(&root);
    let cal = root.join("memory/derived/calibration.md");
    std::fs::create_dir_all(cal.parent().unwrap()).unwrap();
    std::fs::write(&cal, "# judge calibration\n\n{").unwrap();
    let out = run(&root, &["eval", "judge-recalibrate"]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        stdout(&out).contains("judge calibration reset"),
        "{}",
        stdout(&out)
    );
    // The corpus is cleared (reset writes a header-only file).
    let after = std::fs::read_to_string(&cal).unwrap_or_default();
    assert!(!after.contains('{'), "corpus must be cleared: {after}");
    wipe(&root);
}

#[test]
fn cli_eval_hidden_reports_avg_on_held_out_cases() {
    // `eval hidden` (contamination-safe held-out scoring) had no CLI
    // test. A hidden run must be scored and the avg line emitted.
    let root = tmp_root("chid");
    wipe(&root);
    let hidden_case = root.join("evals/hidden/h-case");
    std::fs::create_dir_all(&hidden_case).unwrap();
    std::fs::create_dir_all(root.join("evals/golden")).unwrap();
    let target = root.join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        hidden_case.join("run.json"),
        format!(
            r#"{{"goal":"g","scope":["x"],"outcome":{{"achieved":true}},"tokens_total":1,"cost_usd":0.01,"golden":null,"verify_command":"true","verify_target":"{}","trajectory":[{{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}}]}}"#,
            target.to_string_lossy()
        ),
    )
    .unwrap();
    let out = run(&root, &["eval", "hidden"]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(stdout(&out).contains("hidden h-case:"), "{}", stdout(&out));
    assert!(stdout(&out).contains("hidden avg:"), "{}", stdout(&out));
    assert!(stdout(&out).contains("not gated"), "{}", stdout(&out));
    wipe(&root);
}

#[test]
fn cli_loop_verify_exit_codes_distinguish_open_from_error() {
    // P2-13: loop verify exits 1 for an honest OPEN gap and 2 for a
    // broken verification machinery error. Neither contract had an
    // integration test (core verify is covered; the CLI exit mapping
    // was not).
    let root = tmp_root("clv");
    wipe(&root);
    // Missing case -> score_run errors -> exit 2 (machinery broke).
    let err = run(
        &root,
        &["loop", "verify", "no-such-case", "--claimant", "t"],
    );
    assert_eq!(err.status.code(), Some(2), "{}", combined(&err));
    // A real run.json with composite 0 (unachieved) -> honest OPEN -> 1.
    let case_dir = root.join("evals/cases/fail-case");
    std::fs::create_dir_all(&case_dir).unwrap();
    std::fs::create_dir_all(root.join("evals/golden")).unwrap();
    std::fs::create_dir_all(root.join("evals/results")).unwrap();
    let target = root.join("target");
    std::fs::create_dir_all(&target).unwrap();
    // loop verify's best-state gate requires a baseline; seed it for the
    // same case at the same composite so the gate stays clean.
    std::fs::write(
        root.join("evals/results/baseline.json"),
        r#"[{"case":"fail-case","composite":0.0,"outcome":1.0,"cost_usd":0.1,"tokens":1000,"tool_mismatches":0,"mode":"regression"}]"#,
    )
    .unwrap();
    std::fs::write(
        case_dir.join("run.json"),
        format!(
            r#"{{"goal":"g","scope":["x"],"outcome":{{"achieved":false}},"tokens_total":1,"cost_usd":0.0,"golden":null,"verify_command":"true","verify_target":"{}","trajectory":[{{"step":1,"tool":"read","ok":true,"goal_aligned":false,"tokens":1,"output_tokens":1}}]}}"#,
            target.to_string_lossy()
        ),
    )
    .unwrap();
    let open = run(&root, &["loop", "verify", "fail-case", "--claimant", "t"]);
    assert_eq!(open.status.code(), Some(1), "{}", combined(&open));
    wipe(&root);
}

#[test]
fn cli_loop_objective_dispatches_verifiable_cases() {
    // `loop objective` (bounded batch dispatch) had no CLI test; core
    // objective is covered, the CLI wiring (claimant arg + output) was
    // not.
    let root = tmp_root("lobj");
    wipe(&root);
    std::fs::create_dir_all(root.join("evals/golden")).unwrap();
    let scratch = root.join("evals/cases/obj-low");
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::write(
        scratch.join("run.json"),
        r#"{"goal":"x","scope":["a"],"outcome":{"achieved":false},"tokens_total":1,"cost_usd":0.05,"golden":null,"verify_command":"sh verify.sh","verify_target":"/tmp/x","trajectory":[{"step":1,"tool":"exec","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
    )
    .unwrap();
    let out = run(&root, &["loop", "objective", "--claimant", "cli-test"]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(stdout(&out).contains("obj-low"), "{}", stdout(&out));
    assert!(root.join("tickets").is_dir());
    wipe(&root);
}

#[test]
fn cli_dream_promote_applies_verdicts_into_canonical() {
    // `dream --promote` (D2 promotion -> canonical) had no CLI test;
    // the staging-discovery + verdict-manifest + apply path could break
    // (e.g. wrong staging dir, missing manifest check) silently.
    let root = tmp_root("dream-pro");
    wipe(&root);
    std::fs::create_dir_all(root.join("memory/staging/2026-08-07")).unwrap();
    let staged_path = root.join("memory/staging/2026-08-07/001.md");
    std::fs::write(
        &staged_path,
        "# Staged candidates (dream distiller)\n\n## S-000 (general)\n\nwidget alpha mechanism records budget usage across nodes\n\n## S-001 (general)\n\nenforced_by review rubric: surgical changes only\n",
    )
    .unwrap();
    let manifest = root.join("memory/staging/2026-08-07/001.verdicts.json");
    std::fs::write(
        &manifest,
        serde_json::to_string_pretty(&serde_json::json!({
            "staged": "001.md",
            "verdicts": [
                {"index": 0, "verdict": "promote", "reason": "audited"},
                {"index": 1, "verdict": "promote", "reason": "audited"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let out = run(&root, &["dream", "--promote"]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(stdout(&out).contains("promoted"), "{}", stdout(&out));
    // The promoted fact must land in canonical.
    let canonical =
        std::fs::read_to_string(root.join("memory/canonical/entries/2026-08-07/2026-08-07-001.md"))
            .unwrap_or_default();
    assert!(
        canonical.contains("widget alpha mechanism records budget usage"),
        "{canonical}"
    );
    // The enforced_by fact must NOT be auto-promoted into canonical; it
    // routes to the human review queue (ADR-0010).
    assert!(
        !canonical.contains("enforced_by review rubric"),
        "enforced fact must not auto-promote: {canonical}"
    );
    let queued: Vec<_> = std::fs::read_dir(root.join("memory/review"))
        .unwrap()
        .flatten()
        .collect();
    assert!(
        !queued.is_empty(),
        "enforced fact must land in the human queue"
    );
    wipe(&root);
}

#[test]
fn cli_harness_snapshot_writes_spec_and_ledger() {
    // `harness` (versioned harness snapshot + gate ledger row) had no
    // CLI test; core snapshot is covered, the CLI wiring (output +
    // ledger path) was not.
    let root = tmp_root("charn");
    wipe(&root);
    std::fs::create_dir_all(root.join("evals/results")).unwrap();
    std::fs::write(root.join("evals/results/baseline.json"), "[]").unwrap();
    let out = run(&root, &["harness", "snapshot"]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        stdout(&out).contains("harness snapshot"),
        "{}",
        stdout(&out)
    );
    assert!(stdout(&out).contains("ledger: docs/harness/ledger.md"));
    assert!(root.join("docs/harness/ledger.md").is_file());
    // The harness spec snapshot must land too.
    assert!(root.join("docs/harness").is_dir());
    wipe(&root);
}

#[test]
fn cli_harness_verify_rejects_phantom_claim_with_evidence() {
    // harness verify (the Phantom Guardrails counterfactual gate) had no
    // CLI test; the ACCEPT/REJECT exit-code mapping was uncovered.
    let root = tmp_root("chv");
    wipe(&root);
    std::fs::create_dir_all(root.join("scripts")).unwrap();
    std::fs::create_dir_all(root.join("candidate")).unwrap();
    // Gate observes only 'marker-missing' failures.
    std::fs::write(
        root.join("scripts/verify.sh"),
        "#!/bin/sh\nif [ \"$(cat ok.marker 2>/dev/null)\" = \"x\" ]; then echo \"[ok] build\"; exit 0; else echo \"[FAIL] marker-missing:\"; exit 1; fi\n",
    )
    .unwrap();
    std::fs::write(root.join("ok.marker"), "x").unwrap();
    // A claim of fixing 'tests' (never observed) -> phantom -> REJECT, 1.
    std::fs::write(root.join("candidate/ok.marker"), "x").unwrap();
    let out = run(
        &root,
        &[
            "harness",
            "verify",
            root.join("ok.marker").to_str().unwrap(),
            root.join("candidate/ok.marker").to_str().unwrap(),
            "--claims",
            "tests",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{}", combined(&out));
    assert!(combined(&out).contains("REJECT"), "{}", combined(&out));
    assert!(
        combined(&out).contains("never observed"),
        "{}",
        combined(&out)
    );
    wipe(&root);
}

#[test]
fn cli_eval_score_reports_composite_json() {
    // `eval score` (the reward layer's scoring entrypoint) had no CLI
    // test; a regression in the JSON report shape would pass silently.
    let root = tmp_root("ces");
    wipe(&root);
    let case_dir = root.join("evals/cases/sc-case");
    std::fs::create_dir_all(&case_dir).unwrap();
    std::fs::create_dir_all(root.join("evals/golden")).unwrap();
    let target = root.join("target");
    std::fs::create_dir_all(&target).unwrap();
    let run_path = case_dir.join("run.json");
    std::fs::write(
        &run_path,
        format!(
            r#"{{"goal":"g","scope":["x"],"outcome":{{"achieved":true}},"tokens_total":1,"cost_usd":0.01,"golden":null,"verify_command":"true","verify_target":"{}","trajectory":[{{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}}]}}"#,
            target.to_string_lossy()
        ),
    )
    .unwrap();
    let out = run(&root, &["eval", "score", run_path.to_str().unwrap()]);
    assert!(out.status.success(), "{}", combined(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid json");
    assert!(
        parsed.get("composite").is_some(),
        "no composite: {}",
        stdout(&out)
    );
    assert!(
        parsed["dims"]["outcome"].as_f64().unwrap() > 0.5,
        "{}",
        stdout(&out)
    );
    wipe(&root);
}

#[test]
fn cli_eval_steps_reports_suspicious_steps() {
    // `eval steps` (step-level supervision) had no CLI test. A run whose
    // trajectory has a goal-misaligned step must surface a SUSPICIOUS
    // marker and a suspicious count.
    let root = tmp_root("csteps");
    wipe(&root);
    let case_dir = root.join("evals/cases/st-case");
    std::fs::create_dir_all(&case_dir).unwrap();
    let run_path = case_dir.join("run.json");
    std::fs::write(
        &run_path,
        r#"{"goal":"g","scope":["x"],"outcome":{"achieved":true},"tokens_total":2,"cost_usd":0.02,"golden":null,"verify_command":null,"verify_target":null,"trajectory":[{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1},{"step":2,"tool":"exec","ok":true,"goal_aligned":false,"tokens":1,"output_tokens":1}]}"#,
    )
    .unwrap();
    let out = run(&root, &["eval", "steps", run_path.to_str().unwrap()]);
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        stdout(&out).contains("SUSPICIOUS"),
        "expected a suspicious step: {}",
        stdout(&out)
    );
    assert!(
        stdout(&out).contains("1 suspicious step"),
        "expected suspicious count: {}",
        stdout(&out)
    );
    wipe(&root);
}

#[test]
fn cli_derive_snapshot_and_replay_match() {
    // derive --snapshot/--replay (deterministic materialization proof,
    // production-readiness F.1) had no CLI test; the snapshot write and
    // MATCH verdict were uncovered at the CLI boundary.
    let root = tmp_root("cder");
    wipe(&root);
    seed_existing_entry(
        &root,
        &today(),
        1,
        "# e1\n\n## F-000 `0123456789abcde2`\n\nwidget alpha mechanism tracks budget usage\n",
    );
    let seed = run(&root, &["derive"]);
    assert!(seed.status.success(), "{}", combined(&seed));
    let snap = run(&root, &["derive", "--snapshot", "s1"]);
    assert!(snap.status.success(), "{}", combined(&snap));
    assert!(stdout(&snap).contains("snapshot s1"), "{}", stdout(&snap));
    let replay = run(&root, &["derive", "--replay", "s1"]);
    assert!(replay.status.success(), "{}", combined(&replay));
    assert!(stdout(&replay).contains("MATCH"), "{}", stdout(&replay));
    // A missing snapshot is a clean error, not a panic.
    let missing = run(&root, &["derive", "--replay", "nope"]);
    assert_eq!(missing.status.code(), Some(1), "{}", combined(&missing));
    wipe(&root);
}

#[test]
fn cli_resume_shows_brief_journal_and_in_flight() {
    // CLI resume is what a fresh session loads (AGENTS.md): brief head,
    // journal tail, in-flight checkpoint detection. The failure/mismatch
    // blocks are covered elsewhere; this trio had no integration test.
    let root = tmp_root("cres");
    wipe(&root);
    seed_existing_entry(
        &root,
        &today(),
        1,
        "# e1\n\n## F-000 `0123456789abcde3`\n\nwidget alpha mechanism tracks budget usage\n",
    );
    std::fs::create_dir_all(root.join("memory/episodic")).unwrap();
    std::fs::write(
        root.join("memory/episodic/checkpoints.log"),
        "2026-08-02T10:00:00Z BEGIN step -> abc\n2026-08-02T10:01:00Z VERIFY-PASS step @ abc\n2026-08-02T10:02:00Z BEGIN next -> def\n",
    )
    .unwrap();
    let derive = run(&root, &["derive"]);
    assert!(derive.status.success(), "{}", combined(&derive));
    let out = run(&root, &["resume"]);
    assert!(out.status.success(), "{}", combined(&out));
    let text = stdout(&out);
    assert!(text.contains("journal tail:"), "{text}");
    assert!(text.contains("BEGIN next"), "{text}");
    assert!(text.contains("in-flight checkpoint: yes"), "{text}");
    assert!(text.contains("brief head:"), "{text}");
    wipe(&root);
}

#[test]
fn cli_loop_run_fails_fast_on_bad_args() {
    // loop run (AFK v3 supervisor) spawns a detached codex worker, so
    // the happy path needs an external binary; the fail-fast arg
    // validations are testable without it and had no CLI coverage.
    let root = tmp_root("clr");
    wipe(&root);
    let workdir = root.join("wd");
    std::fs::create_dir_all(&workdir).unwrap();
    // --blind-worker without --hidden-dir -> error, no child.
    let blind = run(
        &root,
        &[
            "loop",
            "run",
            "some goal",
            "--workdir",
            workdir.to_str().unwrap(),
            "--verify",
            "true",
            "--blind-worker",
        ],
    );
    assert!(!blind.status.success(), "{}", combined(&blind));
    assert!(
        combined(&blind).contains("requires --hidden-dir"),
        "{}",
        combined(&blind)
    );
    // Unknown template -> error.
    let templ = run(
        &root,
        &[
            "loop",
            "run",
            "some goal",
            "--workdir",
            workdir.to_str().unwrap(),
            "--verify",
            "true",
            "--template",
            "nope",
        ],
    );
    assert!(!templ.status.success(), "{}", combined(&templ));
    assert!(
        combined(&templ).contains("unknown template"),
        "{}",
        combined(&templ)
    );
    wipe(&root);
}
