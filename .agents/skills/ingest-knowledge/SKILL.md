---
name: ingest-knowledge
description: Knowledge layer — ingest a source (course notes, video notes, docs) once into canonical memory, then derive AGENTS.md fragments and skills for any domain. Use for knowledge from courses/materials so it stops being project-local.
---

# Ingest Knowledge

"Once, and well": knowledge given once must work in every project and domain.

1. Save the raw source to `knowledge/sources/<slug>/<source>.md` (check the
   `knowledge/` dir exists; create it if not).
2. Convert source material into `FACT:` lines or bullets in a temporary
   markdown file (one fact per line, no filler).
3. `mini-agi mem consolidate <file> --domain <domain>` — appends facts to
   canonical memory with provenance (source hash, date, domain).
   Duplicates are rejected by content hash.
4. `mini-agi derive` — regenerates the context brief and per-domain
   AGENTS.md fragments from canonical memory.
5. `mini-agi provenance` — drift gate must pass.
6. For a new domain that needs its own working rules: write the rules ONCE
   into the derived fragment location under memory/derived/per-domain/, or
   as a skill under `.agents/skills/`. Never duplicate facts between
   projects.

Rule: knowledge lives in canonical memory; skills and AGENTS.md are views.
If they disagree, canonical wins.

## Completion criteria

- [ ] The raw source exists under `knowledge/sources/<slug>/`, untouched.
- [ ] The facts file has one fact per line, no filler.
- [ ] `mini-agi mem consolidate --domain <domain>` output shows the facts
      with provenance; duplicates were rejected, not rewritten.
- [ ] `mini-agi derive` and `mini-agi provenance` ran; the fingerprint
      matches the committed index.
- [ ] No fact was duplicated into a project-local file; canonical is the
      single source.
