//! Run data model (condensed).
//!
//! The evaluation/scoring machinery was removed as over-verification:
//! a run is a self-report (`Run`) plus the declared deterministic gate
//! (`verify_command`/`verify_target`) that the loop executes to verify
//! it. The business is knowledge and patterns, not 4D scoring.

use serde::{Deserialize, Serialize};

/// One run step (the trajectory).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct Step {
    /// 1-based step index.
    #[serde(default)]
    pub step: u32,
    /// Action payload.
    #[serde(default)]
    pub action: String,
    /// Tool used for this step.
    pub tool: String,
    /// Gate pass; `null` = not gated.
    #[serde(default)]
    pub ok: Option<bool>,
    /// On goal; `null` = not scored.
    #[serde(default)]
    pub goal_aligned: Option<bool>,
    /// Tokens spent.
    #[serde(default)]
    pub tokens: u64,
    /// Output tokens produced.
    #[serde(default)]
    pub output_tokens: u64,
    /// Edit-commit reverted.
    #[serde(default)]
    pub reverted: bool,
    /// Free-form note.
    #[serde(default)]
    pub note: String,
    /// File paths touched.
    #[serde(default)]
    pub paths: Vec<String>,
}

/// Outcome block of a run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct Outcome {
    /// Whether the run achieved its goal (its own claim until the gate passes).
    #[serde(default)]
    pub achieved: bool,
}

/// A run: the input contract of the loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Run {
    /// The goal given to the agent.
    pub goal: String,
    /// Declared write scope.
    pub scope: Vec<String>,
    /// Outcome block.
    pub outcome: Outcome,
    /// Total tokens spent.
    #[serde(default)]
    pub tokens_total: u64,
    /// Run cost in USD.
    #[serde(default)]
    pub cost_usd: f64,
    /// Deterministic gate command run in `verify_target`.
    #[serde(default)]
    pub verify_command: Option<String>,
    /// Directory where `verify_command` runs.
    #[serde(default)]
    pub verify_target: Option<String>,
    /// Steps of the run.
    pub trajectory: Vec<Step>,
    /// Kernel version that produced the run.
    #[serde(default)]
    pub kernel_version: Option<String>,
    /// Step count.
    #[serde(default)]
    pub n_steps: Option<u64>,
    /// Toolcall count.
    #[serde(default)]
    pub n_toolcalls: Option<u64>,
    /// Latency seconds.
    #[serde(default)]
    pub latency_seconds: Option<u64>,
    /// Run mode.
    #[serde(default)]
    pub mode: Option<String>,
    /// Extra fields.
    #[serde(default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Run {
    /// The run's own claim: whether it reports achieving its goal.
    #[must_use]
    pub const fn achieved(&self) -> bool {
        self.outcome.achieved
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;

    #[test]
    fn fake_compat_fields_are_gone_from_run_serialization() {
        let run: Run = serde_json::from_str(
            r#"{"goal": "g", "scope": ["x"], "outcome": {"achieved": true}, "trajectory": []}"#,
        )
        .unwrap();
        let obj = serde_json::to_value(&run).unwrap();
        let obj = obj.as_object().unwrap();
        for fake in ["golden", "reflection", "mast"] {
            assert!(
                !obj.contains_key(fake),
                "{fake} must be deleted (MUST-FIX 4)"
            );
        }
        assert!(run.achieved());
    }

    #[test]
    fn run_parses_minimal_contract_and_defaults_missing_fields() {
        let run: Run = serde_json::from_str(
            r#"{"goal": "fix x", "scope": [], "outcome": {}, "trajectory": []}"#,
        )
        .unwrap();
        assert!(!run.achieved(), "absent achieved defaults to false");
        assert_eq!(run.goal, "fix x");
        assert!(run.verify_command.is_none(), "gate absent until declared");
        assert!(run.verify_target.is_none());
        assert_eq!(run.tokens_total, 0);
    }

    #[test]
    fn run_roundtrips_a_declared_gate() {
        let text = r#"{
            "goal": "g", "scope": ["crates/"], "outcome": {"achieved": false},
            "trajectory": [],
            "verify_command": "cargo test",
            "verify_target": "crates/mini-agi-core"
        }"#;
        let run: Run = serde_json::from_str(text).unwrap();
        assert_eq!(run.verify_command.as_deref(), Some("cargo test"));
        assert_eq!(run.verify_target.as_deref(), Some("crates/mini-agi-core"));
        let back = serde_json::to_value(&run).unwrap();
        assert_eq!(
            back["verify_command"], "cargo test",
            "the gate survives the roundtrip"
        );
    }

    #[test]
    fn run_ignores_unknown_forward_compatible_fields() {
        let run: Run = serde_json::from_str(
            r#"{"goal": "g", "scope": [], "outcome": {"achieved": true},
                "trajectory": [], "some_future_field": 42, "judge": {"n": 1}}"#,
        )
        .unwrap();
        assert!(run.achieved(), "known fields still parse");
        let obj = serde_json::to_value(&run).unwrap();
        assert!(
            !obj.as_object().unwrap().contains_key("some_future_field"),
            "unknown fields are dropped, not stored"
        );
    }
}
