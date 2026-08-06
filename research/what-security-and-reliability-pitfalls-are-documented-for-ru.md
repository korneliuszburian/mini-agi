I'll research this against primary sources. Starting with the authoritative SQLite documentation on concurrency, locking, WAL, and corruption, plus agent-framework docs.
Strong SQLite primary sources gathered. Now fetching the FAQ, appropriate-uses, busy handler, and corruption docs, plus agent-framework primary docs.
Now fetching primary docs from production agent frameworks (LangGraph persistence/durable execution) and the LLM API providers' token-usage/telemetry docs.
LangGraph redirects and Anthropic token-counting retrieved. Now getting LangGraph persistence/durable-execution (following redirects) and OpenAI usage/tracing docs.
curl has no network access here — only webfetch does. Using webfetch on the post-redirect URLs.
The Messages API doc is large — delegating extraction of the usage object to an explore agent while I fetch LangGraph checkpointers and OpenAI telemetry docs.
Good. Now the OpenAI side: usage/tracing primary docs.
The OpenAI Agents SDK has dedicated "Advanced SQLite session" and "Usage" docs — directly on point. Fetching both.
Two more primary pages to confirm concurrency guidance in the OpenAI SDK sessions docs.
One more pass for the sandboxed-worker half of the question — the OpenAI Agents SDK sandbox concepts and memory docs.
Research complete. Here is the deliverable.

---

## Findings

### 1. SQLite's documented concurrency model — single-writer, reader/writer locks

- **Fact.** "Multiple processes can be doing a SELECT at the same time. But only one process can be making changes to the database at any moment in time." SQLite uses reader/writer locks; writers lock the whole database file during their update. On lock conflict the default is `SQLITE_BUSY`, adjustable via a busy handler/timeout. — *"SQLite Frequently Asked Questions" Q5*, https://www.sqlite.org/faq.html; *"Set A Busy Timeout"*, https://www.sqlite.org/c3ref/busy_timeout.html
- **Fact.** "SQLite supports an unlimited number of simultaneous readers, but it will only allow one writer at any instant in time. For many situations, this is not a problem. Writers queue up... no lock lasts for more than a few dozen milliseconds." The explicit guidance: "Many concurrent writers? → choose client/server." — *"Appropriate Uses For SQLite"*, https://www.sqlite.org/whentouse.html
- **Fact.** The five lock states (UNLOCKED/SHARED/RESERVED/PENDING/EXCLUSIVE) and writer-starvation avoidance via PENDING locks are defined for rollback mode. — *"File Locking And Concurrency In SQLite Version 3"*, https://www.sqlite.org/lockingv3.html

### 2. WAL-mode pitfalls for many concurrent processes on one file

- **Fact.** WAL lets readers and writers proceed concurrently, but "since there is only one WAL file, there can only be one writer at a time." — *"Write-Ahead Logging"* §2.2, https://www.sqlite.org/wal.html
- **Fact.** "All processes using a database must be on the same host computer; WAL does not work over a network filesystem," because the wal-index lives in a shared-memory mmap. — *wal.html* §1 (disadvantage 1), §2.2
- **Fact (directly relevant to sandboxed workers).** The wal-index is backed by a file mmapped in the same directory as the database; "processes with a different root directory (changed via chroot) will see different files and hence use different shared memory areas, leading to database corruption." A chroot'd sandbox worker therefore cannot reliably share a WAL-mode store on the same filesystem path. — *wal.html* §7
- **Fact.** Even in WAL mode queries can return `SQLITE_BUSY`: when another connection holds exclusive locking mode, while the last connection is doing close/cleanup, or while the first connection after a crash holds the recovery lock. — *wal.html* §9
- **Fact.** "Checkpoint starvation": if concurrent overlapping readers mean there is always at least one active reader, checkpoints never complete and the WAL "will grow without bound," causing excess disk use and slow queries. — *wal.html* §6
- **Fact.** The WAL-reset bug (corruption) "only affects databases in WAL mode when there are two or more database connections open on the same file, in separate threads or processes, and when those two connections attempt to write or checkpoint at the same instant." Present in all versions 3.7.0–3.51.2, fixed in 3.51.3 (2026-03-13); rare, low probability. — *wal.html* §11; *howtocorrupt.html* §8.1

### 3. Crash recovery: journal-based, automatic, but fragile to external actions

