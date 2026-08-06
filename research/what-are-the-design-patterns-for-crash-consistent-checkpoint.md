## Findings

Scope: crash-consistent checkpointing/journaling for long-running agent systems. Claims below are traced to first-party documentation of the reference implementations; anything not traceable is labeled.

### 1. Write-ahead logging (WAL) — the durability primitive

**F1 (fact).** WAL's central invariant: "changes to data files... must be written only after those changes have been logged, that is, after WAL records describing the changes have been flushed to permanent storage." Recovery is roll-forward (REDO): "any changes that have not been applied to the data pages can be redone from the WAL records." Source: PostgreSQL 18, "Write-Ahead Logging (WAL)" §28.3, https://www.postgresql.org/docs/current/wal-intro.html

**F2 (fact).** SQLite inverts the rollback journal: original content stays in the database file, changes append to a separate `-wal` file, and "a COMMIT can happen without ever writing to the original database." Commit becomes durable when the commit record is appended and (per `synchronous` setting) synced. Source: SQLite, "Write-Ahead Logging", https://www.sqlite.org/wal.html

**F3 (fact).** MongoDB/WiredTiger is the same split: checkpoints give "a consistent view of data on disk and allow MongoDB to recover from the last checkpoint," and "if MongoDB exits unexpectedly in between checkpoints, journaling is required to recover information that occurred after the last checkpoint." The recovery procedure is explicit: find last checkpoint ID in data files, find matching record in journal, "apply the operations in the journal files since the last checkpoint." Source: MongoDB Manual, "Journaling", https://www.mongodb.com/docs/manual/core/journaling/

**F4 (fact).** The journal is buffered; durability is a knob, not an on/off. WiredTiger buffers records in memory (up to 128 kB) and syncs on `j:true`, on the 100 ms commit interval, and when a journal file is cut (~100 MB). "In between write operations, while the journal records remain in the WiredTiger buffers, updates can be lost following a hard shutdown." So: fsync timing, not just append, defines the recovery point. Source: MongoDB Manual, "Journaling", https://www.mongodb.com/docs/manual/core/journaling/

**F5 (fact).** RocksDB's contract: "In the default configuration, RocksDB guarantees process crash consistency by flushing the WAL after every user write." A single WAL spans all column families; a WAL is only deleted once all column families have flushed "beyond the largest sequence number contained in the WAL." Recovery replays the WAL to rebuild the in-memory memtable. Source: RocksDB Wiki, "Write Ahead Log (WAL)", https://github.com/facebook/rocksdb/wiki/Write-Ahead-Log-(WAL)

**F6 (fact).** WAL truncation safety is handled by design: LevelDB's log format splits records into 32 KB blocks with per-record CRC32C checksums and FULL/FIRST/MIDDLE/LAST framing, so "If there is a corruption, skip to the next block" — recovery can resync at block boundaries. This is the canonical physical format RocksDB inherited. Source: LevelDB repo, `doc/log_format.md`, https://github.com/google/leveldb/blob/main/doc/log_format.md

**F7 (fact/evidence).** Even mature WAL implementations have had crash-consistency bugs. SQLite's "WAL-reset bug" (present 3.7.0–3.51.2, fixed 3.51.3) is a data race where a second checkpoint can wrongly mark part of a reset WAL as already checkpointed, corrupting the database. The fix required instrumentation to reproduce. This is concrete failure-recovery evidence that WAL correctness is subtle and version-dependent. Source: SQLite, "Write-Ahead Logging", §11 "The WAL-Reset Bug", https://www.sqlite.org/wal.html

### 2. Checkpointing / snapshot-restore — bounding the log

**F8 (fact).** Checkpoint is the mechanism that converts a WAL into a bounded recovery window. SQLite: moving WAL content back into the database "is called a checkpoint," run automatically at 1000 pages, and it "requires sync operations... The WAL must be synced to persistent storage prior to moving content from the WAL into the database and the database file must be synced prior to resetting the WAL." Source: SQLite, "Write-Ahead Logging", https://www.sqlite.org/wal.html

**F9 (fact).** WiredTiger creates checkpoints every 60 seconds; "the previous checkpoint is still valid" while a new one is written, so recovery returns to the last valid checkpoint even if the process dies mid-checkpoint; the new checkpoint becomes permanent only when "WiredTiger's metadata table is atomically updated to reference the new checkpoint." Source: MongoDB Manual, "WiredTiger Storage Engine" → "Snapshots and Checkpoints", https://www.mongodb.com/docs/manual/core/wiredtiger/

