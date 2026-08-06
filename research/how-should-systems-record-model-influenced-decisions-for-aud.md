## Findings

**How the sources frame the three mechanisms.** The question's three candidates are not rival formats from one standard; they come from different families of primary sources — software-engineering decision records, provenance/attestation specifications, and AI-regulation record-keeping rules — and they answer different audit questions. Claims below are tagged `fact` (verbatim from the cited primary source I read), `estimate` (inference from verified facts), or `opinion` (synthesis).

### A. Decision logs (records of what was decided, by whom, when, why)

- `fact` — Architecture Decision Records (ADR): an ADR is a short text file recording one "architecturally significant" decision with sections **Title, Context, Decision, Status, Consequences**; records are numbered sequentially and monotonically, never reused; a reversed decision is kept but marked "superseded" with a reference to its replacement; kept in version control under `doc/arch/adr-NNN.md`. — *"Documenting Architecture Decisions," Michael Nygard, Cognitect, Nov 15 2011* [S1].
- `fact` — Rationale preservation is the ADR's stated purpose: without it a new team member can only "blindly accept" or "blindly change" a past decision, and "the motivation behind previous decisions is visible for everyone, present and future." — Nygard [S1].
- `fact` — Kubernetes Enhancement Proposals (KEP): a proposal-format process with "a clear process with approvers and reviewers for making decisions," giving "a discoverable record around the decisions"; each KEP is numbered by its tracking issue and required for most non-trivial changes. — *kubernetes/enhancements, keps/README.md* [S2].
- `fact` — Application-log content guidance: "The application logs must record 'when, where, who and what' for each event," with per-attribute fields including event type, severity, interaction identifier, user identity, action, object, result status, and reason. — *OWASP Logging Cheat Sheet* [S3].
- `fact` — Log separation: "Process monitoring, audit, and transaction logs/trails etc. are usually collected for different purposes than security event logging, and this often means they should be kept separate." — OWASP [S3].
- `fact` — Plain logs have weak non-repudiation: "non-repudiation is hard to achieve for logs because their trustworthiness is often just based on the logging party being audited properly while mechanisms like digital signatures are hard to utilize here." — OWASP [S3].
- `fact` — Log integrity at rest requires "tamper detection," read-only media "as soon as possible," restricted, monitored access; event data from other trust zones must be treated as untrusted. — OWASP [S3].

### B. Evidence chains (what inputs/outputs/process produced this result, cryptographically bound)