- **Fact.** Rollback mode gives atomic commit via a rollback journal; a "hot journal" left by a crash is automatically rolled back by the next process to open the file. The commit point is journal deletion. — *"Atomic Commit In SQLite"* §3–4, https://www.sqlite.org/atomiccommit.html
- **Fact.** "If an application crash, or an operating-system crash, or even a power failure occurs in the middle of a transaction, the partially written transaction should be automatically rolled back the next time the database file is accessed." SQLite "is not immune" to corruption. — *"How To Corrupt An SQLite Database File"* Overview, https://www.sqlite.org/howtocorrupt.html
- **Fact.** Recovery breaks if the journal is interfered with: "SQLite must see the journal files in order to recover... If the hot journal files are moved, deleted, or renamed after a crash or power failure, then automatic recovery will not work and the database may go corrupt." — *howtocorrupt.html* §1.3–1.4
- **Fact.** `PRAGMA synchronous=OFF` can corrupt on OS crash/power loss; "SQLite should always be run with its default synchronous setting of FULL." WAL is more forgiving of out-of-order writes than rollback mode (only checkpoint needs a correct sync). — *howtocorrupt.html* §3.2, §3.1

### 4. Corruption and security pitfalls when many workers touch one file

- **Fact (security).** "SQLite database files are ordinary disk files. That means that any process can open the file and overwrite it with garbage. There is nothing that the SQLite library can do to defend against this." There is no built-in access control — protection is whatever the OS filesystem permissions provide. — *howtocorrupt.html* §1
- **Fact.** POSIX advisory locking is buggy/unreliable on NFS; "two or more clients might try to modify the same part of the same database at the same time, resulting in corruption." — *whentouse.html*; *howtocorrupt.html* §2.1; *faq.html* Q5
- **Fact.** A `close()` on *any* file descriptor for the file in one thread cancels the POSIX advisory locks held by all threads of the process — "This can lead to two or more threads or processes trying to write to the database at the same time, resulting in database corruption." Fixed defensively only partially in 3.51.0. — *howtocorrupt.html* §2.2
- **Fact.** Linking two copies of SQLite into one application, using two different locking protocols, unlinking/renaming the file while open, multiple hard/symlinks to the file — all are documented corruption sources. — *howtocorrupt.html* §2.3–2.6
- **Fact (directly relevant to forked workers).** "Do not open an SQLite database connection, then fork(), then try to use that database connection in the child process. All kinds of locking problems will result and you can easily end up with a corrupt database." Also: "Under Unix, you should not carry an open SQLite database across a fork() system call into the child process." — *howtocorrupt.html* §2.7; *faq.html* Q6
- **Fact.** A read-only mount of a WAL-mode database fails unless the `-shm`/`-wal` files already exist and are readable, or the directory is writable, or the database is opened `immutable` — relevant when a sandbox mounts the store read-only. — *wal.html* §5

### 5. How production agent systems store sessions: they avoid sharing one SQLite file across many workers

- **Fact.** OpenAI Agents SDK session guide assigns: `SQLiteSession` → "Local development and simple apps"; `AsyncSQLiteSession` → async dev; `RedisSession` → "shared session memory across multiple workers or services"; `SQLAlchemySession` → "production apps with existing databases" (PostgreSQL, MySQL, SQLite, etc.); `MongoDBSession` → "multi-process, horizontally-scalable session storage"; `DaprSession` → "production cloud-native deployments." The documented production path for concurrent workers is a shared-server DB (or a single long-running app server that serializes access), not a shared SQLite file. — *"Sessions" overview*, https://openai.github.io/openai-agents-python/sessions/
- **Fact.** "SQLAlchemy sessions use SQLAlchemy to provide a production-ready session implementation, allowing you to use any database supported by SQLAlchemy (PostgreSQL, MySQL, SQLite, etc.)." — *"SQLAlchemy session"*, https://openai.github.io/openai-agents-python/sessions/sqlalchemy_session/
- **Fact.** Concurrency is handled by identifying sessions by `session_id` (a row key), not by file ownership; multiple sessions share one `conversations.db`, and different agents can share the same session. — *"Sessions" overview*; *"Advanced SQLite session"* schema (`agent_sessions`, `agent_messages` tables keyed by `session_id`)
- **Fact.** LangGraph makes the same split explicitly: `SqliteSaver` is "Ideal for experimentation and local workflows"; `PostgresSaver` (used in LangSmith) is "Ideal for using in production." — *"Checkpointers"*, https://docs.langchain.com/oss/python/langgraph/checkpointers/
- **Estimate/opinion.** Neither vendor documents "many sandboxed workers sharing a single SQLite file" as a supported pattern; their documented answers are (a) serialize through one server, or (b) move to Redis/Postgres/MongoDB/Dapr. The single-file SQLite store is documented as the local/dev topology.

