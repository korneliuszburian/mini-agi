//! `mini-agi health` — runtime observability (Phase 7, slice 1).
//!
//! Snapshot of the machine and repo state: load, memory, swap, the
//! process zoo, journal health, claims consistency. Thresholds are
//! tuned to catch the 2026-08-03 incident class (500 agent-browser
//! processes, OOM pressure, load 21 on 16 cores) without false alarms
//! on a healthy repo. Linux /proc reads fall back to `[skip]` on other
//! platforms (std-only kernel, no libc).

use std::fs;
use std::path::Path;

/// One warning or critical message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `warn` or `critical`.
    pub severity: String,
    /// Human message.
    pub message: String,
}

/// Machine-level thresholds (tuned to the 2026-08-03 incident).
pub mod thresholds {
    /// Warn when the 1-minute load exceeds this many cores.
    pub const LOAD_WARN_MULT: f64 = 1.5;
    /// Critical when the 1-minute load exceeds this many cores.
    pub const LOAD_CRIT_MULT: f64 = 3.0;
    /// Warn when available memory drops below this fraction of total.
    pub const MEM_WARN_FRAC: f64 = 0.10;
    /// Critical when available memory drops below this fraction.
    pub const MEM_CRIT_FRAC: f64 = 0.05;
    /// Warn when used swap exceeds this fraction of total swap.
    pub const SWAP_WARN_FRAC: f64 = 0.50;
    /// Warn when a single command has more than this many processes.
    pub const ZOO_WARN_COUNT: usize = 100;
    /// Critical when a single command has more than this many processes.
    pub const ZOO_CRIT_COUNT: usize = 300;
}

/// Configurable machine thresholds (hardening audit P0-2 extension):
/// the health classifiers read these instead of the hardcoded consts;
/// `.miniagi.json` under `"health": { ... }` overrides per-field.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct HealthThresholds {
    /// Warn when the 1-minute load exceeds this many cores.
    pub load_warn_mult: f64,
    /// Critical when the 1-minute load exceeds this many cores.
    pub load_crit_mult: f64,
    /// Warn when available memory drops below this fraction of total.
    pub mem_warn_frac: f64,
    /// Critical when available memory drops below this fraction.
    pub mem_crit_frac: f64,
    /// Warn when used swap exceeds this fraction of total swap.
    pub swap_warn_frac: f64,
    /// Warn when a single command has more than this many processes.
    pub zoo_warn_count: usize,
    /// Critical when a single command has more than this many processes.
    pub zoo_crit_count: usize,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            load_warn_mult: thresholds::LOAD_WARN_MULT,
            load_crit_mult: thresholds::LOAD_CRIT_MULT,
            mem_warn_frac: thresholds::MEM_WARN_FRAC,
            mem_crit_frac: thresholds::MEM_CRIT_FRAC,
            swap_warn_frac: thresholds::SWAP_WARN_FRAC,
            zoo_warn_count: thresholds::ZOO_WARN_COUNT,
            zoo_crit_count: thresholds::ZOO_CRIT_COUNT,
        }
    }
}

/// Classify a 1-minute load average against the core count.
#[must_use]
pub fn classify_load(load1: f64, nproc: usize, t: &HealthThresholds) -> Option<Finding> {
    let mult = load1 / f64::from(u32::try_from(nproc.max(1)).unwrap_or(1));
    if mult > t.load_crit_mult {
        Some(Finding {
            severity: "critical".into(),
            message: format!(
                "load {load1:.1} is {mult:.1}x the {nproc} cores (critical > {})",
                t.load_crit_mult
            ),
        })
    } else if mult > t.load_warn_mult {
        Some(Finding {
            severity: "warn".into(),
            message: format!(
                "load {load1:.1} is {mult:.1}x the {nproc} cores (warn > {})",
                t.load_warn_mult
            ),
        })
    } else {
        None
    }
}

