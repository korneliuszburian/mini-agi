## Findings

**Scope note.** "Enforcement-bound memory" is this repo's term; no production system uses that exact phrase. The closest primary-source concepts are (a) the policy/data state that an authorization or enforcement decision is computed from, and (b) the tamper-evident trail that records those decisions. The findings map the question onto concrete mechanisms in OPA, RocksDB, Akka, Vault, Kafka, and Rekor.

**F1 — Audit trails and enforceable state are kept as separate, differently-treated artifacts.**
- OPA: policy and data (the enforcement input) live in the OPA data document and are replaceable; decision logs are a separate stream. "The decision logs contain events that describe policy queries. Each event includes the policy that was queried, the input to the query, bundle metadata... that enables auditing and offline debugging of policy decisions." *Fact.* (OPA Docs, "Decision Logs": https://www.openpolicyagent.org/docs/latest/management-decision-logs/)
- Vault: "Vault audit devices record all API requests and responses in detail," distinct from server logs; disabling a device "immediately stops writing... but leaves the existing log information untouched." *Fact.* (Vault Docs, "Audit Devices": https://developer.hashicorp.com/vault/docs/audit)
- Akka: the journal (event log) is the history; the snapshot store is separate and exists only to optimize recovery. *Fact.* (Akka Docs, "Classic Persistence — Snapshot store": https://doc.akka.io/docs/akka/current/persistence.html#introduction)
- Rekor (sigstore): the trail is a verifiable log where consolidation is forbidden: "the log remains append-only and entries are never mutated or removed." *Fact.* (Sigstore Docs, "Rekor": https://docs.sigstore.dev/logging/overview/)

**F2 — Consolidation is a derived, copy-style operation over the immutable trail, never an in-place rewrite of it.**
- Akka snapshots: a snapshot is a point-in-time copy of state; recovery = "using the latest saved snapshot to initialize the state. Thereafter the events after the snapshot are replayed." Snapshots are taken while "incoming commands are stashed until the snapshot has been saved... The state instance will not be updated by new events until after the snapshot has been saved," so the snapshot is internally consistent. *Fact.* (Akka Docs, "Snapshotting": https://doc.akka.io/libraries/akka-core/current/typed/persistence-snapshot.html)
- Akka is explicit that consolidating away the trail destroys the audit: "By deleting events you will lose the history of how the system changed before it reached current state, which is one of the main reasons for using Event Sourcing in the first place," and warns `snapshot-is-optional = true` is unsafe "if events have been deleted because that would result in wrong recovered state." *Fact.* (same Akka page)

**F3 — During consolidation, live readers are protected by snapshot pinning: obsolete versions are dropped only when no snapshot still references them.**
- RocksDB: "Both flush and compaction use `CompactionIterator`... it determines if each key-value pair should be dropped or output... `CompactionIterator` is aware of all the snapshots and ensures that the data visible to each snapshot is preserved." A snapshot "captures a point-in-time view of the DB at the time it's created." *Fact.* (RocksDB Wiki, "Snapshot": https://github.com/facebook/rocksdb/wiki/Snapshot)
- RocksDB compaction is exactly "memory rewrite": sorted runs are merged and "a version of a key in L0 must be newer than versions of that same key in all levels below L0," i.e. superseded key versions are eliminated by compaction. *Fact.* (RocksDB Wiki, "Compaction": https://github.com/facebook/rocksdb/wiki/Compaction)

**F4 — What happens to access-control/policy facts during a rewrite: replacement is atomic and versioned, not a partial in-place edit.**
- OPA bundles: policy/data are distributed as immutable, signed bundles. "When a new *snapshot* bundle is downloaded, OPA will erase and overwrite all the policy and data in its cache before activating the new bundle" (scoped to declared `roots`). Activation is gated: "Only if that verification succeeds does OPA activate the new bundle; otherwise, OPA continues using its existing bundle and reports an activation failure." The REST API "will prevent you from modifying policy and data loaded via bundles." *Fact.* (OPA Docs, "Bundles": https://www.openpolicyagent.org/docs/latest/management-bundles/)
- OPA delta bundles rewrite data incrementally via JSON Patch (`upsert`/`replace`/`remove`) applied "in order" to the in-memory store; patch failure fails the whole activation; an empty patch list removes all data. *Fact.* (same OPA Bundles page)
- Failure mode without atomicity: OPA docs warn that with multiple bundle sources "there are **no** ordering guarantees for which bundle loads first and takes over some root. If multiple bundles conflict... OPA may go into an error state." *Fact.* (same page)

**F5 — Enforcement and audit stay reconstructible across policy rewrites by tagging each decision with the exact policy revision that produced it.**
- OPA decision-log events carry `bundles[_].revision` = "Revision of the bundle at the time of evaluation," plus a per-decision `decision_id` ("Unique identifier generated for each decision for traceability"). So a past decision can be replayed/audited against the policy revision that was in force even after that revision was erased from live memory. *Fact.* (OPA Docs, "Decision Logs": https://www.openpolicyagent.org/docs/latest/management-decision-logs/)

**F6 — Some systems make serving *depend* on the audit trail, coupling enforcement to trail integrity.**
- Vault: "if you have audit devices enabled and Vault cannot log information to at least one of the enabled devices, Vault refuses to service the corresponding API request." Audit data is confidentiality-protected via keyed HMAC-SHA256 hashes by default. *Fact.* (Vault Docs: https://developer.hashicorp.com/vault/docs/audit)

**F7 — Latest-value consolidation of a state log.** Kafka's `log.cleanup.policy=compact` "will enable log compaction, which retains the latest value for each key"; with `delete,compact`, old segments are dropped by retention while retained segments are compacted. This is consolidation that deliberately discards history for the keyed-state use case — i.e., it is used for state, not for the audit log, which lives elsewhere or uses delete-based retention. *Fact.* (Apache Kafka Docs, `topic_config.html`, generated from source: https://github.com/apache/kafka-site/blob/markdown/content/en/43/generated/topic_config.html, rendered at https://kafka.apache.org/43/documentation/)

**F8 — At the storage-engine level, application code can define what survives a rewrite.** RocksDB `compaction_filter`: "Allows an application to modify/delete a key-value during background compaction." *Fact.* (RocksDB Wiki, "Compaction" → Options: https://github.com/facebook/rocksdb/wiki/Compaction)

*Synthesis (opinion):* the consistent pattern across these systems is (1) the trail is never the thing being consolidated — it is append-only and either kept immutable or hash-chained so removal is detectable; (2) consolidation targets derived state, built as snapshots or latest-value compactions; (3) enforcement correctness during the rewrite is guaranteed either by snapshot pinning (readers see a consistent old view) or by atomic version activation (readers see old-or-new, never torn); (4) auditability of past decisions is preserved by recording the policy revision alongside each decision, so the historical fact can be reconstructed after the live fact was replaced.

## Sources

1. OPA Docs — Bundles. https://www.openpolicyagent.org/docs/latest/management-bundles/ (fetched; quotes above verbatim)
2. OPA Docs — Decision Logs. https://www.openpolicyagent.org/docs/latest/management-decision-logs/ (fetched)
3. Akka Docs — Snapshotting (typed). https://doc.akka.io/libraries/akka-core/current/typed/persistence-snapshot.html (fetched)
4. Akka Docs — Classic Persistence. https://doc.akka.io/docs/akka/current/persistence.html (fetched; snapshot-store architecture)
5. RocksDB Wiki — Snapshot. https://github.com/facebook/rocksdb/wiki/Snapshot (fetched)
6. RocksDB Wiki — Compaction. https://github.com/facebook/rocksdb/wiki/Compaction (fetched)
7. HashiCorp Vault Docs — Audit Devices. https://developer.hashicorp.com/vault/docs/audit (fetched)
8. Sigstore Docs — Rekor overview. https://docs.sigstore.dev/logging/overview/ (fetched)
9. Apache Kafka Docs — Topic configs, generated `topic_config.html`. https://github.com/apache/kafka-site/blob/markdown/content/en/43/generated/topic_config.html (fetched; source of `log.cleanup.policy` text)

Not reached as primary text: Apache Kafka's long-form "Log Compaction" prose section — kafka.apache.org serves it behind a JS redirect and the kafka/kafka-site repos hold only redirect stubs in the paths I checked; I therefore cite only the config reference, which I read directly.

## Verdict

**Established** (primary sources, quoted): audit trails and enforceable state are separate artifacts; snapshots are derived and copy-based, and deleting the event trail is explicitly documented as losing history; compaction drops superseded versions but preserves data visible to live snapshots; OPA policy/data rewrites are atomic erase-and-overwrite activations gated on signature verification, with delta bundles applied as ordered patches and bundle-owned data locked against REST edits; decisions are tagged with the bundle revision for post-hoc replay; Vault refuses service when the audit device cannot write; Rekor trails are append-only with entries never mutated or removed; Kafka compaction retains the latest value per key.

**Uncertain**: whether any production system treats policy *facts* as plain rewriteable data with a compaction filter deciding survival (RocksDB supports the mechanism but I found no primary source of a deployed access-control store using it that way); and the Kafka long-form compaction prose, which I could not read as primary text.

**What would settle it**: (1) the Apache Kafka "Log Compaction" documentation section as primary text (site needs JS-rendered fetch); (2) a primary engineering doc describing an OPA-style store where a compaction filter modifies/removes policy entries during background consolidation (would confirm/deny the F8 mechanism being used for policy data in production); (3) a document stating how OPA/AWS Verified Permissions retain or discard prior bundle revisions for audit — OPA's decision logs record the revision string but do not (in the pages I read) state retention of the historical bundle itself.