### 6. Crash recovery in production agent systems

- **Fact.** LangGraph writes a checkpoint at each "super-step" boundary and persists per-node writes to a `checkpoint_writes` table so that, if one node fails, successful nodes' writes are durable ("pending writes") and are not re-run on resume. Resume restarts "from the last successful step." — *"Persistence"* and *"Checkpointers"*, https://docs.langchain.com/oss/python/langgraph/persistence/, https://docs.langchain.com/oss/python/langgraph/checkpointers/
- **Fact.** LangGraph durability modes trade recovery vs. performance: `"exit"` persists only on exit (no mid-execution recovery after a process crash), `"async"` persists during the next step (small loss window on crash), `"sync"` persists every checkpoint before the next step (high durability, performance cost). — *"Checkpointers"*
- **Fact.** OpenAI sandbox agents recover sandbox work via serialized `session_state` and workspace `snapshot`s (`SnapshotSpec`); in the SDK-owned lifecycle the runner "persists snapshot-backed workspace state" on shutdown, and `stop()` explicitly checkpoints the live workspace mid-lifecycle. — *"Sandbox agents: Concepts"*, https://openai.github.io/openai-agents-python/sandbox/guide/
- **Fact.** At the SQLite layer, crash recovery is the automatic hot-journal / WAL replay described in §3 — the framework-layer checkpoint/resume sits on top of that file-level guarantee.

### 7. Per-run cost telemetry

- **Fact.** OpenAI Agents SDK "automatically tracks token usage for every run": `requests`, `input_tokens`, `output_tokens`, `total_tokens`, per-request entries, and `input_tokens_details.cached_tokens` / `output_tokens_details.reasoning_tokens`; aggregated across tool calls and handoffs. Sessions note that usage is per-run and independent, and that "previous messages may be re-fed as input to each run, which affects the input token count in subsequent turns." — *"Usage"*, https://openai.github.io/openai-agents-python/usage/
- **Fact.** Persisting cost telemetry into the store is application-driven, not automatic: `AdvancedSQLiteSession` "provides detailed usage analytics by storing token usage data per conversation turn. This is entirely dependent on the `store_run_usage` method being called after each agent run," into a `turn_usage` table (`requests`, `input_tokens`, `output_tokens`, `total_tokens`, plus JSON details columns), keyed by `(session_id, branch_id, user_turn_number)`. — *"Advanced SQLite session"*, https://openai.github.io/openai-agents-python/sessions/advanced_sqlite_session/
- **Fact.** Anthropic returns a `usage` object on every Messages response described as "Billing and rate-limit usage": `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`, with "Total input tokens in a request is the summation of input_tokens, cache_creation_input_tokens, and cache_read_input_tokens," and `output_tokens` being "the inclusive, authoritative total used for billing." Token counts "will not match one-to-one with the exact visible content." — *"Messages API reference"*, https://platform.claude.com/docs/en/api/messages
- **Fact.** Anthropic token *counting* (`/v1/messages/count_tokens`) is explicitly an estimate ("The token count is an estimate") and free; "You are not billed for system-added tokens." Also a documented telemetry pitfall: models since Opus 4.7 use a newer tokenizer producing "approximately 30 percent more tokens" for the same text, so token counts are not portable across models — "don't reuse token counts measured on a model before Claude Opus 4.7 to estimate costs." — *"Token counting"*, https://docs.anthropic.com/en/docs/build-with-claude/token-counting
- **Fact.** SDK tracing (OpenAI Agents SDK) records generation/function/tool spans with usage by default; in long-running workers the batch exporter flushes every few seconds, so for per-job delivery guarantees "call `flush_traces()` after the trace context exits." — *"Tracing"*, https://openai.github.io/openai-agents-python/tracing/

### 8. Security handling for sandboxed workers and shared state