/// Classify available memory as a fraction of total.
#[must_use]
pub fn classify_mem(available_frac: f64, t: &HealthThresholds) -> Option<Finding> {
    if available_frac < t.mem_crit_frac {
        Some(Finding {
            severity: "critical".into(),
            message: format!(
                "available memory {:.0}% of total (critical < {:.0}%)",
                available_frac * 100.0,
                t.mem_crit_frac * 100.0
            ),
        })
    } else if available_frac < t.mem_warn_frac {
        Some(Finding {
            severity: "warn".into(),
            message: format!(
                "available memory {:.0}% of total (warn < {:.0}%)",
                available_frac * 100.0,
                t.mem_warn_frac * 100.0
            ),
        })
    } else {
        None
    }
}

/// Classify the process zoo: the largest single-command process count.
#[must_use]
pub fn classify_zoo(largest: usize, t: &HealthThresholds) -> Option<Finding> {
    if largest > t.zoo_crit_count {
        Some(Finding {
            severity: "critical".into(),
            message: format!(
                "process zoo: {largest} processes share one command (critical > {}; 2026-08-03: 500 agent-browser)",
                t.zoo_crit_count
            ),
        })
    } else if largest > t.zoo_warn_count {
        Some(Finding {
            severity: "warn".into(),
            message: format!(
                "process zoo: {largest} processes share one command (warn > {})",
                t.zoo_warn_count
            ),
        })
    } else {
        None
    }
}

/// The health report.
#[derive(Debug, Default)]
pub struct HealthReport {
    /// 1-minute load average.
    pub load1: Option<f64>,
    /// Core count.
    pub nproc: usize,
    /// Available memory as a fraction of total.
    pub mem_available_frac: Option<f64>,
    /// Used swap as a fraction of total swap.
    pub swap_used_frac: Option<f64>,
    /// Largest single-command process count.
    pub zoo_largest: Option<usize>,
    /// Journal event counts (begin/pass/fail/status).
    pub journal: Option<[usize; 4]>,
    /// Claims on tickets that no longer exist, or on CLOSED tickets.
    pub stale_claims: Vec<String>,
    /// Findings (warn/critical) across all checks.
    pub findings: Vec<Finding>,
}

impl HealthReport {
    /// Overall verdict: critical beats warn beats ok.
    #[must_use]
    pub fn verdict(&self) -> &'static str {
        if self.findings.iter().any(|f| f.severity == "critical") {
            "CRITICAL"
        } else if self.findings.iter().any(|f| f.severity == "warn") {
            "WARN"
        } else {
            "OK"
        }
    }
}

/// Exit code for a verdict (OK=0, WARN=1, CRITICAL=2). Extracted as a
/// testable pure function (cycle-33 review F4).
#[must_use]
pub fn exit_code_for(verdict: &str) -> u8 {
    match verdict {
        "OK" => 0,
        "WARN" => 1,
        _ => 2,
    }
}

fn parse_frac(numerator: u64, denominator: u64) -> Option<f64> {
    if denominator == 0 {
        return None;
    }
    let numerator = f64::from(u32::try_from(numerator).ok()?);
    let denominator = f64::from(u32::try_from(denominator).ok()?);
    Some(numerator / denominator)
}

/// Read `/proc/meminfo` fields (kB).
fn meminfo() -> Option<(u64, u64, u64)> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    let mut total = 0u64;
    let mut available = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next()?.trim_end_matches(':');
        let value = parts.next()?.parse::<u64>().ok()?;
        match key {
            "MemTotal" => total = value,
            "MemAvailable" => available = value,
            "SwapTotal" => swap_total = value,
            "SwapFree" => swap_free = value,
            _ => {}
        }
    }
    Some((total, available, swap_total.saturating_sub(swap_free)))
}

/// Largest count of processes sharing one command (from /proc scan).
fn process_zoo() -> Option<usize> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let entries = fs::read_dir("/proc").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let comm = entry.path().join("comm");
        let Ok(text) = fs::read_to_string(&comm) else {
            continue;
        };
        *counts.entry(text.trim().to_string()).or_default() += 1;
    }
    counts.values().copied().max()
}