- `fact` — W3C PROV defines provenance as "information about entities, activities, and people involved in producing a piece of data or thing, which can be used to form assessments about its quality, reliability or trustworthiness"; the family's recommendations include reproducibility, versioning, attribution, processing steps, and derivation; PROV-DM is the conceptual model, PROV-O its OWL2/RDF serialization. — *W3C PROV-Overview (Working Group Note, 2013)* [S4].
- `fact` — in-toto (CNCF graduated project) "is designed to ensure the integrity of a software product from initiation to end-user installation… making it transparent to the user what steps were performed, by whom and in what order." — *in-toto.io* [S5].
- `fact` — The in-toto attestation Statement is the layer "binding it to a particular subject and unambiguously identifying the types of the Predicate"; every subject element "MUST have `digest` set" (subjects are matched purely by digest), and policies should be monotonic (ignoring an attestation/field must never turn DENY into ALLOW). — *in-toto attestation spec v1, statement.md + README* [S6].
- `fact` — SLSA provenance is "an attestation that a particular build platform produced a set of software artifacts through execution of the buildDefinition"; the model separates `buildDefinition` (buildType, untrusted `externalParameters`, trusted `internalParameters`, `resolvedDependencies` with digests) from `runDetails` (`builder.id`, `invocationId`, timestamps, `byproducts`); "Consumers MUST accept only specific signer-builder pairs." — *SLSA v1.0 provenance spec* [S7].
- `fact` — W3C Verifiable Credentials Data Integrity: cryptographic proofs make "documents and data tamper-evident"; **Proof Sets** are unordered multiple signatures, while **Proof Chains** preserve order via `previousProof` (the spec's own example is a notary counter-signing), and `proofPurpose` constrains what a proof may be used for. — *W3C VC Data Integrity 1.0 (Recommendation, 2025)* [S8].

### C. Approval records (who authorized / who oversaw)

- `fact` — EU AI Act, high-risk systems: "Deployers shall assign human oversight to natural persons who have the necessary competence, training and authority" (Art 26(2)); for biometric identification, "no action or decision may be taken by the deployer on the basis of the identification… unless this has been separately verified and confirmed by at least two natural persons," and the separate verifications "could be sufficient… automatically recorded in the logs generated by the system" (recital 72). — *Regulation (EU) 2024/1689 (AI Act), EUR-Lex* [S9].
- `fact` — EU AI Act: post-remote biometric identification "shall request an authorisation, ex ante… by a judicial authority or an administrative authority whose decision is binding and subject to judicial review"; if the authorisation is rejected, use stops immediately and the linked personal data is deleted (Art 26(10)). — *AI Act, EUR-Lex* [S9].
- `fact` — The KEP process's approver/reviewer approval is itself the recorded gate ("approvers and reviewers for making decisions… discoverable record"), i.e., an approval step persisted as part of the decision record. — *kubernetes/enhancements* [S2].
- `fact` — VC Data Integrity's proof sets/chains are a standards-level mechanism for *multi-party* signing of one document (unordered co-signatures vs. ordered counter-signatures) — the substrate approval records could be built on. — W3C [S8].

### D. Binding regulatory record-keeping requirements (decision-log obligations in law)

- `fact` — AI Act Art 12(1): "High-risk AI systems shall technically allow for the automatic recording of events (logs) over the lifetime of the system." Logging must enable recording of events relevant to (a) identifying risk situations or substantial modification, (b) post-market monitoring, (c) monitoring operation; for biometric systems the minimum logged fields include the period of each use (start/end time), the reference database checked, input data that produced a match, and identification of the natural persons involved in verifying results (Art 12(2)-(3)). — *EUR-Lex* [S9].
- `fact` — Retention: providers must keep these logs "for a period appropriate to the intended purpose… of at least six months" (Art 19(1)); deployers have the same six-month minimum (Art 26(6)); financial institutions keep them as part of internal-governance documentation. — *EUR-Lex* [S9].
- `fact` — Access: upon a reasoned request, providers must give competent authorities access to the automatically generated logs (Art 21(2)). — *EUR-Lex* [S9].
- `fact` — Transparency: high-risk systems must be "sufficiently transparent to enable deployers to interpret a system's output and use it appropriately," and instructions for use must state capabilities and limitations, known risks, and human-oversight measures (Art 13). — *EUR-Lex* [S9].
- `fact` — NIST AI RMF 1.0 (released 26 Jan 2023, voluntary) organizes risk management into four functions — **Govern, Map, Measure, Manage** — with a companion Playbook of suggested actions per sub-category. — *NIST AI RMF page* [S10].
- `fact` — AI RMF Playbook, Govern: sub-category **GOVERN 1.1** "Legal and regulatory requirements involving AI are understood, managed, and documented"; **GOVERN 1.4** urges standardized documentation (AI-actor contact info, business justification, scope and usages, expected risks, assumptions and limitations, training-data characterization, algorithmic methodology, testing and validation results, dependencies, deployment/monitoring/change-management plans) plus a "model documentation inventory"; **GOVERN 1.5** covers ongoing monitoring, incident response, and "appeal and override" (human adjudication of system outcomes); **GOVERN 2.1** requires roles, responsibilities, and lines of communication to be "documented and clear." — *NIST AI RMF Playbook, Govern* [S11].

### E. Not verifiable from the sources reached

- `unknown — not verifiable` — NIST SP 800-92 (r1) content: OWASP references the 2006 *Guide to Computer Security Log Management* [S3], but I could not retrieve the r1 document (CSRC/DOI/nvlpubs fetches 404'd) and I did not read it; do not attribute any specific claim to it.
- `unknown — not verifiable` — ISO/IEC 42001:2023 (AI management systems) is a known standard, but its normative text is paywalled and was not read here; not used as a source.
- `unknown — not verifiable` — The NIST AI RMF 1.0 *Core* PDF was only partially readable (the fetch truncated and the local copy could not be saved); the AI RMF claims above rest on the NIST landing page and the AIRC Playbook HTML, not on the Core PDF itself.

### F. Comparative reading (synthesis)

- `opinion` — The three mechanisms answer different audit questions: decision logs answer *what was decided and why* (rationale + when/who/what fields); evidence chains answer *what produced this result and can it be re-verified* (attribution + derivation + digests, tamper-evident); approval records answer *who authorized it* (a human gate persisted as a record). ADRs/KEPs [S1][S2] are rationale-first decision logs; PROV/in-toto/SLSA [S4][S5][S6][S7] are structural evidence chains; AI Act Art 14/26(10) [S9] are legally binding approval gates whose output is itself written into the log.
- `opinion` — In the EU AI Act these layers are **combined by law**, not alternatives: Art 12 mandates an automatic decision log; Art 14/26 require that human-oversight and biometric authorisation events be captured (as approvals *inside* the logs, recital 72); Art 19/26(6) set retention; Art 21 gives authorities log access.
- `opinion` — The weakness of a bare decision log is integrity and non-repudiation — OWASP states plain logs cannot reliably provide it without signatures [S3]. The evidence-chain standards supply exactly the missing machinery (signed Statements bound to subjects by digest [S6], provenance predicates [S7], proof chains [S8]). The suggested shape is therefore a decision log whose entries embed digest references that are themselves bound by an attestation/evidence chain, with approval records as human-signed gates on top — not one mechanism chosen over the others.

## Sources

- [S1] Nygard, M., "Documenting Architecture Decisions" (Cognitect blog, 15 Nov 2011) — https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions
- [S2] Kubernetes Enhancement Proposals, keps/README.md (kubernetes/enhancements) — https://raw.githubusercontent.com/kubernetes/enhancements/master/keps/README.md
- [S3] OWASP Logging Cheat Sheet — https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html
- [S4] W3C PROV-Overview (Working Group Note, 30 Apr 2013) — https://www.w3.org/TR/prov-overview/
- [S5] in-toto project home — https://in-toto.io/
- [S6] in-toto attestation spec v1 (README, Statement layer) — https://github.com/in-toto/attestation/blob/main/spec/v1/README.md and https://raw.githubusercontent.com/in-toto/attestation/main/spec/v1/statement.md
- [S7] SLSA v1.0 Specification, Provenance — https://slsa.dev/spec/v1.0/provenance
- [S8] W3C Verifiable Credential Data Integrity 1.0 (Recommendation, 15 May 2025) — https://www.w3.org/TR/vc-data-integrity/
- [S9] Regulation (EU) 2024/1689 (AI Act), Articles 12, 13, 19, 21, 26 and recital 72 (EUR-Lex, OJ L 2024/1689) — https://eur-lex.europa.eu/eli/reg/2024/1689/oj/eng
- [S10] NIST AI Risk Management Framework (landing page) — https://www.nist.gov/itl/ai-risk-management-framework
- [S11] NIST AI RMF Playbook, Govern (NIST AIRC) — https://airc.nist.gov/airmf-resources/playbook/govern/

## Verdict

**Established (fact):** Each mechanism is a real, standards-/law-backed practice with distinct content. Decision logs: ADR/KEP formats record rationale and are versioned [S1][S2]; audit-log practice mandates when/who/what fields and separate audit trails [S3]. Evidence chains: PROV models attribution/derivation [S4]; in-toto/SLSA bind predicates to subjects by digest with tamper-evident signed attestations [S5][S6][S7]; VC Data Integrity supplies proof sets/chains and proof purposes [S8]. Approval records: the EU AI Act makes human-oversight and biometric-authorisation approvals mandatory and records them in system logs with six-month retention [S9]; the NIST AI RMF Playbook requires documented roles, standardized model documentation, and an inventory [S10][S11].

**Uncertain:** (1) Whether any single standard prescribes how to *combine* all three for AI decision auditability — no primary source reached does; it is legal text + practice guides, not one integrated spec. (2) NIST SP 800-92r1 content and ISO/IEC 42001 content were not verified (retrieval/paywall), and the AI RMF Core PDF was not fully read.

**What would settle it:** read NIST SP 800-92r1 and ISO/IEC 42001:2023 in full; read the complete AI RMF 1.0 Core and the GAO-21-519SP AI Accountability Framework (referenced by the Playbook [S11]); and check whether in-toto/attestation or SLSA maintainers publish a machine-attestation schema for "AI decision provenance" that ties decision-log entries to signed evidence chains.
