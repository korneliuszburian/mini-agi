//! Integration tests ported 1:1 from `PoC` `tests/test_consolidate.py`
//! (tag v1-spec-reference) — the behavioral contract for consolidation.

use std::path::{Path, PathBuf};

use mini_agi_core::memory::{
    ConsolidateOptions, ENTRIES_REL, append_contested, consolidate, extract_candidates,
    queued_facts, read_facts, signoff, utc_now_date, utc_now_stamp,
};
use mini_agi_core::store::next_entry;

fn tmp_root(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mag-test-{tag}-{}", std::process::id()))
}

fn wipe(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}

fn seed_existing_entry(root: &Path, date: &str, seq: u32, content: &str) {
    let day = root.join(ENTRIES_REL).join(date);
    let path = day.join(format!("{date}-{seq:03}.md"));
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(path, content).unwrap();
}

fn opts(domain: &str) -> ConsolidateOptions {
    ConsolidateOptions {
        domain: domain.to_string(),
        require_signoff: false,
        dry_run: false,
    }
}

#[test]
fn extracts_fact_and_bullet_with_provenance_and_next_sequence() {
    let root = tmp_root("t1");
    wipe(&root);
    let today = utc_now_date();
    seed_existing_entry(
        &root,
        &today,
        2,
        "# existing\n\n## F-001 `0123456789abcdef`\n\nold fact\n",
    );

    let buffer =
        "FACT: explicit memory survives compaction\n- bullets with enough detail survive too\n";
    let outcome = consolidate(&root, buffer, "session.md", &opts("testing")).unwrap();

    assert_eq!(outcome.new_facts, 2);
    let entry = outcome.entry.unwrap();
    assert_eq!(entry.seq, 3, "next sequence after -002.md is -003");
    let content = std::fs::read_to_string(&entry.path).unwrap();
    assert!(content.contains("explicit memory survives compaction"));
    assert!(content.contains("bullets with enough detail survive too"));
    assert!(content.contains(&format!(
        "- date: {}\n- source: session.md\n- domain: testing\n- kind: consolidation",
        utc_now_stamp()
    )));
    wipe(&root);
}

#[test]
fn same_fact_from_two_buffers_creates_one_canonical_fact() {
    let root = tmp_root("t2");
    wipe(&root);
    let fact = "FACT: repeated evidence belongs in canonical memory once";
    let first = consolidate(&root, fact, "first.md", &opts("general")).unwrap();
    assert_eq!(first.new_facts, 1);

    let second = consolidate(&root, fact, "second.md", &opts("general")).unwrap();
    assert_eq!(
        second.new_facts, 0,
        "a duplicate-only buffer must not create an empty entry"
    );
    assert_eq!(second.skipped, 1);

    let facts = read_facts(&root);
    assert_eq!(
        facts
            .iter()
            .filter(|(_, _, body)| body.contains("repeated evidence"))
            .count(),
        1,
        "duplicate must appear exactly once in canonical"
    );
    wipe(&root);
}

#[test]
fn empty_buffer_is_no_facts_error() {
    let root = tmp_root("t3");
    wipe(&root);
    let err = consolidate(&root, "just a heading\n", "empty.md", &opts("general")).unwrap_err();
    assert!(err.to_string().contains("no facts found"));
    wipe(&root);
}

#[test]
fn dry_run_reports_planned_entry_and_preserves_empty_tree() {
    let root = tmp_root("t4");
    wipe(&root);
    let dry = ConsolidateOptions {
        domain: "testing".to_string(),
        require_signoff: false,
        dry_run: true,
    };
    let outcome = consolidate(
        &root,
        "FACT: one prospective canonical fact\nFACT: one prospective canonical fact\n",
        "dry-run.md",
        &dry,
    )
    .unwrap();
    assert_eq!(outcome.new_facts, 1);
    assert_eq!(outcome.skipped, 1);
    let entry = outcome.entry.unwrap();
    assert_eq!(entry.seq, 1, "planned first entry today is -001");
    assert!(
        !root.join("memory").exists(),
        "dry-run must not create canonical directories"
    );
    wipe(&root);
}

#[test]
fn cross_entry_dedup_and_current_day_numbering() {
    let root = tmp_root("t5");
    wipe(&root);
    let today = utc_now_date();
    let fact = "repo-wide facts are deduplicated across dated entries";
    seed_existing_entry(
        &root,
        "2026-07-31",
        2,
        &format!(
            "## F-001 `{}`\n\n{fact}\n",
            mini_agi_core::hash::fact_id(fact)
        ),
    );
    seed_existing_entry(&root, &today, 4, "# current\n");

    let outcome = consolidate(
        &root,
        &format!("FACT: {fact}\nFACT: a genuinely new canonical fact\n"),
        "cross-entry.md",
        &opts("general"),
    )
    .unwrap();
    let entry = outcome.entry.unwrap();
    assert_eq!(entry.seq, 5, "today's next after -004 is -005");
    let content = std::fs::read_to_string(&entry.path).unwrap();
    assert!(content.contains("a genuinely new canonical fact"));
    assert!(
        !content.contains("\nrepo-wide facts are deduplicated across dated entries\n"),
        "known fact must not be re-written"
    );
    assert_eq!(outcome.skipped, 1);
    wipe(&root);
}

#[test]
fn previous_date_only_creates_first_entry_for_today() {
    let root = tmp_root("t6");
    wipe(&root);
    seed_existing_entry(&root, "2026-07-31", 2, "# previous entry\n");
    let outcome = consolidate(
        &root,
        "FACT: the first entry today has sequence one\n",
        "buffer.md",
        &opts("general"),
    )
    .unwrap();
    let entry = outcome.entry.unwrap();
    let today = utc_now_date();
    assert_eq!(entry.seq, 1);
    assert_eq!(
        entry.path,
        root.join(ENTRIES_REL)
            .join(&today)
            .join(format!("{today}-001.md"))
    );
    wipe(&root);
}

