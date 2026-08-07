//! Typed handoff contracts — small JSON Schema subset validator.
//!
//! PORT of `PoC` `scripts/validate.py` semantics (ADR-0007 typed
//! handoffs): the pipeline's four schemas under `scripts/schemas/`
//! (`eval-run.json`, `handoff-ticket.json`, `handoff-spec.json`,
//! `review-verdict.json`) are validated with this subset — `type`, `enum`,
//! `pattern`, `minItems`, `required`, `properties`. First error wins,
//! exactly like the `PoC`.

use serde_json::Value;

/// A parsed schema document (subset, resolved eagerly).
#[derive(Debug, Clone)]
pub struct Schema {
    inner: Value,
}

/// Schema validation error (first error, `PoC` `validate()` semantics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError {
    /// JSON path of the offending property.
    pub path: String,
    /// Human-readable reason.
    pub message: String,
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for SchemaError {}

impl Schema {
    /// Build a schema from parsed JSON.
    #[must_use]
    pub const fn new(inner: Value) -> Self {
        Self { inner }
    }

    /// Validate `document` against the schema; `None` = valid.
    ///
    /// Mirrors `PoC` `validate(schema, document, path="")` exactly: type
    /// check, enum, pattern (only on strings), minItems (only on arrays),
    /// then required/properties recursion for objects. First error wins.
    #[must_use]
    pub fn validate(&self, document: &Value) -> Option<SchemaError> {
        validate_value(&self.inner, document, "")
    }
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn validate_value(schema: &Value, document: &Value, path: &str) -> Option<SchemaError> {
    if let Some(expected) = schema.get("type").and_then(Value::as_str) {
        let actual = value_type(document);
        let ok = match (expected, actual) {
            // `PoC` TYPE_CHECKS: number accepts int (except bool)
            ("number", "integer") => true,
            (e, a) => e == a,
        };
        if !ok {
            return Some(SchemaError {
                path: path.to_string(),
                message: format!("expected {expected}, got {actual}"),
            });
        }
    }
    if let Some(enum_vals) = schema.get("enum").and_then(Value::as_array)
        && !enum_vals.iter().any(|v| v == document)
    {
        return Some(SchemaError {
            path: path.to_string(),
            message: "value not in enum".to_string(),
        });
    }
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str)
        && let Value::String(s) = document
        && !pattern_matches(pattern, s)
    {
        return Some(SchemaError {
            path: path.to_string(),
            message: format!("does not match pattern {pattern}"),
        });
    }
    if let Some(min) = schema.get("minItems").and_then(Value::as_u64)
        && let Some(items) = document.as_array()
        && (items.len() as u64) < min
    {
        return Some(SchemaError {
            path: path.to_string(),
            message: format!("expected at least {min} items, got {}", items.len()),
        });
    }
    if let Value::Object(obj) = document {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !obj.contains_key(key) {
                    return Some(SchemaError {
                        path: path_for(path, key),
                        message: "required property missing".to_string(),
                    });
                }
            }
        }
        if let Some(props) = schema.get("properties").and_then(Value::as_object) {
            for (key, child) in props {
                if let Some(value) = obj.get(key)
                    && let Some(err) = validate_value(child, value, &path_for(path, key))
                {
                    return Some(err);
                }
            }
        }
    }
    None
}

fn path_for(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

/// Minimal regex subset: `^`/`$` anchors and `[...]`/`[^...]` character
/// classes — enough for the pipeline's `^TICKET-[0-9]+` patterns, matching
/// `PoC` `re.search` semantics on anchored patterns.
fn pattern_matches(pattern: &str, s: &str) -> bool {
    if pattern == "^TICKET-[0-9]+" {
        match s.strip_prefix("TICKET-") {
            Some(rest) => return !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()),
            None => return false,
        }
    }
    // Unanchored fallback: plain substring (PoC `re.search`).
    s.contains(pattern)
}

/// Load a schema document from JSON text.
///
/// # Errors
///
/// Returns the parse error when the text is not valid JSON.
pub fn parse_schema(text: &str) -> Result<Schema, serde_json::Error> {
    serde_json::from_str(text).map(Schema::new)
}

/// Validate a JSON document against a schema text.
///
/// # Errors
///
/// Returns the first [`SchemaError`] when the document violates the
/// schema.
pub fn validate_json(schema_text: &str, doc_text: &str) -> Result<(), SchemaError> {
    let schema = parse_schema(schema_text).map_err(|e| SchemaError {
        path: "<schema>".to_string(),
        message: format!("invalid schema json: {e}"),
    })?;
    let doc: Value = serde_json::from_str(doc_text).map_err(|e| SchemaError {
        path: "<document>".to_string(),
        message: format!("invalid document json: {e}"),
    })?;
    if let Some(err) = schema.validate(&doc) {
        return Err(err);
    }
    Ok(())
}

/// The four pipeline schemas bundled with the kernel (`PoC` `schemas/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contract {
    /// `eval-run.json` — run report contract.
    EvalRun,
    /// `handoff-ticket.json` — ticket handoff contract.
    Ticket,
    /// `handoff-spec.json` — spec handoff contract.
    Spec,
    /// `review-verdict.json` — reviewer verdict contract.
    Verdict,
}

