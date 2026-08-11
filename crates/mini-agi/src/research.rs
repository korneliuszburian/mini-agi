//! Auto-researcher (Phase 2): an opencode flash worker with a
//! primary-source research contract. The kernel captures the worker's
//! answer and writes it to `research/<slug>.md`; the findings then feed
//! the dream-loop (`dream --source <file>`) so research becomes memory.

use std::path::Path;

/// Slugify a research question into a safe file stem.
#[must_use]
pub fn slugify(question: &str) -> String {
    let mut out = String::new();
    for c in question.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if (c.is_whitespace() || c == '-' || c == '_')
            && !out.ends_with('-')
            && !out.is_empty()
        {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "research".to_string()
    } else {
        out.chars().take(60).collect()
    }
}

/// The research prompt: primary sources, per-claim provenance, bounded
/// scope, no fabrication. The answer is a markdown document the kernel
/// writes verbatim.
#[must_use]
pub fn research_prompt(question: &str) -> String {
    format!(
        "You are an AUTO-RESEARCHER. Investigate the question below against \
         HIGH-TRUST PRIMARY SOURCES — official docs, source code, specs, \
         first-party APIs — not secondary write-ups of them. Use your tools \
         to fetch and read actual sources.\n\n\
         CONTRACT (binding):\n\
         1. Every claim carries its source: cite the document name and URL \
         or repo path beside the claim. A claim without a nearby source is \
         an opinion — label it 'opinion'.\n\
         2. Follow every claim back to the source that owns it; do not \
         paraphrase a secondary write-up as if it were primary.\n\
         3. Distinguish fact | estimate | opinion explicitly.\n\
         4. Bounded scope: answer THIS question; do not produce an essay, \
         a tutorial, or adjacent topics.\n\
         5. NO FABRICATION: if a fact is not verifiable, say 'unknown — not \
         verifiable from the sources I reached' rather than inventing it. \
         Never invent URLs, names, or numbers.\n\
         6. End with a short VERDICT section: what is established, what is \
         uncertain, and what evidence would settle it.\n\n\
         QUESTION: {question}\n\n\
         7. Do NOT narrate your process — no 'I will research', no \
         'now fetching', no progress commentary. The output is ONLY the \
         deliverable document.\n\
         8. If the sources are PDFs you cannot fully read, say so in the \
         Verdict and cite what you could verify — never invent the rest.\n\n\
         OUTPUT FORMAT: a markdown document with exactly these sections: \
         ## Findings (claims with sources), ## Sources (full list), \
         ## Verdict. The document is NOT complete without all three."
    )
}

/// The findings file path for a question.
#[must_use]
pub fn findings_path(root: &Path, question: &str) -> std::path::PathBuf {
    root.join("research")
        .join(format!("{}.md", slugify(question)))
}

/// Completeness gate: a research run is only a deliverable when the
/// answer carries all three required sections (Findings / Sources /
/// Verdict). Narration-only or truncated answers fail the gate — the
/// kernel must NOT write them as findings (observed: flash workers
/// stalled mid-investigation and returned only process narration).
#[must_use]
pub fn is_complete_deliverable(findings: &str) -> bool {
    let lower = findings.to_lowercase();
    let Some(fidx) = lower.find("## findings") else {
        return false;
    };
    // The claim guard is scoped to the Findings section: a "fact"
    // mention anywhere else (Sources, Verdict) must not satisfy it —
    // an empty Findings section with a wordy Verdict would otherwise
    // smuggle a narration-only document past the gate.
    let rest = &lower[fidx..];
    let section_len = rest.find("\n## ").unwrap_or(rest.len());
    lower.contains("## sources")
        && lower.contains("## verdict")
        && rest[..section_len].contains("fact")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_produces_safe_stems() {
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("  Spaces  and  -- dashes  "), "spaces-and-dashes");
        assert_eq!(
            slugify("What is the cost of deepseek flash?"),
            "what-is-the-cost-of-deepseek-flash"
        );
        assert_eq!(slugify("???"), "research", "empty stem falls back");
        assert!(slugify(&"a".repeat(200)).len() <= 60, "stems are bounded");
        for c in slugify("A/B\\C:D*E?F\"G<H>I|J").chars() {
            assert!(c.is_ascii_alphanumeric() || c == '-', "only safe chars");
        }
    }

    #[test]
    fn completeness_gate_rejects_narration_only_and_truncated() {
        assert!(is_complete_deliverable(
            "## Findings\n- fact: x (source: y)\n## Sources\n- y\n## Verdict\n- established"
        ));
        // Process narration only (observed in the wild): rejected.
        assert!(!is_complete_deliverable(
            "I'll research this. Now fetching primary docs. Abstracts retrieved..."
        ));
        // Missing one section: rejected.
        assert!(!is_complete_deliverable(
            "## Findings\n- fact: x\n## Sources\n- y"
        ));
        // Empty Findings: rejected.
        assert!(!is_complete_deliverable(
            "## Findings\n## Sources\n- y\n## Verdict\n- unknown"
        ));
    }

    #[test]
    fn research_prompt_carries_the_contract() {
        let p = research_prompt("test question");
        assert!(p.contains("PRIMARY SOURCES"));
        assert!(p.contains("NO FABRICATION"));
        assert!(p.contains("fact | estimate | opinion"));
        assert!(p.contains("VERDICT"));
        assert!(p.contains("test question"));
        assert!(p.contains("unknown — not verifiable"));
    }

    #[test]
    fn claim_guard_is_scoped_to_the_findings_section() {
        // "fact" mentioned ONLY in Sources/Verdict must not satisfy the
        // gate — the Findings section is empty, the doc is narration.
        assert!(!is_complete_deliverable(
            "## Findings\n\n## Sources\n- fact sheet at example.com\n## Verdict\n- fact: unknown"
        ));
        // A claim inside Findings satisfies the gate even when the other
        // sections are terse.
        assert!(is_complete_deliverable(
            "## Findings\n- fact: x, source: y\n## Sources\n- y\n## Verdict\n- est."
        ));
    }

    #[test]
    fn deliverable_gate_header_shapes() {
        // Uppercase headers are accepted (case-insensitive contract)...
        assert!(is_complete_deliverable(
            "## FINDINGS\n- fact: x\n## SOURCES\n- y\n## VERDICT\n- est."
        ));
        // ...but a single-# heading is not a section header.
        assert!(!is_complete_deliverable(
            "# Findings\n- fact: x\n# Sources\n- y\n# Verdict\n- est."
        ));
    }

    #[test]
    fn findings_path_matches_the_slugged_stem() {
        let root = std::path::Path::new("/r");
        assert_eq!(
            findings_path(root, "What is the cost of deepseek flash?"),
            std::path::PathBuf::from("/r/research/what-is-the-cost-of-deepseek-flash.md")
        );
        // A non-ASCII-only question cannot produce a file stem; the
        // fallback stem keeps the write safe and the registry consistent.
        assert_eq!(
            findings_path(root, "żółć?").file_name().unwrap(),
            "research.md"
        );
    }

    #[test]
    fn slugify_strips_trailing_separators_and_foreign_chars() {
        assert_eq!(slugify("ABC  DEF "), "abc-def");
        assert_eq!(slugify("a--"), "a", "trailing separator trimmed");
        assert_eq!(
            slugify("żółć hello"),
            "hello",
            "non-ascii dropped, no debris"
        );
        assert_eq!(slugify("x.y.z"), "xyz", "dots are dropped, not separators");
        assert_eq!(slugify("  "), "research", "blank collapses to fallback");
    }
}
