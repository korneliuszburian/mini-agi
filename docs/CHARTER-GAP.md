# CHARTER-GAP AUDIT — stan mini-agi vs docs/CHALLENGE.md

> Audit: 2026-08-12. Źródło prawdy: docs/CHALLENGE.md (verbatim), docs/PLAN.md,
> docs/EXPERIMENTS.md, docs/VERIFIABLE-REWARD-RESEARCH.md, scripts/verify.sh,
> stan repo (HEAD 3a3115a, 600 testów, verify ALL GREEN).
> Cel: kryteria sukcesu charteru → status → dowód → dziura → plan zamknięcia.

## Wnioski (wprost)

1. **Projekt jest znacznie bliżej charteru, niż wygląda z poziomu codziennej
   pracy.** Przełomowy wzorzec istnieje i jest ZMIERZONY (EXP-012/013, non-
   overlapping CIs). Nie jest to "slop" — jest to pipeline z udokumentowaną
   przewagą w klasie zadań, w której przewaga jest możliwa.
2. **Główne dziury to konsolidacja i portability, nie mechanika.** Kryterium
   3 (7 paradoksów z metryką) nie miało jednego artefaktu — ta dziura jest
   zamknięta przez ten dokument (sekcja "Paradoksy"). README był nieaktualny
   (450 zamiast 600 testów) — poprawiony w tym samym cyklu.
3. **Dwie dziury wymagają eksperymentów, nie dokumentów**: (a) dowód, że przy
   przepełnieniu kontekstu nie ginie żadna decyzja/informacja (kryterium 2),
   (b) walidacja portability pipeline'u poza Codex (kryterium 5).

## Kryteria sukcesu — macierz statusów

| # | Kryterium (CHALLENGE.md) | Status | Dowód |
| --- | --- | --- | --- |
| 1 | Pipeline pełnego cyklu (ticket→research→spec→implement→verify→review→retro) | **ZAMKNIĘTE** | Phases 0–11 DONE (PLAN.md); `loop dispatch/objective/verify`, tickets + claims (ADR-0008), artifacts/spec, failure register, orchestrate skill; ten audit sam jest produktem pipeline'u. |
| 2 | Pamięć wieloprojektowa; nic nie ginie przy przepełnieniu kontekstu | **CZĘŚCIOWE** | canonical (99 plików, ~1210 faktów) → derived/brief; provenance gate w verify.sh (canonical_sha256 == brief); compact/handoff skills; ingest-knowledge. **Dziura G4**: brak ZMIERZONEGO dowodu "nic nie ginie" (brak eksperymentu w stylu EXP). |
| 3 | 7 paradoksów — każde z rozwiązaniem i metryką | **ZAMKNIĘTE (ten dokument)** | Sekcja "Paradoksy" niżej — 7/7 z mechanizmem i dowodem. |
| 4 | Ewale 4D z przewagą nad baseline; regresyjne gate'y; checkpointing | **ZAMKNIĘTE** | Eval engine 4D (outcome/trajectory/tool-use/cost), 26 cases, gate w verify.sh (0 regresji), best-state bound, METRICS.md time series, checkpoint.sh (T008 journal). Przewaga: EXP-012/013. |
| 5 | Udokumentowany wzorzec przełomowy + opis podpinania nowych projektów | **CZĘŚCIOWE** | Wzorzec: EXP-012/013 + VERIFIABLE-REWARD-RESEARCH.md (addendum) + `codex --iterate`/`--blind-worker` w binarku. Onboarding: README + `mini-agi init` + MCP. **Dziura G5**: pipeline nie był walidowany pod realnym Claude Code / innym agentem (CLAUDE.md shim istnieje, nieprzetestowany). |

## Paradoksy — matryca (kryterium 3, zamknięcie dziury G1)