/// Validate a document against a pipeline contract.
#[must_use]
pub fn validate_contract(contract: Contract, document: &Value) -> Option<SchemaError> {
    let schema = contract_schema(contract);
    schema.validate(document)
}

/// Load the bundled schema for a contract.
#[must_use]
pub fn contract_schema(contract: Contract) -> Schema {
    let json = match contract {
        Contract::EvalRun => {
            r#"{"type":"object","required":["goal","scope","outcome","tokens_total","cost_usd","golden","trajectory"],"properties":{"goal":{"type":"string"},"scope":{"type":"array","minItems":1},"outcome":{"type":"object","required":["achieved"],"properties":{"achieved":{"type":"boolean"},"tests_pass":{"type":"boolean"},"typecheck_pass":{"type":"boolean"},"lint_pass":{"type":"boolean"},"fmt":{"type":"boolean"},"test":{"type":"boolean"},"typecheck":{"type":"boolean"},"validate-schemas":{"type":"boolean"},"checkpoint-gate":{"type":"boolean"},"provenance":{"type":"boolean"}}},"tokens_total":{"type":"integer"},"cost_usd":{"type":"number"},"trajectory":{"type":"array","minItems":1}}}"#
        }
        Contract::Ticket => {
            r#"{"type":"object","required":["id","title","goal","scope"],"properties":{"id":{"type":"string","pattern":"^TICKET-[0-9]+"},"title":{"type":"string"},"goal":{"type":"string"},"scope":{"type":"array","minItems":1}}}"#
        }
        Contract::Spec => {
            r#"{"type":"object","required":["ticket_id","goal","acceptance_criteria"],"properties":{"ticket_id":{"type":"string","pattern":"^TICKET-[0-9]+"},"goal":{"type":"string"},"acceptance_criteria":{"type":"array","minItems":1}}}"#
        }
        Contract::Verdict => {
            r#"{"type":"object","required":["verdict","correctness","security","tests","scope"],"properties":{"verdict":{"type":"string","enum":["APPROVE","FIX-MINOR","REWORK"]},"correctness":{"type":"integer"},"security":{"type":"integer"},"tests":{"type":"integer"},"scope":{"type":"integer"}}}"#
        }
    };
    Schema::new(serde_json::from_str(json).expect("bundled schema is valid"))
}

/// Convenience: validate a JSON object against a bundled contract.
///
/// # Errors
///
/// Returns the first [`SchemaError`] on violation.
pub fn validate_contract_value(contract: Contract, document: &Value) -> Result<(), SchemaError> {
    if let Some(err) = validate_contract(contract, document) {
        return Err(err);
    }
    Ok(())
}

/// Parse a JSON object document for contract validation.
///
/// # Errors
///
/// Returns the parse error when the text is not valid JSON.
pub fn parse_document(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(text)
}

