//! Runtime configuration: `.miniagi.json` + `MINIAGI_*` env overlay.
//!
//! Hardening audit P0-2: thresholds that were hardcoded constants are
//! now tunable through one file. Behavior-preserving defaults equal the
//! historical constants, so an absent file changes nothing. JSON is used
//! (not TOML) because `serde_json` is already a pinned dependency and the
//! kernel stays std-only; the config shape is identical to the repo's
//! other data files (run.json, baseline.json).

use std::path::Path;

/// Runtime configuration. Per-field serde defaults keep absent fields at
/// the historical constant values.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Config {
    /// Loop gap-closing target (default `loopcmd::TARGET_COMPOSITE`).
    #[serde(default = "default_target_composite")]
    pub target_composite: f64,
    /// Regression-gate composite tolerance (default `eval::DEFAULT_TOLERANCE`).
    #[serde(default = "default_regression_tolerance")]
    pub regression_tolerance: f64,
    /// Worker hard caps (`None` = unlimited). Enforced in `worker.rs`
    /// around the codex/hitl worker (P0-1).
    #[serde(default)]
    pub max_steps: Option<usize>,
    /// Maximum accepted run cost in USD (`None` = unlimited).
    #[serde(default)]
    pub max_cost_usd: Option<f64>,
    /// Maximum accepted run tokens (hard loop gate, production-readiness
    /// E; `None` = unlimited).
    #[serde(default)]
    pub max_tokens: Option<u64>,
    /// Maximum accepted run wall-clock time in seconds (`None` = unlimited).
    #[serde(default)]
    pub max_wall_seconds: Option<u64>,
    /// AFK supervisor idle timeout (AFK-SUPERVISOR S1): when the
    /// worker's output file has not changed for this many seconds, the
    /// worker is killed as STUCK (`None` = idle detection disabled).
    #[serde(default)]
    pub max_idle_seconds: Option<u64>,
    /// Repetition watchdog: abort after this many identical consecutive
    /// actions in a captured trajectory (P1-5). `None` = disabled.
    #[serde(default)]
    pub max_repeated_steps: Option<usize>,
    /// Loop retry bound (cycle-33 finding, CRC #69 + GGC #60 + SQLQE
    /// bounded repair): stop re-dispatching a case after this many rerun
    /// attempts when its best result is still below the target — further
    /// retries would only burn budget on a case whose base risk exceeds
    /// the target (abstention / escalate-to-human). `None` = no bound
    /// (retry-forever, the pre-existing behavior).
    #[serde(default)]
    pub max_rerun_attempts: Option<usize>,
    /// Machine thresholds for `health` (hardening audit P0-2
    /// extension): per-field override of the hardcoded consts.
    #[serde(default)]
    pub health: crate::health::HealthThresholds,
    /// Judge-drift recalibration trigger (production-readiness C.3):
    /// when `eval judge-drift` precision drops below this, the audit
    /// signals a recalibration. Default 1.0 (any disagreement is a
    /// signal).
    #[serde(default = "default_min_judge_precision")]
    pub min_judge_precision: f64,
    /// HITL approval gate (production-readiness D.4): when true, a
    /// worker run requires an explicit `--approve <reason>`; the
    /// decision is logged to the action log.
    #[serde(default)]
    pub require_approval: bool,
}

const fn default_min_judge_precision() -> f64 {
    1.0
}

const fn default_target_composite() -> f64 {
    crate::loopcmd::TARGET_COMPOSITE
}

const fn default_regression_tolerance() -> f64 {
    crate::eval::DEFAULT_TOLERANCE
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_composite: default_target_composite(),
            regression_tolerance: default_regression_tolerance(),
            max_steps: None,
            max_cost_usd: None,
            max_tokens: None,
            max_wall_seconds: None,
            max_idle_seconds: None,
            max_repeated_steps: None,
            max_rerun_attempts: None,
            health: crate::health::HealthThresholds::default(),
            min_judge_precision: default_min_judge_precision(),
            require_approval: false,
        }
    }
}

impl Config {
    /// Load from `<root>/.miniagi.json` (if present and parseable),
    /// overlaid by `MINIAGI_*` env vars (which win over the file).
    #[must_use]
    pub fn load(root: &Path) -> Self {
        let mut cfg = Self::default();
        let path = root.join(".miniagi.json");
        if let Ok(text) = std::fs::read_to_string(&path) {
            match serde_json::from_str::<Self>(&text) {
                Ok(parsed) => cfg = parsed,
                Err(e) => {
                    // Fail-open but LOUD: a malformed config silently
                    // falling back to defaults hid misconfigurations.
                    eprintln!(
                        "warning: {} is invalid JSON — using defaults ({e})",
                        path.display()
                    );
                }
            }
        }
        cfg.apply_env();
        cfg
    }

