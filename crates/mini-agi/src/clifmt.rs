//! Truthful run.json draft builder shared by the codex worker and the
//! reparse path (hardening audit C.6: extracted from `main.rs`, which
//! built the same JSON in two places that drifted apart).

use mini_agi_core::capture::CapturedStep;

/// Build a truthful run.json draft from captured steps.
///
/// The draft is deliberately NOT a success claim: `outcome.achieved` is
/// false and the outcome gates are false until a deterministic verifier
/// confirms the work (verified before trusted, ADR-0011). `goal` and
/// `scope` come from the slice spec; `verify_command`/`verify_target`
/// from the caller or the spec's embedded verifier (P0-3).
#[must_use]
pub fn build_run_draft(
    goal: &str,
    scope: &[String],
    steps: &[CapturedStep],
    verify_command: Option<&str>,
    verify_target: Option<&str>,
) -> serde_json::Value {
    let trajectory: Vec<serde_json::Value> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            serde_json::json!({
                "step": i + 1,
                "action": s.action,
                "tool": s.tool,
                "ok": s.ok,
                "goal_aligned": null,
                "tokens": 0,
                "output_tokens": 0,
                "note": format!("captured from codex transcript line {}", s.line),
                "paths": s.paths,
            })
        })
        .collect();
    serde_json::json!({
        "goal": goal,
        "scope": scope,
        "outcome": {"achieved": false, "tests": false, "typecheck": false},
        "tokens_total": 0,
        "cost_usd": 0.0,
        "golden": null,
        "verify_command": verify_command,
        "verify_target": verify_target,
        "trajectory": trajectory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(tool: &str, action: &str) -> CapturedStep {
        CapturedStep {
            line: 1,
            tool: tool.to_string(),
            action: action.to_string(),
            paths: vec![],
            ok: None,
        }
    }

    #[test]
    fn draft_is_never_a_success_claim() {
        let d = build_run_draft(
            "goal",
            &["src/".to_string()],
            &[step("exec", "make verify")],
            Some("make verify"),
            Some("workdir"),
        );
        assert_eq!(d["outcome"]["achieved"], false);
        assert_eq!(d["verify_command"], "make verify");
        assert_eq!(d["verify_target"], "workdir");
        assert_eq!(d["scope"][0], "src/");
        assert_eq!(d["trajectory"][0]["action"], "make verify");
    }

    #[test]
    fn draft_keeps_ok_from_the_capture() {
        let d = build_run_draft("g", &[], &[step("exec", "probe")], None, None);
        assert!(d["verify_command"].is_null());
        assert!(d["trajectory"][0]["ok"].is_null());
    }
}
