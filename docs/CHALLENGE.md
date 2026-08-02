# Charter — stały pipeline agentic coding (zalążek firmowego mini-AGI)

> Źródło: user prompt (v1, przed `agentic-core`). Zachowane verbatim 2026-08-02 —
> dokument założycielski trzech generacji: `agentic-core` (v1) →
> `mini-agi` (v2 PoC, spec) → `mini-agi-rs` (v3, Rust product). Nigdy nie usuwać,
> nie parafrazować; zmiany tylko jako ADR + wersjonowany dodatek.

## Cel

Zaprojektuj i zbuduj — od zera, na osobnym branchu nowego repozytorium — stały pipeline
agentic coding, który jest zalążkiem firmowego mini-AGI (Block/Jack Dorsey, Sequoia):
systemem, w którym wiedza i decyzje nie żyją w pojedynczych sesjach, tylko w trwałej,
współdzielonej inteligencji. Pipeline jest moim podstawowym narzędziem pracy — nie
„jednorazowym researchiem".

## Paradoksy, które system MA rozwiązać (nie maskować)

Nazwij i rozwiąż każdy z nich wprost, z mierzalnym dowodem:

1. Utrata kontekstu / „zapominanie" między sesjami — wiedza przekazana raz musi działać
   w każdym projekcie i domenie, bez powtórzeń.
2. Token maxing — kupowanie jakości tokenami („tokeny na task rosną szybciej niż wartość").
   Harness ma być dźwignią, która tnie koszt bez cięcia jakości.
3. Coherence Collapse — agent pisze poprawną edycję, potem ją nadpisuje/niszczy.
   Zastosuj edit-commit checkpointing: stan przed każdą dalszą edycją jest zapisywany.
4. Memory contamination / semantic drift — szum i konflikt starych i nowych faktów.
   Oddziel encoding (bufor epizodyczny) od konsolidacji (stabilizacja semantyczna).
5. Silently failing — agent twierdzi, że przeszedł testy, których nie uruchomił.
   Weryfikacja = wymóg deterministyczny, nie deklaracja.
6. Goal drift / reactive loops — agent oddala się od celu lub kręci się w pętli.
   Termination conditions, budżety kroków/tokenów i punkt kontroli celu jako first-class.
7. Wiedza rozproszona — wiedza z kursów/materiałów (Polubis, ThePrimeagen, Pocock,
   Karpathy, Andy Osman i inni) ginie w jednym projekcie zamiast zasilać wszystkie.

## Zakres (co ma powstać)

1. ORCHESTRACJA: pełny workflow — pomysł/ticket → research → specyfikacja →
   implementacja → review → evale → wdrożenie → retrospektywa. Subagenci, skille
   i templaty spięte wspólnymi, logicznymi ścieżkami i konwencjami plików.
   Zaczynaj od wzorca orchestrator–subagent; dodawaj generator–verifier, agent teams,
   message bus czy shared state tylko wtedy, gdy mierzalnie rozwiązują problem.
2. MEMORY SYSTEM: wieloprojektowa, kanoniczna pamięć długoterminowa z warstwą
   „derived context". Zasady: kanoniczna pamięć = źródło prawdy; embeddingi, podsumowania,
   AGENTS.md, skille są z niej wyprowadzane i zawsze wskazują provenance (wersję źródła);
   gdy się różnią, kanoniczna wygrywa. Zapisy decyzyjne jako log przyrostowy
   (append-only, datowany) — nigdy przepisywany.
3. STANDARDY: lean prompting, context budgeting i kompakcja dwustopniowa (checkpoint
   z trwałą pamięcią + summary resumability + live-tail ostatnich wiadomości), progressive
   disclosure, context firewalls (subagenci zwracają capped summary), blast radius,
   least-privilege dla tooli, audit/trajectory retention.
4. EWALUACJA: czterowymiarowa (outcome + trajectory + tool-use + cost) wg standardów
   OpenAI Cookbook/docs i stanu badań 2026. Trajectory scoring per-krok (geomean),
   cost-normalized success, LLM-as-judge z kalibracją, golden trajectories, regresyjne
   gate'y w CI. Weryfikacja formalna tam, gdzie istnieje (testy, typy, lint).
5. KNOWLEDGE LAYER: proces pozyskiwania wiedzy „raz, a dobrze" — kursy/materiały →
   kanoniczne fakty → wyprowadzone AGENTS.md/skille/refs per domena i projekt.
6. WZORZEC PRZEŁOMOWY: co najmniej jeden udokumentowany, nowy wzorzec architektury
   spinający powyższe w całość — z mierzoną przewagą, nie teoretyczną obietnicą.

## Architektura: wzorce do rozważenia (decyzja należy do Ciebie)

- Memory lifecycle: bufor epizodyczny → konsolidacja semantyczna (inspiracja: GAM,
  EverMemOS, MAGMA — nie wektorowa baza jako cała odpowiedź).
- Multi-granularity context: pełny → szczegółowe → brief → placeholder, wybierane
  wg przewidywanej przydatności dla następnego kroku (inspiracja: PACE).
- Subagenci jako context firewalls; wspólny mały scratchpad (cel + kluczowe fakty)
  + izolowane historie agentów; typed handoffs (schema, nie proza).
- Hyperlokalny kontekst (AGENTS.md, `.agents/checks/*.md`) + globalny world model.
- Review/protectors z „default to action" (sprawdź, oznacz, skieruj — zanim poprosisz
  człowieka), z mierzalnymi rubrykami, nie „czy wygląda dobrze".
