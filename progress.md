# progress — TICKET: add the --max-idle <seconds> flag to `mini-agi loop run` (AFK supervisor S6, self-hosting proof). Today the worker idle-timeout (a silent worker killed as STUCK after max_idle_seconds) can only be set via the repo config (.miniagi.json / MINIAGI_MAX_IDLE_SECONDS). The CLI flag is the missing escape hatch.

Contract:
- LoopRunArgs (crates/mini-agi/src/main.rs) gains --max-idle <seconds> (Option<u64>).
- cmd_loop_run passes it into supervisor::SupervisorArgs as max_idle.
- SupervisorArgs (crates/mini-agi/src/supervisor.rs) gains max_idle: Option<u64>; supervisor::run passes it into worker::IterationInput as max_idle.
- IterationInput (crates/mini-agi/src/worker.rs) gains max_idle: Option<u64>; run_verified_iteration resolves the idle cap as input.max_idle.or(Config::load(workdir).max_idle_seconds) — the FLAG WINS over the config, and the run_capped_idle call receives it.
- clap help text documents the flag.
Do NOT run checkpoint.sh and do NOT commit: the supervised verified-iteration loop is the gate. Run cargo fmt after the change. The verifier (build + `loop run --help` shows --max-idle + full test suite green) is run by the kernel, not by you.

- 2026-08-05T10:21:25Z attempt 1 started
- 2026-08-05T10:22:37Z attempt 1: VERIFIER PASSED
