# ADR-0012 — worker sandbox: Landlock write-containment

Status: accepted (2026-08-04)

## Context

Hardening audit (docs/HARDENING-AUDIT.md, P0-4) found the codex/hitl
worker runs arbitrary commands in a workdir behind only a procedural
trust boundary (AGENTS.md). Two candidate primitives:

- **bubblewrap**: full sandbox (mounts, network namespaces, uid).
  Rejected — it is an external binary dependency that breaks the repo's
  single-binary std-only identity, requires user namespaces or a setuid
  helper (often unavailable on CI/servers), and over-delivers for the
  actual threat.
- **Landlock** (Linux kernel 5.13+, 2021): fine-grained filesystem
  access control (read/write/execute per path), no root, no user
  namespaces, no external binary. Matches the real threat: the binding
  risk of a coding worker is writing outside its workdir (`rm -rf`,
  clobbering `~/.ssh`, `/etc`, other repos), not network egress.

## Decision

The worker runs under **Landlock write-containment** (Linux only):

- **Policy (default):** the whole tree is readable and executable
  (`read`+`execute` on `/` — a coding agent must inspect context and run
  tools), but **write/create is confined** to the workdir, codex's own
  state dir (`$HOME/.codex`), and any explicit `--allow-write` dirs.
- Codex's own `-s workspace-write` sandbox remains the first line;
  kernel-level Landlock is defense-in-depth for the trust boundary.
- Applied via a dedicated `mini-agi exec-sandbox <allow-dirs> -- <cmd>`
  wrapper process (self-spawned): the wrapper applies the ruleset to
  itself, then spawns the worker (which inherits the restrictions), then
  waits and forwards the exit code. No `pre_exec`/unsafe is used; the
  workspace `unsafe_code = "forbid"` stays intact.
- The `landlock` crate (pure safe syscalls via `linux-raw-sys`) is a
  dependency of the **binary crate** (`crates/mini-agi`), NOT the kernel
  crate — `mini-agi-core` remains std-only + the pinned four deps.
- **Graceful degradation:** if the kernel reports no Landlock ABI
  (< 5.13) or the syscalls fail, the wrapper prints a warning and runs
  the worker unsandboxed — a missing sandbox is reported, never silent.
- Non-Linux targets compile the command as a documented no-op (the
  worker runs without the sandbox; the repo is Linux-targeted).
- `--no-sandbox` on `mini-agi codex` is an explicit escape hatch,
  mirrored by a warning.

## Consequences

- `mini-agi codex` routes the worker through the wrapper on Linux.
- The pinned-dependencies note in AGENTS.md gains the `landlock` crate
  for the binary crate only.
- A regression test verifies: writes inside the workdir succeed, writes
  outside fail (skipped when the host kernel lacks Landlock).