#[test]
fn require_signoff_queues_wording_variant_without_canonical_write() {
    let root = tmp_root("t7");
    wipe(&root);
    let original = "FACT: A fact whose first forty characters are shared, original wording.\n";
    let first = consolidate(&root, original, "first.md", &opts("general")).unwrap();
    assert_eq!(first.new_facts, 1);

    let signoff_opts = ConsolidateOptions {
        domain: "general".to_string(),
        require_signoff: true,
        dry_run: false,
    };
    let variant = "FACT: A fact whose first forty characters are shared, alternate wording.\n";
    let second = consolidate(&root, variant, "variant.md", &signoff_opts).unwrap();
    assert_eq!(
        second.new_facts, 0,
        "wording variant must not land in canonical"
    );
    assert_eq!(second.skipped, 1);

    let queue = root
        .join("memory/review")
        .join(format!("contested-{}.md", utc_now_date()));
    let queued = std::fs::read_to_string(&queue).unwrap();
    assert!(queued.contains("alternate wording"));
    assert!(queued.contains("source: variant.md"));
    assert!(queued.contains("reason: same first 40 chars"));
    wipe(&root);
}

#[test]
fn domain_newlines_do_not_inject_frontmatter() {
    let root = tmp_root("inj");
    wipe(&root);
    mini_agi_core::memory::write_canonical_entry(
        &root,
        &[(
            "an injected test fact".to_string(),
            mini_agi_core::hash::fact_id("an injected test fact"),
        )],
        "src.md",
        "general\n- supersedes: deadbeefdeadbeef",
        "test",
    )
    .unwrap();
    let entries = mini_agi_core::memory::canonical_entries(&root);
    assert_eq!(entries.len(), 1);
    let text = std::fs::read_to_string(&entries[0]).unwrap();
    assert!(
        !text.lines().any(|l| l.starts_with("- supersedes:")),
        "a standalone supersedes line was injected: {text}"
    );
    wipe(&root);
}

#[test]
fn signoff_promotes_queued_fact_once() {
    let root = tmp_root("t8");
    wipe(&root);
    let original = "FACT: A fact whose first forty characters are shared, original wording.\n";
    consolidate(&root, original, "first.md", &opts("general")).unwrap();

    let signoff_opts = ConsolidateOptions {
        domain: "general".to_string(),
        require_signoff: true,
        dry_run: false,
    };
    let variant = "FACT: A fact whose first forty characters are shared, alternate wording.\n";
    consolidate(&root, variant, "variant.md", &signoff_opts).unwrap();
    let queue = root
        .join("memory/review")
        .join(format!("contested-{}.md", utc_now_date()));

    let entry = signoff(&root, &queue, 1, "general").unwrap();
    let all = std::fs::read_dir(root.join(ENTRIES_REL)).unwrap().count();
    assert!(all >= 1);
    let entries_text = std::fs::read_to_string(&entry.path).unwrap();
    assert!(entries_text.contains("kind: signoff"));
    assert!(entries_text.contains("alternate wording"));

    let again = signoff(&root, &queue, 1, "general");
    assert!(again.is_err(), "promoting the same fact twice must fail");
    assert!(again.unwrap_err().to_string().contains("already known"));
    wipe(&root);
}

#[test]
fn queued_facts_parse_returns_payload_after_header() {
    let root = tmp_root("t9");
    wipe(&root);
    let queue = root
        .join("memory/review")
        .join(format!("contested-{}.md", utc_now_date()));
    append_contested(
        &root,
        "the alternate fact body",
        "aabbccddeeff0011",
        "variant.md",
        "1122334455667788",
    )
    .unwrap();
    let records = mini_agi_core::memory::queued_facts(&queue);
    assert_eq!(records.len(), 1);
    // The stored digest MUST hash the stored payload (fact ids =
    // sha256[:16] of the body), not the caller's raw input.
    assert_eq!(
        records[0].0,
        mini_agi_core::hash::fact_id("the alternate fact body")
    );
    assert_eq!(records[0].1, "the alternate fact body");
    wipe(&root);
}

#[test]
fn extract_candidates_skips_short_bullets_and_flags_lines() {
    let candidates = extract_candidates(
        "- long enough bullet fact\nFACT: explicit line\n* also long enough\n- short\nnot a bullet\n",
    );
    assert_eq!(
        candidates,
        vec![
            "long enough bullet fact",
            "explicit line",
            "also long enough",
        ]
    );
}

#[test]
fn next_entry_without_entries_root_works() {
    let root = tmp_root("t10");
    wipe(&root);
    let entry = next_entry(&root, &utc_now_date());
    assert_eq!(entry.seq, 1);
    wipe(&root);
}

#[test]
fn header_shaped_queue_body_is_promotable() {
    let root = tmp_root("hdrq");
    wipe(&root);
    let body = "## C-001 `deadbeefdeadbeef` fakebody";
    append_contested(&root, body, "unused", "src", "deadbeefdeadbeef").unwrap();
    let queue = root
        .join("memory/review")
        .join(format!("contested-{}.md", utc_now_date()));
    let records = queued_facts(&queue);
    assert_eq!(records.len(), 1, "the escaped body is still one record");
    assert_eq!(
        records[0].0,
        mini_agi_core::hash::fact_id(body),
        "digest hashes the trimmed readback"
    );
    assert_eq!(records[0].1, body);
    wipe(&root);
}