    fn apply_env(&mut self) {
        self.apply_env_overlay(|name| std::env::var(name).ok());
    }

    /// Environment overlay behind a lookup closure (testable without
    /// mutating process env — `unsafe_code = "forbid"`).
    fn apply_env_overlay(&mut self, get: impl Fn(&str) -> Option<String>) {
        let set_f64 = |name: &str, slot: &mut f64| {
            if let Some(v) = get(name).and_then(|s| s.parse::<f64>().ok()) {
                *slot = v;
            }
        };
        set_f64("MINIAGI_TARGET_COMPOSITE", &mut self.target_composite);
        set_f64(
            "MINIAGI_REGRESSION_TOLERANCE",
            &mut self.regression_tolerance,
        );
        let set_usize = |name: &str, slot: &mut Option<usize>| {
            if let Some(v) = get(name).and_then(|s| s.parse::<usize>().ok()) {
                *slot = Some(v);
            }
        };
        set_usize("MINIAGI_MAX_STEPS", &mut self.max_steps);
        set_usize("MINIAGI_MAX_REPEATED_STEPS", &mut self.max_repeated_steps);
        set_usize("MINIAGI_MAX_RERUN_ATTEMPTS", &mut self.max_rerun_attempts);
        if let Some(v) = get("MINIAGI_MAX_TOKENS").and_then(|s| s.parse::<u64>().ok()) {
            self.max_tokens = Some(v);
        }
        if let Some(v) = get("MINIAGI_MAX_COST_USD").and_then(|s| s.parse::<f64>().ok()) {
            self.max_cost_usd = Some(v);
        }
        if let Some(v) = get("MINIAGI_MAX_WALL_SECONDS").and_then(|s| s.parse::<u64>().ok()) {
            self.max_wall_seconds = Some(v);
        }
        if let Some(v) = get("MINIAGI_MAX_IDLE_SECONDS").and_then(|s| s.parse::<u64>().ok()) {
            self.max_idle_seconds = Some(v);
        }
    }

    /// Loop gap-closing target for a repo (config-aware).
    #[must_use]
    pub fn target_composite_for(root: &Path) -> f64 {
        Self::load(root).target_composite
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn tmp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mag-config-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn assert_approx_eq(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn file_parses_max_idle_seconds() {
        let root = tmp_root("idle-file");
        std::fs::write(root.join(".miniagi.json"), r#"{"max_idle_seconds": 42}"#).unwrap();
        let cfg = Config::load(&root);
        assert_eq!(cfg.max_idle_seconds, Some(42));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn env_overlays_max_idle_seconds() {
        let root = tmp_root("idle-env");
        std::fs::write(root.join(".miniagi.json"), r#"{"max_idle_seconds": 42}"#).unwrap();
        let mut cfg = Config::load(&root);
        cfg.apply_env_overlay(|name| {
            if name == "MINIAGI_MAX_IDLE_SECONDS" {
                Some("7".to_string())
            } else {
                None
            }
        });
        assert_eq!(
            cfg.max_idle_seconds,
            Some(7),
            "the env var must win over the file (S3 config contract)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn absent_file_yields_historical_defaults() {
        let root = tmp_root("defaults");
        let cfg = Config::load(&root);
        assert_approx_eq(cfg.target_composite, 0.5);
        assert_approx_eq(cfg.regression_tolerance, 0.05);
        assert_eq!(cfg.max_steps, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_file_falls_back_but_warns() {
        // Fail-open but loud: a malformed .miniagi.json must not crash —
        // defaults win (asserted), and a warning is emitted on stderr.
        let root = tmp_root("malformed");
        std::fs::write(root.join(".miniagi.json"), "{bad json").unwrap();
        let cfg = Config::load(&root);
        assert_approx_eq(cfg.target_composite, 0.5);
        assert_approx_eq(cfg.regression_tolerance, 0.05);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn file_overrides_target_and_tolerance() {
        let root = tmp_root("file");
        let mut f = std::fs::File::create(root.join(".miniagi.json")).unwrap();
        f.write_all(br#"{"target_composite": 0.6, "regression_tolerance": 0.02, "max_steps": 25}"#)
            .unwrap();
        let cfg = Config::load(&root);
        assert_approx_eq(cfg.target_composite, 0.6);
        assert_approx_eq(cfg.regression_tolerance, 0.02);
        assert_eq!(cfg.max_steps, Some(25));
        // Missing fields fall back to defaults.
        let _ = std::fs::remove_dir_all(&root);
    }

    // NOTE: the `MINIAGI_*` env overlay is not unit-tested because
    // `std::env::set_var` is unsafe in edition 2024 and the workspace
    // forbids unsafe (`unsafe_code = "forbid"`). The file-path merge
    // logic above is covered; the env path is a three-line passthrough.
}