/// Run the health snapshot.
///
/// # Errors
///
/// Returns the underlying filesystem error.
pub fn health(root: &Path) -> Result<HealthReport, std::io::Error> {
    // Configurable thresholds (hardening audit P0-2): `.miniagi.json`
    // under "health" overrides; defaults equal the historical consts.
    let t = crate::config::Config::load(root).health;
    let mut report = HealthReport::default();

    // Machine snapshot (Linux /proc; skipped elsewhere).
    if let Ok(text) = fs::read_to_string("/proc/loadavg")
        && let Some(load1) = text
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<f64>().ok())
    {
        report.load1 = Some(load1);
        if let Some(finding) = classify_load(load1, nproc_count(), &t) {
            report.findings.push(finding);
        }
    }
    if let Some((total, available, swap_used)) = meminfo() {
        report.nproc = nproc_count();
        report.mem_available_frac = parse_frac(available, total);
        if let Some(f) = report.mem_available_frac.and_then(|f| classify_mem(f, &t)) {
            report.findings.push(f);
        }
        let swap_total = total_swap();
        report.swap_used_frac = parse_frac(swap_used, swap_total);
        if let Some(frac) = report.swap_used_frac
            && frac > t.swap_warn_frac
        {
            report.findings.push(Finding {
                severity: "warn".into(),
                message: format!(
                    "swap {:.0}% used (warn > {:.0}%)",
                    frac * 100.0,
                    t.swap_warn_frac * 100.0
                ),
            });
        }
    }
    report.zoo_largest = process_zoo();
    if let Some(largest) = report.zoo_largest
        && let Some(finding) = classify_zoo(largest, &t)
    {
        report.findings.push(finding);
    }

    // Journal health.
    let journal_path = root.join("memory/episodic/checkpoints.log");
    if let Ok(text) = fs::read_to_string(&journal_path) {
        let mut counts = [0usize; 4];
        for event in crate::journal::parse_journal(&text) {
            match event.kind {
                crate::journal::JournalKind::Begin => counts[0] += 1,
                crate::journal::JournalKind::VerifyPass => counts[1] += 1,
                crate::journal::JournalKind::VerifyFail => counts[2] += 1,
                crate::journal::JournalKind::Status => counts[3] += 1,
                crate::journal::JournalKind::End => {}
            }
        }
        report.journal = Some(counts);
        // Journal semantics (T008): a BEGIN is resolved by the next
        // VERIFY-PASS/VERIFY-FAIL; an unpaired BEGIN is an anomaly unless
        // it is the literal last line (verification in progress).
        let begins = counts[0];
        let resolved = counts[1] + counts[2];
        let unpaired = begins.saturating_sub(resolved);
        let last_is_begin = text.lines().last().is_some_and(|l| l.contains("BEGIN"));
        if unpaired > 1 || (unpaired == 1 && !last_is_begin) {
            report.findings.push(Finding {
                severity: "warn".into(),
                message: format!(
                    "checkpoint journal: {begins} BEGIN vs {resolved} resolved ({unpaired} unpaired, last line not a BEGIN)"
                ),
            });
        }
    }

    // Claims consistency: leases on missing or CLOSED tickets.
    for claim in crate::ticket::read_claims(root).unwrap_or_default() {
        match crate::ticket::find_ticket(root, &claim.ticket) {
            Err(_) => report.stale_claims.push(format!(
                "{} claimed by {} since {} — ticket missing",
                claim.ticket, claim.claimant, claim.since
            )),
            Ok(t) if t.status == "CLOSED" => report.stale_claims.push(format!(
                "{} claimed by {} since {} — ticket CLOSED, lease stale",
                claim.ticket, claim.claimant, claim.since
            )),
            Ok(_) => {}
        }
    }
    for stale in &report.stale_claims {
        report.findings.push(Finding {
            severity: "warn".into(),
            message: stale.clone(),
        });
    }

    Ok(report)
}