**F10 (fact).** Snapshot-restore as a standalone recover point: RocksDB's Checkpoint API "creates a consistent snapshot of a given RocksDB database in the specified directory," hard-linking SST files (same filesystem), copying MANIFEST/CURRENT, and copying log files "for the period covering the start and end of the checkpoint, in order to provide a consistent snapshot across column families." Source: RocksDB Wiki, "Checkpoints", https://github.com/facebook/rocksdb/wiki/Checkpoints

**F11 (fact).** Raft makes snapshotting load-bearing for catch-up: etcd's `--snapshot-count` "configures the number of applied Raft entries to hold in-memory before compaction. When... reaches, server first persists snapshot data onto disk, and then truncates old entries. When a slow follower requests logs before a compacted index, leader sends the snapshot forcing the follower to overwrite its state." Default 100,000 since v3.2. Source: etcd v3.5/3.8 "Maintenance", https://etcd.io/docs/v3.5/op-guide/maintenance/

**F12 (fact).** Compaction discards history and changes the failure model: "Compacting the keyspace history drops all information about keys superseded prior to a given keyspace revision" and "Revisions prior to the compaction revision become inaccessible." After compaction, only a periodic snapshot restores the keyspace. Source: etcd "Maintenance", https://etcd.io/docs/v3.5/op-guide/maintenance/

**F13 (fact).** etcd recovery is snapshot-first: `etcdctl snapshot save` (or copying `member/snap/db`), then `etcdutl snapshot restore` builds fresh data directories. "A cluster restore... creates new etcd data directories; all members should restore using the same snapshot. Restoring overwrites some snapshot metadata (specifically, the member ID and cluster ID)... the restore must start a new logical cluster." Restoring from the raw `member/snap/db` file "might lose data that has not been written yet, but is included in the wal (write-ahead-log) folder." Source: etcd "Disaster recovery", https://etcd.io/docs/v3.5/op-guide/recovery/

**F14 (fact).** The snapshot/WAL split is explicit at the file level in etcd: `member/snap/db` (bbolt) stores applied data and a `consistent_index` marker of "the offset of the last applied WAL entry"; `member/wal/*.wal` is "Raft's Write Ahead Logs, containing recent transactions accepted by Raft, periodic snapshots or CRC records"; "the entire Raft state of the member can be recovered from the WAL log alone." The WAL is append-only and "entries with index > HardState.commit are subject to change," so recovery only trusts entries up to the committed hard state. Source: etcd "Persistent storage files", https://etcd.io/docs/v3.5/learning/persistent-storage-files/

**F15 (fact).** Hybrid snapshot+WAL-replay is the standard disaster-recovery recipe beyond crash recovery. PostgreSQL continuous archiving: "we can combine a file-system-level backup with backup of the WAL files. If recovery is needed, we restore the file system backup and then replay from the backed-up WAL files." The base backup "doesn't have to be an instantaneous snapshot" — inconsistency is repaired by replay — and stopping replay early gives point-in-time recovery. Source: PostgreSQL 18, "Continuous Archiving and Point-in-Time Recovery", https://www.postgresql.org/docs/current/continuous-archiving.html