- **Fact.** The OpenAI SDK architecture keeps state out of the sandbox by design: "The outer runtime still owns approvals, tracing, handoffs, and resume bookkeeping. The sandbox session owns commands, file changes, and environment isolation." The conversational `Session` is explicitly distinct from a sandbox session. — *"Sandbox agents: Concepts"*
- **Fact.** Sandbox manifests treat path grants as trusted config: "Treat manifests that contain `extra_path_grants` as trusted configuration. Do not load grants from model output or other untrusted payloads unless your application has already approved those host paths," and `read_only=True` is recommended for host data the sandbox should not modify. — *"Sandbox agents: Concepts"*
- **Fact.** Both frameworks document encrypting persisted session state at rest: LangGraph `EncryptedSerializer` for checkpoints, OpenAI `EncryptedSession` wrapper ("Encryption + TTL on top of another session"). — *"Checkpointers"*, *"Sessions" overview*

---

## Sources

1. SQLite WAL — https://www.sqlite.org/wal.html
2. SQLite File Locking and Concurrency v3 — https://www.sqlite.org/lockingv3.html
3. SQLite Atomic Commit — https://www.sqlite.org/atomiccommit.html
4. SQLite FAQ — https://www.sqlite.org/faq.html
5. SQLite Appropriate Uses — https://www.sqlite.org/whentouse.html
6. SQLite `sqlite3_busy_timeout` — https://www.sqlite.org/c3ref/busy_timeout.html
7. SQLite How To Corrupt An SQLite Database File — https://www.sqlite.org/howtocorrupt.html
8. OpenAI Agents SDK: Sessions overview — https://openai.github.io/openai-agents-python/sessions/
9. OpenAI Agents SDK: SQLAlchemy session — https://openai.github.io/openai-agents-python/sessions/sqlalchemy_session/
10. OpenAI Agents SDK: Advanced SQLite session — https://openai.github.io/openai-agents-python/sessions/advanced_sqlite_session/
11. OpenAI Agents SDK: Usage — https://openai.github.io/openai-agents-python/usage/
12. OpenAI Agents SDK: Tracing — https://openai.github.io/openai-agents-python/tracing/
13. OpenAI Agents SDK: Sandbox agents concepts — https://openai.github.io/openai-agents-python/sandbox/guide/
14. OpenAI Agents SDK: Sandbox agent memory — https://openai.github.io/openai-agents-python/sandbox/memory/
15. Anthropic: Token counting — https://docs.anthropic.com/en/docs/build-with-claude/token-counting
16. Anthropic: Messages API reference — https://platform.claude.com/docs/en/api/messages
17. LangGraph: Persistence — https://docs.langchain.com/oss/python/langgraph/persistence/
18. LangGraph: Checkpointers — https://docs.langchain.com/oss/python/langgraph/checkpointers/

## Verdict

**Established:** SQLite's single-writer model, `SQLITE_BUSY` behavior, WAL's one-writer/many-reader semantics, and its hard constraints (same host only; shared-memory wal-index; `SQLITE_BUSY` edge cases; checkpoint starvation; WAL-reset bug in ≤3.51.2) are primary-source documented. Crash recovery is automatic via hot-journal/WAL replay but is defeated by deleting/renaming journals, `synchronous=OFF`, broken NFS locks, `fork()`-inherited connections, or out-of-band `close()` on the file. Security-wise, the store is a plain file with no access control; a sandbox worker granted write access can corrupt it, and chroot'd sandboxes break WAL shared memory.

**Uncertain/opinion:** Whether a single SQLite file is a viable store for *many concurrent sandboxed workers* is not documented by any vendor as a blessed topology. Production frameworks (OpenAI Agents SDK, LangGraph) instead document sharing via Redis/Postgres/MongoDB/Dapr or a single server, with SQLite reserved for dev/local. Per-run cost telemetry is not automatic at the storage layer — frameworks return usage per run (OpenAI `Usage`, Anthropic `usage` object) and require an explicit store step (`store_run_usage`); Anthropic token counts are estimates and tokenizer changes (~30% on newer models) make counts non-portable across models.

**What would settle it:** (1) An incident report or vendor doc from a system actually running N sandboxed workers against one SQLite file (none surfaced from primary sources reached); (2) reproducible stress evidence for the WAL-reset bug's real-world rate, which SQLite itself labels "less than or equal to the expected occurrence rate of SSD malfunctions"; (3) a first-party statement on token-count billing precision beyond the "not match one-to-one" and "estimate" wording already cited.
