//! Deterministic inventory and context-budget measurement.
//!
//! PORT of `PoC` `scripts/stats.py` and `scripts/budget.py` semantics:
//! the kernel measures itself — canonical inventory by domain (stats) and
//! the context budget report (AGENTS chain vs 32KiB cap, skills list vs
//! 2% budget, memory leverage ratio).

use std::fs;
use std::io;
use std::path::Path;

/// Per-domain canonical fact counts.
#[derive(Debug, Default)]
pub struct StatsReport {
    /// Canonical entry files under `memory/canonical/entries`.
    pub entries: usize,
    /// Facts (``## F-N `<16hex>` `` headings) across entries.
    pub facts: usize,
    /// Derived markdown files under `memory/derived`.
    pub derived_views: usize,
    /// (domain, facts) sorted by domain name.
    pub per_domain: Vec<(String, usize)>,
}

/// Context budget report.
#[derive(Debug)]
pub struct BudgetReport {
    /// AGENTS.md bytes (instruction chain).
    pub agents_chain_bytes: u64,
    /// Approximate chain tokens (`PoC` `approx_tokens`: ~4 chars/token).
    pub agents_chain_tokens: usize,
    /// Chain as percentage of the 32KiB Codex cap.
    pub chain_pct_of_32k: f64,
    /// True when the chain exceeds the 32KiB cap.
    pub chain_over_cap: bool,
    /// Number of skills in the registry.
    pub skills_count: usize,
    /// Char count of skill frontmatter blocks (what the skill list
    /// shows), counted in chars to match the 8000-char budget — byte
    /// counting would inflate non-ASCII descriptions.
    pub skills_list_bytes: usize,
    /// Skills list as percentage of the 2% context budget (8000 chars).
    pub skills_pct_of_budget: f64,
    /// True when the skills list exceeds the budget.
    pub skills_over_budget: bool,
    /// Canonical memory bytes.
    pub canonical_bytes: u64,
    /// Derived brief bytes (what actually enters agent context).
    pub brief_bytes: u64,
    /// Canonical/brief compression ratio into the working set.
    pub leverage_ratio: f64,
}

const CHAIN_CAP_BYTES: u64 = 32 * 1024;
const SKILLS_BUDGET_CHARS: usize = 8000;

/// Compute canonical-memory statistics for a repo root.
///
/// # Errors
///
/// Returns [`io::Error`] when `memory/canonical/entries` cannot be read.
pub fn stats(root: &Path) -> Result<StatsReport, io::Error> {
    let entries_dir = root.join("memory/canonical/entries");
    let mut report = StatsReport::default();
    let mut counts: Vec<(String, usize)> = Vec::new();
    if entries_dir.is_dir() {
        for entry in walk_md(&entries_dir)? {
            let text = fs::read_to_string(&entry)?;
            let facts = fact_count(&text);
            let domain = domain_for(&text);
            report.entries += 1;
            report.facts += facts;
            if let Some(slot) = counts.iter_mut().find(|(d, _)| d == &domain) {
                slot.1 += facts;
            } else {
                counts.push((domain, facts));
            }
        }
    }
    counts.sort_by(|a, b| a.0.cmp(&b.0));
    report.per_domain = counts;
    let derived = root.join("memory/derived");
    if derived.is_dir() {
        report.derived_views = walk_md(&derived)?.len();
    }
    Ok(report)
}

