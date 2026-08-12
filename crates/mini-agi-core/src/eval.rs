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
    /// Legacy golden reference (unused).
    #[serde(default)]
    pub golden: Option<String>,
    /// Deterministic gate command run in `verify_target`.
    #[serde(default)]
    pub verify_command: Option<String>,
    /// Directory where `verify_command` runs.
    #[serde(default)]
    pub verify_target: Option<String>,
    /// Verbal self-reflection.
    #[serde(default)]
    pub reflection: Option<String>,
    /// MAST failure classification.
    #[serde(default)]
    pub mast: Option<String>,
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

/// Failure-mode classification used by the harness counterfactual gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepairSignal {
    /// A gate/goal/revert failure on a step — a corrected retry helps.
    Mechanical,
    /// Clean steps but the outcome is unachieved — change approach.
    Semantic,
    /// The trajectory repeats the same (tool, action) — break the loop.
    Spinning,
}