- Sensor na wejściu i wyjściu: wspólny kontrakt CLI (np. `just fmt/test/typecheck`)
  jako deterministyczny pomost między agentem a CI.

## Stack i biblioteki (zweryfikuj po metrykach, nie po modzie)

- Dla kontraktów tool-calling preferuj biblioteki natywnie JSON Schema (TypeBox/ArkType):
  jedna definicja = validator + OpenAPI + schema dla modelu. Standard Schema jako
  warstwa portability. Valibot/Zod — tylko tam, gdzie wygrywają (edge/bundle vs ekosystem).
- Każdy wybór biblioteki uzasadnij metryką (bundle, inference, ekosystem, JSON Schema,
  koszt cold-start), analogicznie do decyzji „valibot zamiast zod".
- NIE używaj rozwiązań wymagających własnego klucza API — tylko Codex / SDK Codex.

## Autonomia

- Bez pytania: research, czytanie, projektowanie, zakładanie repo, tworzenie skilli,
  evali, orchestracji, edit-commit checkpointing, praca na branchu, testy nie-niszczące.
- Zatrzymaj się i zapytaj przed: publikacją zewnętrzną, działaniami destrukcyjnymi,
  płatnościami, materialnym rozszerzeniem zakresu.

## Research (źródła)

- OpenAI: prompt engineering / GPT-5.6 guidance (docs) + Cookbook (Responses API,
  long-running tasks, context compaction, evals). Wzorce 1:1, ale tylko Codex/SDK.
- Codex docs 2026: AGENTS.md (precedencja, limit 32 KiB), skills (progressive disclosure,
  budżet listy 2% kontekstu), subagenci (default/worker/explorer, custom TOML),
  MCP, pluginy, memory.
- Ewaluacja: Springer „From benchmarks to deployment" (2026), AgentLens (arXiv 2607.06624),
  TRACEProbe (2607.06184), TRAJEVAL (2603.24631), CostBench (ACL 2026), „Scaling the
  Harness" (2605.26112), Writer „Harness Effect" (2607.06906).
- Memory: GAM (ACL 2026), EverMemOS (ACL 2026), MAGMA (ACL 2026), AgeMem (ACL 2026),
  PACE (ACL 2026), Oracle „Persistent Memory & Derived Context".
- Orchestracja: Anthropic multi-agent patterns (2026), arxiv „Practical Guide"
  (2512.08769), UNU „Engineering and Governing the Agent Harness", David Gasquez,
  Kilo.ai, Jaymin West, Alex Lavaee, SaaS with Alex, InfoWorld.
- Mini-AGI: Sequoia podcast (Dorsey), Block „From Hierarchy to Intelligence"
  (block.xyz/inside), Block engineering „Protecting Our Systems with Intelligence".
- Karpathy: „think before coding", „simplicity first" — scal jako behavioral guidelines.
- Skille Matta Pococka: kopiuj 1:1; ulepszaj tylko przy wykrytej realnej niespójności.

## Wolna ręka

Nie narzucam struktur, nazw, warstw pamięci ani rozwiązań orchestracji — decyzje
projektowe należą do Ciebie, oceniaj je dowodami. Powyższe wzorce to hipotezy do
zweryfikowania, nie wymogi. Oczekuję spójnej całości (templaty → ścieżki → skille →
memory → evale) zazębionej logicznie. Wzorce mają być przenośne poza Codex
(Claude Code, Pi Agent, Hermes) — AGENTS.md kanoniczny, CLAUDE.md jako import-shim.

## Kryteria sukcesu (mierzalne)

1. Działający pipeline w nowym repo na osobnym branchu, obsługujący pełny cykl pracy.
2. Memory system wieloprojektowy: wiedza przekazana raz jest dostępna i używana
   w każdej domenie; przy przepełnieniu kontekstu żadna decyzja/informacja nie ginie.
3. Każdy z 7 paradoksów (sekcja wyżej) ma udokumentowane rozwiązanie z metryką.
4. Evale czterowymiarowe pokazujące przewagę nad baseline (jakość, koszt, tokeny,
   latency); regresyjne gate'y w CI; edit-commit checkpointing chroni przed Coherence
   Collapse.
5. Udzokumentowany wzorzec przełomowy + ARCHITECTURE/README opisujące pipeline,
   memory i sposób podpinania nowych projektów.

## Format odpowiedzi

- Na początku: wnioski — co powstało, jak się to ma do mini-AGI i dlaczego to przełom.
- Dowody: metryki z evali, cytowane źródła, wzorce skopiowane 1:1.
- Potem: następny krok — co uruchomić i co zweryfikować.
- Utnij wstępy, powtórzenia, generyczne zapewnienia i zbędne tło.

## Ton

Mów wprost. Jeśli coś się nie sprawdziło — nazwij konkretny problem, zanim podasz
następny krok. Bez zbędnych pochwał i podpisów.