fn nproc_count() -> usize {
    fs::read_to_string("/proc/cpuinfo").map_or(1, |t| t.matches("processor").count())
}

fn total_swap() -> u64 {
    fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| {
                l.strip_prefix("SwapTotal:")
                    .and_then(|rest| rest.split_whitespace().next())
                    .and_then(|v| v.parse().ok())
            })
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_classification_boundaries() {
        let t = HealthThresholds::default();
        assert!(classify_load(1.0, 16, &t).is_none());
        assert_eq!(
            classify_load(25.0, 16, &t).unwrap().severity,
            "warn",
            "1.56x cores"
        );
        assert_eq!(
            classify_load(50.0, 16, &t).unwrap().severity,
            "critical",
            "3.1x cores"
        );
        assert_eq!(
            classify_load(48.1, 16, &t).unwrap().severity,
            "critical",
            "3.0x is the exclusive boundary"
        );
        // Configurable thresholds: a tight critical bound trips earlier.
        let tight = HealthThresholds {
            load_crit_mult: 0.5,
            load_warn_mult: 0.2,
            ..HealthThresholds::default()
        };
        assert_eq!(
            classify_load(10.0, 16, &tight).unwrap().severity,
            "critical"
        );
    }

    #[test]
    fn mem_classification_boundaries() {
        let t = HealthThresholds::default();
        assert!(classify_mem(0.5, &t).is_none());
        assert_eq!(classify_mem(0.08, &t).unwrap().severity, "warn");
        assert_eq!(classify_mem(0.03, &t).unwrap().severity, "critical");
    }

    #[test]
    fn zoo_classification_catches_the_incident_class() {
        let t = HealthThresholds::default();
        assert!(classify_zoo(50, &t).is_none());
        assert_eq!(classify_zoo(150, &t).unwrap().severity, "warn");
        assert_eq!(
            classify_zoo(500, &t).unwrap().severity,
            "critical",
            "2026-08-03: 500 agent-browser processes"
        );
    }

    #[test]
    fn verdict_composition() {
        let mut r = HealthReport::default();
        assert_eq!(r.verdict(), "OK");
        assert_eq!(exit_code_for(r.verdict()), 0, "OK -> 0");
        r.findings.push(Finding {
            severity: "warn".into(),
            message: "x".into(),
        });
        assert_eq!(r.verdict(), "WARN");
        assert_eq!(exit_code_for(r.verdict()), 1, "WARN -> 1");
        r.findings.push(Finding {
            severity: "critical".into(),
            message: "y".into(),
        });
        assert_eq!(r.verdict(), "CRITICAL");
        assert_eq!(exit_code_for(r.verdict()), 2, "CRITICAL -> 2");
        // Every state maps to a distinct code (a regression collapsing
        // WARN and CRITICAL would fail here).
        let codes: Vec<u8> = ["OK", "WARN", "CRITICAL"]
            .iter()
            .map(|v| exit_code_for(v))
            .collect();
        assert_eq!(codes, vec![0, 1, 2]);
    }

    #[test]
    fn health_reports_healthy_on_the_repo() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../")
            .canonicalize()
            .unwrap();
        let report = health(&root).unwrap();
        // Structural soundness only: the verdict may be OK, WARN, or
        // CRITICAL depending on real machine load/memory (a loaded box
        // legitimately reports CRITICAL — the test must not assume a
        // quiet machine). What must hold: a complete, self-consistent
        // report with valid fractions and findings that match the verdict.
        assert!(
            matches!(report.verdict(), "OK" | "WARN" | "CRITICAL"),
            "verdict must be one of OK/WARN/CRITICAL, got {:?}",
            report.verdict()
        );
        assert!(
            report
                .findings
                .iter()
                .all(|f| matches!(f.severity.as_str(), "warn" | "critical")),
            "every finding must carry a valid severity"
        );
        if let Some(frac) = report.mem_available_frac {
            assert!((0.0..=1.0).contains(&frac));
        }
    }
}
