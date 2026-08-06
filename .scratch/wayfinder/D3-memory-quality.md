# D3 — Memory quality: merge/supersede + retrieval budget + directed consolidation

Status: OPEN (F-012 decided the approach: fact merge/supersede + selective
retrieval + directed consolidation with preservation list; design + build
pending)
Date: 2026-08-06. Source: MEMORY-RESEARCH.md (TRACK 1); F-012.

## Context
Current memory: append-only canonical facts (sha256[:16]), derived views
regenerated, consolidation manual. Known gaps from research:
1. Facts accumulate duplicates/contradictions — no merge/supersede.
2. Retrieval is full-brief — no token-budgeted selective retrieval
   (HippoRAG-style graph recall + budget).
3. Consolidation is undirected — everything competes for the same weight;
   no preservation list to protect load-bearing facts.
4. Memory is not consulted with urgency — the kernel dogfoods it, but
   there is no recency/importance fusion.

## Options
- (a) In-kernel operations (recommended): `mem merge|supersede` with
  provenance-by-construction (the superseding fact cites the superseded id),
  a dedup gate in `mem verify`/CI, token-budgeted retrieval in the loop's
  context assembly, directed consolidation with a preservation list
  (load-bearing facts exempt from decay/merge).
- (b) Derived-view tricks only: hide stale facts in views without touching
  canonical — leaves the canonical store polluted (conflict risk at signoff).
- (c) External index (vector store): out of scope for a std-only kernel
  (ADR-0012); a later ADR if retrieval quality demands it.

## Evidence
- E-mem/D-MEM: cheap extractors + supersede beats blind append on recall.
- MemoryBank/Karpathy: promote/gravity with nothing deleted → soft-delete
  lineage, never hard delete (matches our append-only rule).
- HippoRAG: selective retrieval beats full-context on budgeted recall.
- Our own dogfood: the 2026-08-06 consolidation hit a real char-boundary
  panic and produced 15 facts — the pipeline works, the hygiene is missing.

## Decision
OPEN. Recommended: (a) — kernel ops with provenance chains, dedup gate,
budgeted retrieval, preservation list.

## Effort
M-L. New memory ops + gate + retrieval rework + preservation list model.

## Dependencies
D2 (the dream-loop promotes INTO this machinery). D1 (economics of
re-extraction).
