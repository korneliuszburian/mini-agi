# ADR-0014 — OWASP Agentic Top-10-2026 risk mapping

Status: accepted (2026-08-04)

## Context

Production-readiness (docs/PRODUCTION-READINESS.md, D.3) asked for an ADR
mapping each OWASP Agentic Top-10-2026 risk to a concrete mini-agi
control, so the taxonomy is auditable and gaps are explicit. OWASP
Agentic Applications Top 10 (2026) is a peer-reviewed checklist, not an
enforceable standard; this ADR maps it to the kernel's actual seams.

## Mapping

1. **Prompt injection** (indirect prompt injection via retrieved/tool
   context) — PARTIAL. Canonical memory is append-only with provenance
   and fact ids; the action log (D.1) records every kernel action; but
   the worker's own prompt assembly is host-agent territory. Mitigation:
   the counterfactual harness gate and judge-drift calibration bound the
   damage (a model-tampered claim is caught by the deterministic
   verifier before it closes).
2. **Excessive agency** (agent doing too much) — MITIGATED. Hard budget
   gates (max_steps/max_cost_usd/max_tokens, E), Landlock
   write-containment (ADR-0012), per-skill read-only sandbox (D.2),
   `loop objective` bounded batch dispatch.
3. **Improper tool usage** / tool misconfiguration — PARTIAL. The
   worker sandbox (ADR-0012) + the deterministic verifier; tool schemas
   are the host agent's. The MCP surface is read-only introspective.
4. **Insecure output handling** — PARTIAL. `run verify` treats the
   declared verify_command output as the ground truth and attributes it;
   the calibration corpus tracks verifier-vs-judge disagreement (C.3).
5. **Memory poisoning** (malicious memory writes) — MITIGATED.
   Append-only canonical with content-hash fact ids; `memory signoff`
   for contested facts; the provenance fingerprint in the audit; the
   judge-drift recalibration trigger.
6. **Hallucination / false claims** — MITIGATED. "Verified before
   trusted" (ADR-0011): a run's outcome is the run's OWN claim until the
   verifier confirms it; probe-vs-gate scoring (ADR-0013) keeps probe
   noise from zeroing or inflating composite.
7. **Data leakage / privacy** — PARTIAL. The action log records
   principal + content hash, deliberately NOT conversation contents
   (Anthropic practice); the worker sandbox confines writes.
8. **Unbounded / runaway autonomy** — MITIGATED. Budget gates (E), wall
   cap + live kill (worker.rs), repetition watchdog (P1-5), max_cases
   on loop objective.
9. **Denial of service / resource exhaustion** — PARTIAL. Wall/step/
   cost caps + the release CI's deterministic gate bound build-time
   cost; no hard network/cpu quotas (not-applicable to a local kernel).
10. **Supply chain** (compromised deps/plugins) — MITIGATED. cargo-deny
    (advisories/licenses/bans) in CI, pinned toolchain, `--locked`
    builds, artifact attestations in the release pipeline.

## Consequences

- The taxonomy is explicit and auditable; gaps are marked PARTIAL (host-
  agent territory) vs MITIGATED (kernel-enforced) vs N/A.
- Follow-ups on PARTIAL items are tracked in the PRODUCTION-READINESS
  backlog (permission layer, per-tool allowlists).

## Related
- docs/PRODUCTION-READINESS.md (D.1-D.4), docs/HARDENING-AUDIT.md.
