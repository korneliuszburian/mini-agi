# run report — TICKET: add the --max-idle <seconds> flag to `mini-agi loop run` (AFK supervisor S6, self-hosting proof). Today the worker idle-timeout (a silent worker killed as STUCK after max_idle_seconds) can only be set via the repo config (.miniagi.json / MINIAGI_MAX_IDLE_SECONDS). The CLI flag is the missing escape hatch.

Contract:
- LoopRunArgs (crates/mini-agi/src/main.rs) gains --max-idle <seconds> (Option<u64>).
- cmd_loop_run passes it into supervisor::SupervisorArgs as max_idle.
- SupervisorArgs (crates/mini-agi/src/supervisor.rs) gains max_idle: Option<u64>; supervisor::run passes it into worker::IterationInput as max_idle.
- IterationInput (crates/mini-agi/src/worker.rs) gains max_idle: Option<u64>; run_verified_iteration resolves the idle cap as input.max_idle.or(Config::load(workdir).max_idle_seconds) — the FLAG WINS over the config, and the run_capped_idle call receives it.
- clap help text documents the flag.
Do NOT run checkpoint.sh and do NOT commit: the supervised verified-iteration loop is the gate. Run cargo fmt after the change. The verifier (build + `loop run --help` shows --max-idle + full test suite green) is run by the kernel, not by you.
- goal: TICKET: add the --max-idle <seconds> flag to `mini-agi loop run` (AFK supervisor S6, self-hosting proof). Today the worker idle-timeout (a silent worker killed as STUCK after max_idle_seconds) can only be set via the repo config (.miniagi.json / MINIAGI_MAX_IDLE_SECONDS). The CLI flag is the missing escape hatch.

Contract:
- LoopRunArgs (crates/mini-agi/src/main.rs) gains --max-idle <seconds> (Option<u64>).
- cmd_loop_run passes it into supervisor::SupervisorArgs as max_idle.
- SupervisorArgs (crates/mini-agi/src/supervisor.rs) gains max_idle: Option<u64>; supervisor::run passes it into worker::IterationInput as max_idle.
- IterationInput (crates/mini-agi/src/worker.rs) gains max_idle: Option<u64>; run_verified_iteration resolves the idle cap as input.max_idle.or(Config::load(workdir).max_idle_seconds) — the FLAG WINS over the config, and the run_capped_idle call receives it.
- clap help text documents the flag.
Do NOT run checkpoint.sh and do NOT commit: the supervised verified-iteration loop is the gate. Run cargo fmt after the change. The verifier (build + `loop run --help` shows --max-idle + full test suite green) is run by the kernel, not by you.
- attempts: 1
- verifier: PASSED
- total wall: 72s | ~22226 tokens (transcript bytes / 4)
- run.json: /mnt/storage/coding/krn/active/mini-agi/run.json

## attempt chain
- {"attempt":1,"failed_cases":[],"passed":true}