| # | Paradoks | Mechanizm | Metryka / dowód | Status |
| --- | --- | --- | --- | --- |
| 1 | Utrata kontekstu między sesjami | canonical memory (append-only, datowane), derived views, brief; compact (2-stopniowy) + handoff skills; goal-context przenoszony między sesjami | Provenance gate w verify.sh: `canonical_sha256 ... matches the brief` (audit krok [ok]); ~1210 faktów w canonical; ta sesja kontynuowała cel poprzedniej bez re-researchu (kontekst gola). | Działa, mierzalne |
| 2 | Token maxing | Budżet kontekstu: budgeted skills list (TICKET-14, cap 8000 znaków = 2%, rachunek ≤ cap), brief cap, hard budget gates loopu (max_tokens/max_cost_usd) | verify.sh `budget` krok [ok]; test `verify_blocks_close_on_hard_budget_breach`; koszt w run.json (cost_usd) + METRICS.md kolumna tokens | Działa, mierzalne |
| 3 | Coherence Collapse | Edit-commit checkpointing: `checkpoint.sh begin/verify`, journal T008 (BEGIN→VERIFY-PASS/FAIL), rollback do ostatniego green na czerwonym gate | Audit journala w verify.sh (`checkpoint` krok [ok]); health.rs wykrywa anomalie journala (unpaired BEGIN); rollback-on-red przetestowany | Działa, mierzalne |
| 4 | Memory contamination / semantic drift | Encoding (episodic buffer) oddzielony od konsolidacji (dream promote, canonical-first); derived regenerowane, nigdy ręcznie; dedup; supersede lineage | verify.sh `mem-dedup` + `derive` + `provenance` kroki [ok]; na konflikcie canonical wygrywa (ADR-0002) | Działa, mierzalne |
| 5 | Silently failing | Weryfikacja deterministyczna = wymóg: verify.sh (build/fmt/clippy/tests/skills/eval-gate/audit), ADR-0011 verifier (verified/disagrees/unverified, fail-closed: błąd verifiera blokuje close), judge-drift calibration (precision ≥ min_judge_precision) | 44 wykonania verifierów w attribution; judge precision 1.000 (26 weryfikacji); EXP-008: kalibracja złapała realny drift (date-rollover, 89.5%→100%) | Działa, mierzalne, PROWENIENNIE |
| 6 | Goal drift / reactive loops | Termination conditions (max 3 retry, max 40 kroków), bounded rerun attempts (max_rerun_attempts), budget stop w objective, no-progress STOP (`dispatch_no_work`), repair-aware dispatch (Mechanical/Spinning/Semantic, GGC #60) | Testy: `objective_blocks_exhausted_case_beyond_rerun_bound`, `dispatch_no_work_is_a_positive_stop_signal`, CLI exit-code contract (STOP=0); `loop status --attempts` eksponuje numerator pilota EXP-005 | Działa, mierzalne |
| 7 | Wiedza rozproszona | ingest-knowledge → canonical facts → derived AGENTS.md fragments + skills; jeden rejestr skills z verify hookami; dual-registration drift (D4); MCP memory_query dla zewnętrznych agentów | 17 skills w rejestrze, skills verify-all w gate; `skill verify-all` + drift check w verify.sh; codex sessions (AFK-SUPERVISOR) używają memory_query przez MCP | Działa, mierzalne |

## Przełom (kryterium 4/5 — dowód już istnieje)

- **EXP-012** (N=5): P (blind best-of-k) 10/20 = 50% (Wilson CI [0.30, 0.70])
  vs K (verified-iteration loop) 20/20 = 100% (CI [0.84, 1.00]) — CIs
  NIEPOKRYWAJĄCE. Pod-progiem (e1+e2): P 0/10 vs K 10/10 (p < 0.001).
- **EXP-013** (N=10, `--blind-worker`): P 25% [0.142, 0.402] vs K 82.5%
  [0.680, 0.913] — replikacja. Equal-attempts comparator: P best-of-5 25%.
- **Uczciwe negatywy**: EXP-005/009/010/011 — ~70 pre-registered solo runs,
  gate odrzucił wszystkie kandydatki (solo 10/10); kontrola odrzuciła
  hipotezę szybkości — task-shopping zabroniony pre-registracją. Granica:
  e6 (multi-funkcyjny, 3/10, 5 attempts exhausted), eskalacja feedbacku 0/5.
- **Wniosek**: kernel transformuje słabe ślepe generacje w zweryfikowane
  passy DOKŁADNIE tam, gdzie solo jest pod progiem. Wzorzec shipuje jako
  `mini-agi codex --iterate N`.

## Dziury i plan zamknięcia (priorytetowo)

| ID | Dziura | Kryterium | Plan | Status |
| --- | --- | --- | --- | --- |
| G1 | Brak skonsolidowanej matrycy paradoks→mechanizm→metryka | 3 | Ten dokument (sekcja "Paradoksy") | **ZAMKNIĘTE 2026-08-12** |
| G2 | README nieaktualny (450 vs 600 testów, brak wzmianki o latest) | 1/5 | Refresh README w tym cyklu | **ZAMKNIĘTE 2026-08-12** |
| G3 | README: brak konkretnego "jak podpiąć nowy projekt" | 5 | Sekcja onboarding (init → codex trust → loop dispatch → verify) | **ZAMKNIĘTE 2026-08-12** |
| G4 | "Nic nie ginie przy przepełnieniu kontekstu" bez pomiaru | 2 | EXP-014: sesja z dużą objętością → compact → kontynuacja → sprawdzić, że żaden fakt/zadanie nie zginął (protokół jak EXP-005: pre-registered, N≥3, dowody commited) | PLAN |
| G5 | Portability poza Codex niewalidowany | 5 | Eksperyment: ten sam pipeline (memory_query + loop) pod Claude Code / opencode; wynik do EXPERIMENTS.md | PLAN |
| G6 | Hartowanie pozostałych powierzchni (supervisor, init, clifmt, sandbox, failure, mismatch, research_registry) | 1 | Cykle falsyfikatorów jak cykle 1–4 (9 defektów, 600 testów) | BACKLOG (user decyzja) |
| G7 | README twierdzi "39 tools MCP" — zweryfikować i ujednolicić liczbę | 4 | Sprawdzone 2026-08-12: TOOLS w mcp.rs = 39 wpisów — README poprawny | **ZAMKNIĘTE 2026-08-12** |

## Następny krok

Zgodnie z decyzją usera: audyt → plan → pierwsza dziura zamknięta. Ten cykl
zamyka G1+G2+G3 (artefakt + README). Kolejny cykl do wyboru: G4 (eksperyment
overflow-loss) albo G5 (walidacja portability) — oba z falsyfikowalnym
protokołem w stylu EXP.
