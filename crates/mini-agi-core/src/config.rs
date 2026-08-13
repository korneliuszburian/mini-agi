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
    /// HITL approval gate (production-readiness D.4): when true, a
    /// worker run requires an explicit `--approve <reason>`; the
    /// decision is logged to the action log.
    #[serde(default)]
    pub require_approval: bool,
    /// Allow `loop verify` gates to run in a target OUTSIDE the repo
    /// root (ARCHITECTURE-CONDENSED 5.1). Default `false`: a declared
    /// `verify_target` that escapes the root after canonicalization is
    /// rejected. Outside targets are opt-in and explicit, never implicit.
    #[serde(default)]
    pub allow_outside_targets: bool,
}

const fn default_target_composite() -> f64 {
    crate::loopcmd::TARGET_COMPOSITE
}

impl Default for Config {
    fn default() -> Self {
        Self {
            target_composite: default_target_composite(),
            max_steps: None,
            max_cost_usd: None,
            max_tokens: None,
            max_wall_seconds: None,
            max_idle_seconds: None,
            max_repeated_steps: None,
            max_rerun_attempts: None,
            require_approval: false,
            allow_outside_targets: false,
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

    /// Fail-closed load for loop commands (ARCHITECTURE-CONDENSED 5.2):
    /// a malformed `.miniagi.json` or a non-numeric `MINIAGI_*` bound is
    /// a hard error that refuses dispatch/verify — NOT a warning that
    /// silently means "unlimited" (finding 2).
    ///
    /// # Errors
    ///
    /// Returns a message naming the malformed file or env bound.
    pub fn load_checked(root: &Path) -> Result<Self, String> {
        let mut cfg = Self::default();
        let path = root.join(".miniagi.json");
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                cfg = serde_json::from_str::<Self>(&text)
                    .map_err(|e| format!("{} is invalid JSON: {e}", path.display()))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                // PRESENT but unreadable (permissions/io): failing open to
                // defaults would silently disable every worker cap.
                return Err(format!(
                    "{} is present but unreadable ({e}) — refusing to run with unlimited caps",
                    path.display()
                ));
            }
        }
        let check = |name: &str, kind: &str, parse: fn(&str) -> bool| {
            if let Some(raw) = std::env::var(name).ok()
                && !parse(&raw)
            {
                return Err(format!("{name} is not a {kind} ('{raw}')"));
            }
            Ok(())
        };
        check("MINIAGI_TARGET_COMPOSITE", "number", |s| {
            s.parse::<f64>().is_ok()
        })?;
        check("MINIAGI_MAX_COST_USD", "number", |s| {
            s.parse::<f64>().is_ok()
        })?;
        check("MINIAGI_MAX_STEPS", "integer", |s| {
            s.parse::<usize>().is_ok()
        })?;
        check("MINIAGI_MAX_REPEATED_STEPS", "integer", |s| {
            s.parse::<usize>().is_ok()
        })?;
        check("MINIAGI_MAX_RERUN_ATTEMPTS", "integer", |s| {
            s.parse::<usize>().is_ok()
        })?;
        check("MINIAGI_MAX_TOKENS", "integer", |s| {
            s.parse::<u64>().is_ok()
        })?;
        check("MINIAGI_MAX_WALL_SECONDS", "integer", |s| {
            s.parse::<u64>().is_ok()
        })?;
        check("MINIAGI_MAX_IDLE_SECONDS", "integer", |s| {
            s.parse::<u64>().is_ok()
        })?;
        cfg.apply_env();
        Ok(cfg)
    }

    /// Environment overlay behind a lookup closure (testable without
    /// mutating process env — `unsafe_code = "forbid"`).
    fn apply_env_overlay(&mut self, get: impl Fn(&str) -> Option<String>) {
        let set_f64 = |name: &str, slot: &mut f64| {
            if let Some(raw) = get(name) {
                match raw.parse::<f64>() {
                    Ok(v) => *slot = v,
                    Err(e) => {
                        eprintln!("warning: {name} is not a number ('{raw}') — ignoring ({e})");
                    }
                }
            }
        };
        set_f64("MINIAGI_TARGET_COMPOSITE", &mut self.target_composite);

        let set_usize = |name: &str, slot: &mut Option<usize>| {
            if let Some(raw) = get(name) {
                match raw.parse::<usize>() {
                    Ok(v) => *slot = Some(v),
                    Err(e) => {
                        eprintln!("warning: {name} is not a number ('{raw}') — ignoring ({e})");
                    }
                }
            }
        };
        set_usize("MINIAGI_MAX_STEPS", &mut self.max_steps);
        set_usize("MINIAGI_MAX_REPEATED_STEPS", &mut self.max_repeated_steps);
        set_usize("MINIAGI_MAX_RERUN_ATTEMPTS", &mut self.max_rerun_attempts);
        let set_u64 = |name: &str, slot: &mut Option<u64>| {
            if let Some(raw) = get(name) {
                match raw.parse::<u64>() {
                    Ok(v) => *slot = Some(v),
                    Err(e) => {
                        eprintln!("warning: {name} is not a number ('{raw}') — ignoring ({e})");
                    }
                }
            }
        };
        set_u64("MINIAGI_MAX_TOKENS", &mut self.max_tokens);
        set_u64("MINIAGI_MAX_WALL_SECONDS", &mut self.max_wall_seconds);
        set_u64("MINIAGI_MAX_IDLE_SECONDS", &mut self.max_idle_seconds);
        if let Some(raw) = get("MINIAGI_MAX_COST_USD") {
            match raw.parse::<f64>() {
                Ok(v) => self.max_cost_usd = Some(v),
                Err(e) => eprintln!(
                    "warning: MINIAGI_MAX_COST_USD is not a number ('{raw}') — ignoring ({e})"
                ),
            }
        }
    }

    /// Loop gap-closing target for a repo (config-aware).
    #[must_use]
    pub fn target_composite_for(root: &Path) -> f64 {
        Self::load(root).target_composite
    }
}