**F16 (estimate).** Snapshot frequency bounds recovery time (replay length) and storage cost (archived WAL/old snapshots). Every source above states the trade-off but none gives universal numbers; etcd example snapshot is "2.1 MB" for 7 keys / rev 10 (https://etcd.io/docs/v3.5/op-guide/maintenance/), which is illustrative, not a rule. Labeled estimate: choose checkpoint interval per write volume and RTO/RPO.

### 3. Event sourcing — the log as system of record

**F17 (fact).** Fowler defines Event Sourcing as: "all changes to application state are stored as a sequence of events" enabling "Complete Rebuild: We can discard the application state completely and rebuild it by re-running the events from the event log on an empty application," plus temporal query and event replay. He explicitly pairs it with snapshots for performance: "started at the beginning of the day from an overnight snapshot... Should it crash it replays the events from the overnight store." Source: M. Fowler, "Event Sourcing" (12 Dec 2005), https://martinfowler.com/eaaDev/EventSourcing.html

**F18 (fact).** Fowler flags the operational hazard specific to event sourcing in agents: replaying events re-runs side effects, so external systems must be gated — "you'll need to wrap any external systems with a Gateway" that can detect replay mode and suppress outbound calls. This is the same problem durable-execution platforms solve with idempotency. Source: Fowler, "Event Sourcing" → "External Updates", https://martinfowler.com/eaaDev/EventSourcing.html

**F19 (fact).** Temporal's durable execution is event-sourcing in disguise: "a complete and durable log of everything that has happened in the lifecycle of a Workflow Execution." The workflow doesn't perform actions directly; it sends Commands which "are then mapped to Events which are persisted in case of failure. For example, if the Worker crashes, the Worker uses the Event History to replay the code and recreate the state of the Workflow Execution to what it was immediately before the crash." Source: Temporal docs, "Event History", https://docs.temporal.io/encyclopedia/event-history

**F20 (fact).** Temporal: "Durable Execution... refers to the ability of a Workflow Execution to maintain its state and progress even in the face of failures, crashes, or server outages. This is achieved through Temporal's use of an Event History, which records the state of a Workflow Execution at each step. If a failure occurs, the Workflow Execution can resume from the last recorded event." Source: Temporal docs, "What is Temporal?", https://docs.temporal.io/temporal.md

### 4. Applied patterns in agent systems

**F21 (fact).** LangGraph uses checkpoint/snapshot per execution step, not a WAL: "A checkpointer saves a snapshot of graph state at each super-step, organized into threads." Fault tolerance is explicit: "if one or more nodes fail at a given superstep, you can restart your graph from the last successful step." Source: LangGraph docs, "Checkpointers", https://docs.langchain.com/oss/python/langgraph/checkpointers/

**F22 (fact).** LangGraph's durability is tunable: `"exit"` persists only when execution exits (mid-run crash loses state), `"async"` persists while the next step executes ("small risk... does not write checkpoints if the process crashes"), `"sync"` "persists changes synchronously before the next step starts" — i.e. an explicit trade between durability and throughput, mirroring WAL `synchronous` settings. Source: LangGraph docs, "Checkpointers → Durability modes", https://docs.langchain.com/oss/python/langgraph/checkpointers/

**F23 (fact).** LangGraph separates full snapshots from finer-grained writes: per-node "pending writes" are written durably within a super-step so that on resume "you don't re-run the successful nodes," while time-travel/replay resumes only at full super-step checkpoints. This is snapshot-restore + per-task delta, the classic pattern. Source: LangGraph docs, "Checkpointers → Pending writes / Super-steps", https://docs.langchain.com/oss/python/langgraph/checkpointers/

**F24 (fact).** LangGraph documents an unbounded-growth failure mode: "Over long conversations, checkpoints accumulate. This can increase latency and storage costs" — fix is pruning or retention policy (equivalent to compaction). `DeltaChannel` stores deltas instead of full values to bound checkpoint size. Source: LangGraph docs, "Persistence → Troubleshooting" and "Checkpointers → DeltaChannel", https://docs.langchain.com/oss/python/langgraph/persistence/ , https://docs.langchain.com/oss/python/langgraph/checkpointers/

**F25 (fact).** AWS Step Functions draws the exact-once vs at-least-once line that matters for agents: Standard workflows are "durable... exactly-once," "Execution state internally persists between state transitions," and full execution history is retrievable; Express workflows are at-least-once (async) / at-most-once (sync) and "Execution state doesn't persist between state transitions." Retry-on-at-least-once requires idempotent actions. Source: AWS Step Functions Developer Guide, "Choosing workflow type in Step Functions", https://docs.aws.amazon.com/step-functions/latest/dg/concepts-standard-vs-express.html

### 5. Comparative synthesis

**F26 (fact).** All three approaches converge on one structure: an append-only, checksummed, order-preserving log (WAL or event history) whose replay is bounded by periodic snapshots/checkpoints (state images). Evidence: F1+F2+F8 (WAL+checkpoint), F11+F14+F15 (Raft snapshot + WAL), F17+F19 (event log + snapshot).

**F27 (opinion, sourced inference).** The selector for agents is which failure you must survive and at what cost:
- **WAL + checkpoint (F1–F16):** strongest crash consistency, minimal replay, but requires a storage engine contract (fsync ordering, page-level recovery); overkill if the agent state is small.
- **Snapshot-restore (F9–F10, F13, F21):** simplest, bounded recovery window, but loses everything since the last snapshot unless combined with a log; RPO = snapshot interval (60 s in WiredTiger default, F9).
- **Event sourcing / durable execution (F17–F20):** enables exact resume, audit, time travel, and human-in-the-loop review (F19, F23) at the cost of replay correctness hazards — side-effect duplication (F18), unbounded history requiring compaction/pruning (F12, F24), and schema/version evolution of events.

**F28 (fact).** A claim that a failure was "recovered" is only as good as its verifier: this repo's own discipline (AGENTS.md) requires `run verify` to actually execute the declared `verify_command` before an outcome is trusted — no source in this report is a substitute for executing a recovery path on your own system.

## Sources

1. SQLite — "Write-Ahead Logging" (incl. §11 WAL-reset bug). https://www.sqlite.org/wal.html
2. PostgreSQL 18 — "Write-Ahead Logging (WAL)" §28.3. https://www.postgresql.org/docs/current/wal-intro.html
3. PostgreSQL 18 — "Continuous Archiving and Point-in-Time Recovery (PITR)" §25.3. https://www.postgresql.org/docs/current/continuous-archiving.html
4. MongoDB Manual — "Journaling" (WiredTiger). https://www.mongodb.com/docs/manual/core/journaling/
5. MongoDB Manual — "WiredTiger Storage Engine" (Snapshots and Checkpoints). https://www.mongodb.com/docs/manual/core/wiredtiger/
6. RocksDB Wiki — "Write Ahead Log (WAL)". https://github.com/facebook/rocksdb/wiki/Write-Ahead-Log-(WAL)
7. RocksDB Wiki — "Checkpoints". https://github.com/facebook/rocksdb/wiki/Checkpoints
8. LevelDB repo — `doc/log_format.md`. https://github.com/google/leveldb/blob/main/doc/log_format.md
9. etcd v3.5 — "Maintenance" (compaction, defrag, snapshot, `--snapshot-count`). https://etcd.io/docs/v3.5/op-guide/maintenance/ (also served at /v3.8/)
10. etcd v3.5 — "Persistent storage files" (WAL, snap/db, recovery semantics). https://etcd.io/docs/v3.5/learning/persistent-storage-files/
11. etcd v3.5 — "Disaster recovery" (snapshot save/restore, quorum loss, force-new-cluster). https://etcd.io/docs/v3.5/op-guide/recovery/
12. Martin Fowler — "Event Sourcing" (12 Dec 2005). https://martinfowler.com/eaaDev/EventSourcing.html
13. Temporal Docs — "Event History". https://docs.temporal.io/encyclopedia/event-history
14. Temporal Docs — "What is Temporal?" (Durable Execution). https://docs.temporal.io/temporal.md
15. AWS Step Functions Developer Guide — "Choosing workflow type in Step Functions" (Standard vs Express, execution guarantees). https://docs.aws.amazon.com/step-functions/latest/dg/concepts-standard-vs-express.html
16. LangGraph Docs — "Persistence". https://docs.langchain.com/oss/python/langgraph/persistence/
17. LangGraph Docs — "Checkpointers" (durability modes, pending writes, DeltaChannel, pruning). https://docs.langchain.com/oss/python/langgraph/checkpointers/

Note: none of the fetched sources were PDFs; all were readable HTML/markdown. No fabrication needed.

## Verdict

**Established (high confidence):** The three patterns are one family, not alternatives. Every credible system (PostgreSQL, SQLite, WiredTiger/MongoDB, RocksDB, etcd/Raft, Temporal, LangGraph, AWS Step Functions) combines an append-only, checksummed, order-preserving log with periodic state snapshots that bound replay. Crash consistency comes from flush-before-publish ordering (F1, F2, F8); recovery truncates at the last committed/durable point (F14); compaction/pruning bounds history (F12, F24); event sourcing additionally enables exact-resume, replay, and audit at the cost of side-effect re-execution (F17–F20). The exact-once vs at-least-once distinction (F25) and tunable durability (F4, F22) are first-party documented knobs, not folklore.

**Uncertain / not established from these sources:** No primary source quantifies end-to-end failure-recovery success rates, RTO/RPO for arbitrary agent workloads, or a cost model for choosing among the three. The WAL-reset bug (F7) shows latent-corruption risk exists in even the most battle-tested implementation; nothing in these docs proves any engine's recovery is correct under every crash point. Comparative evidence (F27) is my inference from the cited mechanics, not a measured study — treat as opinion.

**What would settle it:** (a) a crash-injection fault-injection harness (kill/reboot/power-cut at randomized points) run against each candidate with asserted post-crash invariants; (b) measured RPO = time between last durable checkpoint and crash for each configuration; (c) replay-cost curves (recovery time vs log length/snapshot interval) for agent-sized state; and (d) — per this repo's own gate — a verifier that actually executes a restored snapshot plus replay and asserts the agent's state equals a reference trajectory.
