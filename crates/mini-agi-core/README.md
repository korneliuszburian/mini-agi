# mini-agi-core

The kernel of mini-agi: enforcement-bound memory, evaluation, skills
registry and checkpoint journal — as a dependency-free-in-spirit library.

Behavioral contract: ports of the PoC (`v1-spec-reference`) — identical
hashing (`sha256[:16]`), identical file layout, identical semantics.

## Modules

- `hash` — fact ids and source hashes (sha256 prefix, 16 hex chars).
- `store` — append-only canonical entry store (per-day sequence files).
- `memory`, `eval`, `skills`, `journal` — phases 0-3, landed incrementally.

## Testing

```sh
cargo test -p mini-agi-core
```
