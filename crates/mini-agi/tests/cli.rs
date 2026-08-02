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
    assert!(
        !root.join("memory").exists(),
        "dry-run must not create canonical directories"
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
    stdin
        .write_all(&frame(serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}
        })))
        .unwrap();
    stdin
        .write_all(&frame(serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}
        })))
        .unwrap();
    stdin
        .write_all(&frame(serde_json::json!({
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