/// Collect all `*.md` files under `dir` (recursive, sorted).
fn walk_md(dir: &Path) -> Result<Vec<std::path::PathBuf>, io::Error> {
    let mut out = Vec::new();
    collect_md(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_md(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), io::Error> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_md(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
    Ok(())
}

/// Count `## F-N` fact headings carrying 16-hex ids (`PoC` `FACT_HEADING`).
fn fact_count(text: &str) -> usize {
    let mut count = 0;
    for line in text.lines() {
        let line = line.trim_end();
        let Some(rest) = line.strip_prefix("## ") else {
            continue;
        };
        let Some((_f, id)) = rest.split_once('`') else {
            continue;
        };
        let Some(id) = id.strip_suffix('`') else {
            continue;
        };
        if id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()) {
            count += 1;
        }
    }
    count
}

/// Read the `- domain:` line (`PoC` `domain_for`); default `general`.
fn domain_for(text: &str) -> String {
    text.lines()
        .find_map(|line| line.strip_prefix("- domain:"))
        .map_or_else(|| "general".to_string(), |rest| rest.trim().to_string())
}

/// Compute the context budget report (`PoC` `budget.py`).
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "exact port of PoC float math; byte counts are bounded by repo size"
)]
pub fn budget(root: &Path) -> BudgetReport {
    let agents_md = root.join("AGENTS.md");
    let claude_md = root.join("CLAUDE.md");
    let chain_bytes = agents_md.metadata().map_or(0, |m| m.len());
    let chain_text = read_quiet(&agents_md) + &read_quiet(&claude_md);
    let chain_tokens = approx_tokens(&chain_text);
    let pct = 100.0 * chain_bytes as f64 / CHAIN_CAP_BYTES as f64;
    let chain_over_cap = chain_bytes > CHAIN_CAP_BYTES;

    let skills_dir = root.join(".agents/skills");
    let mut skills_count = 0usize;
    let mut list_bytes = 0usize;
    if skills_dir.is_dir() {
        let mut md_files = Vec::new();
        if let Ok(entries) = fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("SKILL.md").is_file() {
                    md_files.push(path.join("SKILL.md"));
                }
            }
        }
        md_files.sort();
        for skill_md in md_files {
            if let Ok(text) = fs::read_to_string(&skill_md) {
                if let Some(block) = frontmatter_block(&text) {
                    // Budget is defined in CHARS (SKILLS_BUDGET_CHARS);
                    // count chars, not bytes — em-dashes / non-ASCII
                    // descriptions otherwise inflate the percentage.
                    list_bytes += block.chars().count();
                }
                skills_count += 1;
            }
        }
    }
    let skills_pct = 100.0 * list_bytes as f64 / SKILLS_BUDGET_CHARS as f64;
    let skills_over_budget = list_bytes > SKILLS_BUDGET_CHARS;

    let canonical_bytes = dir_md_bytes(&root.join("memory/canonical"));
    let brief_bytes = root
        .join("memory/derived/context-brief.md")
        .metadata()
        .map_or(0, |m| m.len());
    let leverage_ratio = canonical_bytes as f64 / brief_bytes.max(1) as f64;

    BudgetReport {
        agents_chain_bytes: chain_bytes,
        agents_chain_tokens: chain_tokens,
        chain_pct_of_32k: round1(pct),
        chain_over_cap,
        skills_count,
        skills_list_bytes: list_bytes,
        skills_pct_of_budget: round1(skills_pct),
        skills_over_budget,
        canonical_bytes,
        brief_bytes,
        leverage_ratio: round2(leverage_ratio),
    }
}