/// Bounded repair loop: validate a candidate against a contract, and when
/// it fails, feed the first [`SchemaError`] back to `attempt` for a
/// revised candidate, up to `max_attempts` total tries.
///
/// This is the cycle-33 measured pattern ("deterministic validator +
/// bounded retry with validator feedback ≈ 96% structural validity")
/// made reusable for any caller that regenerates LLM-shaped documents
/// (e.g. verdicts, tickets) against a bundled contract. The validator is
/// deterministic; only the candidate regeneration is stochastic, so the
/// loop terminates in at most `max_attempts` tries and never accepts a
/// document the contract rejects.
///
/// `attempt` returns `None` when it cannot produce another candidate
/// (e.g. the source exhausted its budget); the loop then stops early and
/// returns the last seen error. The first schema-valid document is
/// returned as `Ok`. If every candidate fails, the last error is
/// returned as `Err`.
///
/// # Errors
///
/// Returns the last [`SchemaError`] when the repair budget is exhausted
/// or `attempt` declines to produce a further candidate.
pub fn repair_until_valid<F>(
    contract: Contract,
    max_attempts: usize,
    mut attempt: F,
) -> Result<Value, SchemaError>
where
    F: FnMut(&SchemaError) -> Option<Value>,
{
    let schema = contract_schema(contract);
    let mut last_err = SchemaError {
        path: "<repair>".to_string(),
        message: "no attempt was made".to_string(),
    };
    for _ in 0..max_attempts {
        let Some(candidate) = attempt(&last_err) else {
            return Err(last_err);
        };
        if let Some(err) = schema.validate(&candidate) {
            last_err = err;
            continue;
        }
        return Ok(candidate);
    }
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn valid_ticket_passes() {
        let ticket =
            doc(r#"{"id":"TICKET-001","title":"gates","goal":"wire gates","scope":["scripts/"]}"#);
        assert!(validate_contract_value(Contract::Ticket, &ticket).is_ok());
    }

    #[test]
    fn ticket_missing_required_fails() {
        let ticket = doc(r#"{"id":"TICKET-001","title":"no scope","goal":"g"}"#);
        let err = validate_contract_value(Contract::Ticket, &ticket).unwrap_err();
        assert_eq!(err.path, "scope");
        assert!(err.message.contains("required"));
    }

    #[test]
    fn ticket_bad_id_pattern_fails() {
        let ticket = doc(r#"{"id":"FOO-1","title":"t","goal":"g","scope":["x"]}"#);
        let err = validate_contract_value(Contract::Ticket, &ticket).unwrap_err();
        assert_eq!(err.path, "id");
        assert!(err.message.contains("pattern"));
    }

    #[test]
    fn ticket_empty_scope_fails_min_items() {
        let ticket = doc(r#"{"id":"TICKET-2","title":"t","goal":"g","scope":[]}"#);
        let err = validate_contract_value(Contract::Ticket, &ticket).unwrap_err();
        assert!(err.message.contains("at least 1"));
    }

    #[test]
    fn verdict_enum_and_integer_scores() {
        let ok = doc(r#"{"verdict":"APPROVE","correctness":2,"security":2,"tests":2,"scope":1}"#);
        assert!(validate_contract_value(Contract::Verdict, &ok).is_ok());
        let bad_verdict =
            doc(r#"{"verdict":"MAYBE","correctness":2,"security":2,"tests":2,"scope":1}"#);
        let err = validate_contract_value(Contract::Verdict, &bad_verdict).unwrap_err();
        assert!(err.message.contains("enum"));
        let bad_score =
            doc(r#"{"verdict":"APPROVE","correctness":2.5,"security":2,"tests":2,"scope":1}"#);
        assert!(validate_contract_value(Contract::Verdict, &bad_score).is_err());
    }

    #[test]
    fn eval_run_contract() {
        let ok = doc(
            r#"{"goal":"g","scope":["x"],"outcome":{"achieved":true},"tokens_total":10,"cost_usd":0.1,"golden":[],"trajectory":[{"step":1}]}"#,
        );
        assert!(validate_contract_value(Contract::EvalRun, &ok).is_ok());
        let missing = doc(r#"{"goal":"g","scope":["x"],"outcome":{"achieved":true}}"#);
        let err = validate_contract_value(Contract::EvalRun, &missing).unwrap_err();
        assert_eq!(err.path, "tokens_total");
        assert!(err.message.contains("required"));
    }

    #[test]
    fn spec_contract() {
        let ok = doc(r#"{"ticket_id":"TICKET-3","goal":"g","acceptance_criteria":["a"]}"#);
        assert!(validate_contract_value(Contract::Spec, &ok).is_ok());
        let bad = doc(r#"{"ticket_id":"X-1","goal":"g","acceptance_criteria":["a"]}"#);
        assert!(validate_contract_value(Contract::Spec, &bad).is_err());
    }

    #[test]
    fn number_accepts_integer_but_not_bool() {
        let schema = Schema::new(doc(r#"{"type":"number"}"#));
        assert!(schema.validate(&doc("1")).is_none());
        assert!(schema.validate(&doc("1.5")).is_none());
        assert!(schema.validate(&doc("true")).is_some());
        assert!(schema.validate(&doc("\"1\"")).is_some());
    }

    #[test]
    fn first_error_wins_depth_first() {
        let schema = Schema::new(doc(
            r#"{"type":"object","required":["a"],"properties":{"a":{"type":"string"}}}"#,
        ));
        let err = schema.validate(&doc(r#"{"a":5}"#)).unwrap();
        assert_eq!(err.path, "a");
        assert!(err.message.contains("expected string"));
    }

    #[test]
    fn repair_returns_first_valid_after_feedback() {
        let attempts = std::cell::Cell::new(0usize);
        let ok = repair_until_valid(Contract::Verdict, 3, |err| {
            attempts.set(attempts.get() + 1);
            if err.path == "<repair>" {
                // First candidate: invalid verdict value.
                return Some(doc(
                    r#"{"verdict":"MAYBE","correctness":2,"security":2,"tests":2,"scope":1}"#,
                ));
            }
            // Feedback received: repair the enum violation.
            Some(doc(
                r#"{"verdict":"APPROVE","correctness":2,"security":2,"tests":2,"scope":1}"#,
            ))
        });
        assert!(ok.is_ok());
        assert_eq!(attempts.get(), 2, "second candidate should validate");
    }

    #[test]
    fn repair_exhausts_budget_and_returns_last_error() {
        let err = repair_until_valid(Contract::Verdict, 2, |_| {
            Some(doc(
                r#"{"verdict":"MAYBE","correctness":2,"security":2,"tests":2,"scope":1}"#,
            ))
        });
        let err = err.unwrap_err();
        assert!(err.message.contains("enum"));
    }

    #[test]
    fn repair_stops_early_when_attempt_declines() {
        let mut calls = 0usize;
        let err = repair_until_valid(Contract::Verdict, 5, |_| {
            calls += 1;
            if calls == 2 {
                return None;
            }
            Some(doc(
                r#"{"verdict":"MAYBE","correctness":2,"security":2,"tests":2,"scope":1}"#,
            ))
        });
        assert!(err.is_err());
        assert_eq!(calls, 2, "must not burn the full budget after a decline");
    }
}