fn read_quiet(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn frontmatter_block(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("---")?;
    let end = rest.find("---")?;
    Some(&rest[..end])
}

fn dir_md_bytes(dir: &Path) -> u64 {
    let Ok(files) = walk_md(dir) else {
        return 0;
    };
    files
        .iter()
        .map(|p| p.metadata().map_or(0, |m| m.len()))
        .sum()
}

fn approx_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn root_with(tag: &str, entries: &[(&str, &str, &[String])]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("mag-metrics-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (name, domain, ids) in entries {
            let day = root.join("memory/canonical/entries/2026-08-02");
            let facts = ids
                .iter()
                .enumerate()
                .map(|(i, id)| format!("## F-{i} `{id}`\n\nFact {i}."))
                .collect::<Vec<_>>()
                .join("\n\n");
            let text = format!("- domain: {domain}\n\n{facts}\n");
            fs::create_dir_all(&day).unwrap();
            fs::write(day.join(name), text).unwrap();
        }
        root
    }

    #[test]
    fn stats_sorts_fact_counts_per_domain() {
        let root = root_with(
            "a",
            &[
                ("alpha.md", "zebra", &["a".repeat(16), "b".repeat(16)]),
                ("beta.md", "alpha", &["c".repeat(16)]),
            ],
        );
        let report = stats(&root).unwrap();
        assert_eq!(report.entries, 2);
        assert_eq!(report.facts, 3);
        assert_eq!(report.derived_views, 0);
        let domains: Vec<(&str, usize)> = report
            .per_domain
            .iter()
            .map(|(d, n)| (d.as_str(), *n))
            .collect();
        assert_eq!(domains, vec![("alpha", 1), ("zebra", 2)]);
    }

    #[test]
    fn stats_ignores_entries_without_fact_headings() {
        let root = root_with("b", &[("facts.md", "testing", &["d".repeat(16)])]);
        let day = root.join("memory/canonical/entries/2026-08-02");
        fs::write(
            day.join("empty.md"),
            "- domain: testing\n\nNo facts here.\n",
        )
        .unwrap();
        let report = stats(&root).unwrap();
        assert_eq!(report.entries, 2);
        assert_eq!(report.facts, 1);
        assert_eq!(report.per_domain[0], ("testing".to_string(), 1));
    }

    #[test]
    fn stats_counts_derived_views() {
        let root = root_with("c", &[("a.md", "general", &["e".repeat(16)])]);
        let derived = root.join("memory/derived");
        fs::create_dir_all(derived.join("per-domain")).unwrap();
        fs::write(derived.join("context-brief.md"), "brief").unwrap();
        fs::write(derived.join("per-domain/AGENTS.x.md"), "x").unwrap();
        let report = stats(&root).unwrap();
        assert_eq!(report.derived_views, 2);
    }

    #[test]
    fn budget_measures_chain_skills_and_leverage() {
        let root = root_with("c-budget", &[("a.md", "general", &["e".repeat(16)])]);
        fs::write(root.join("AGENTS.md"), "agents instructions here").unwrap();
        fs::write(root.join("CLAUDE.md"), "shim").unwrap();
        let skills = root.join(".agents/skills/demo");
        fs::create_dir_all(&skills).unwrap();
        fs::write(
            skills.join("SKILL.md"),
            "---\nname: demo\ndescription: demo skill\n---\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("memory/derived")).unwrap();
        fs::write(
            root.join("memory/derived/context-brief.md"),
            "brief content",
        )
        .unwrap();
        let b = budget(&root);
        assert_eq!(b.agents_chain_bytes, 24);
        assert!(!b.chain_over_cap);
        assert!(b.chain_pct_of_32k < 1.0);
        assert_eq!(b.skills_count, 1);
        assert!(b.skills_list_bytes > 0);
        assert!(!b.skills_over_budget);
        assert!(b.canonical_bytes > 0);
        assert!(b.brief_bytes > 0);
        // Leverage = canonical / brief. Sanity bound: the working-set
        // brief must not be an order of magnitude larger than the source
        // canonical (the pre-iter-22 fact-linking bug made it 5x larger
        // = leverage 0.19). A tiny fixture legitimately sits near 2-3;
        // the bound catches pathological expansion (>= 5x).
        assert!(
            b.leverage_ratio > 0.0 && b.leverage_ratio <= 5.0,
            "leverage {} out of sane (0,5] — brief {} vs canonical {}",
            b.leverage_ratio,
            b.brief_bytes,
            b.canonical_bytes
        );
    }

    #[test]
    fn budget_flags_chain_over_cap() {
        let root = std::env::temp_dir().join(format!("mag-metrics-cap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("AGENTS.md"),
            "x".repeat(usize::try_from(CHAIN_CAP_BYTES).unwrap_or(usize::MAX) + 10),
        )
        .unwrap();
        let b = budget(&root);
        assert!(b.chain_over_cap);
    }

    #[test]
    fn approx_tokens_scales_with_length_and_never_zeroes() {
        // Token estimate feeds the budget report's chain_tokens field;
        // it had no direct test. Rough heuristic: bytes / 4, floor 1.
        assert_eq!(approx_tokens(""), 1, "empty must not read as 0 tokens");
        assert_eq!(approx_tokens("abcdefgh"), 2, "8 ASCII bytes -> 2 tokens");
        assert_eq!(approx_tokens("a"), 1, "short input floors at 1");
        // Non-ASCII counts bytes (over-estimates tokens); still >= 1.
        let emdash = approx_tokens("—");
        assert!(emdash >= 1, "non-ASCII must still yield >= 1");
    }

    #[test]
    fn frontmatter_block_extracts_between_dashes_and_handles_missing() {
        // frontmatter_block parses skill metadata for the skills budget
        // count; a parse regression would silently mis-count the list.
        let text = "---\nname: demo\ndescription: d\n---\n# body\n";
        let block = frontmatter_block(text).unwrap();
        assert!(block.contains("name: demo"), "{block}");
        assert!(block.contains("description: d"), "{block}");
        assert!(
            !block.contains("body"),
            "block must stop at the closing ---"
        );
        // No opening fence -> None (no skills-list contribution).
        assert!(frontmatter_block("no frontmatter").is_none());
        // Unterminated fence -> None.
        assert!(frontmatter_block("---\nname: demo\n").is_none());
    }

    #[test]
    fn skills_over_budget_is_flagged() {
        // The 2% skills budget cap: a single over-long frontmatter block
        // must set skills_over_budget (only the negative path was tested).
        let root = std::env::temp_dir().join(format!("mag-metrics-sk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let skills = root.join(".agents/skills/big");
        fs::create_dir_all(&skills).unwrap();
        fs::write(
            skills.join("SKILL.md"),
            format!(
                "---\nname: big\ndescription: {}\n---\n",
                "x".repeat(SKILLS_BUDGET_CHARS + 100)
            ),
        )
        .unwrap();
        let b = budget(&root);
        assert!(b.skills_over_budget, "over-long skill must flag the budget");
        let _ = fs::remove_dir_all(&root);
    }
}
