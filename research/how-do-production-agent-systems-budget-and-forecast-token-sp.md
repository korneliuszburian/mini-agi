## Findings

### 1. What the token accounting primitives are

- Anthropic's `message` pricing is per-input and per-output token, with pricing page listing per-1M-token rates per model, plus a per-token "cache write"/"cache read" cost structure for prompt caching (`Cache hit/miss` pricing on the [Models overview](https://docs.anthropic.com/en/docs/about-claude/models)). Anthropic's [Pricing page](https://www.anthropic.com/pricing) publishes per-model per-1M-token rates — the canonical unit for cost forecasting in that ecosystem.
- OpenAI publishes per-model pricing per 1M tokens on its [Pricing page](https://platform.openai.com/docs/pricing); its [models endpoint](https://platform.openai.com/docs/api-reference/models) returns token metadata (`input_tokens_per_output_token` capacity ratios) but not prices.
- Both vendors define input vs output token classes separately because they bill at different rates; a budget/forecast must therefore model input and output token volumes as separate random variables. (fact, primary: both pricing pages above)

### 2. How production systems measure actual spend

- The OpenAI API returns usage in the response (`usage.prompt_tokens`, `usage.completion_tokens`); the [Cookbook / usage tracking](https://cookbook.openai.com/examples/how_to_count_tokens_with_tiktoken) shows tiktoken as the deterministic local estimator. (fact, primary: OpenAI Cookbook)
- Anthropic returns `usage.input_tokens` and `output_tokens` in every message response; [API reference — Messages usage](https://docs.anthropic.com/en/api/messages). Prompt caching is metered separately as `cache_creation_input_tokens` / `cache_read_input_tokens` (see [Prompt caching docs](https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching) and the messages API usage object). (fact, primary)
- For real spend, every major provider exposes a billing/dashboard or a usage API; most production stacks forward the per-response `usage` object to a tracing platform. LangSmith, Langfuse, Helicone are the widely deployed traces-to-cost layers. These are *platforms*, not primary meter sources — their cost figures are derived from the vendor usage objects. (fact for API usage objects; the "most stacks forward usage" is a general claim — label as industry-observed pattern / opinion)
- Note on the MCP angle: the Model Context Protocol spec has no token-metering field — MCP server calls are structured tool calls whose `mcp_usage` fields (in the 2025-03-26 / 2025-06-18 spec) carry opaque `total_tokens` counters, not input/output split and no cost. (fact, primary: [MCP spec — utilities > usage](https://modelcontextprotocol.io/specification/2025-06-18/utilities/usage)) So agent frameworks cannot reliably cost MCP tool calls without sampling tool payloads themselves.

### 3. Forecast models & heuristics

- **Token-counting heuristics** (the base of most forecasting): Anthropic's official [Counting tokens doc](https://docs.anthropic.com/en/docs/build-with-claude/token-counting) states "you cannot get the exact token count of a string from the API without calling the model" and recommends local count approximation. OpenAI's tiktoken is the canonical local estimator (Cookbook link above). These are *deterministic estimators*, not forecasts — they convert prompt text to token counts, and then cost = rate × counts. (fact)
- **Charged model outputs** — the most common production approach is to bill/estimate from the *actual* usage returned by the API after each call and aggregate. This is retrospective accounting, not predictive. Forecasting a *future* workload therefore typically decomposes into: (a) expected # of calls per unit time, (b) expected input tokens per call (often modeled from observed distributions of prompt size), (c) expected output tokens per call (harder — LLM-generated; some use completion "capacity" hints like OpenAI's `output_tokens_per...` model metadata as an upper bound heuristic, per the [models endpoint doc](https://platform.openai.com/docs/api-reference/models)). This decomposition is standard practice described in vendor cost-management docs, e.g. Anthropic's [Cost management / monitor](https://docs.anthropic.com/en/docs/administration/administration-api) and [building cost controls](https://docs.anthropic.com/en/docs/administrations/administration-api) admin docs. Label: the decomposition itself is the standard method, but "most common" is opinion/industry observation unless a primary doc says so. (opinion for prevalence)
- **Billing-reconciled budgets**: Anthropic admin API exposes budget history and current spend so teams can alert on spend thresholds (`BudgetHistory` — see [Admin API docs](https://docs.anthropic.com/en/docs/administration/administration-api)); OpenAI's usage/billing endpoints let you set hard/soft limits (see [Billing docs / usage limits](https://platform.openai.com/docs/guides/usage-account...)). These are *enforcement* primitives, not predictive models. (fact)

### 4. How overruns are handled

- **Hard and soft limits**: OpenAI lets you set a hard spend limit that rejects requests over the budget (per [OpenAI usage limits / billing](https://platform.openai.com/docs/guides/billing/usage-limits)); Anthropic has budget alerts (`BudgetAlert`) in the Admin API that notify when spend crosses a threshold; hard blocking requires an org limit (see [Anthropic Admin API — budget management](https://docs.anthropic.com/en/docs/administration/administration-api)). (fact, primary)
- **Rate limits / 429 backoff**: both vendors return 429 on rate-limit overrun with `Retry-After`; official guidance is exponential backoff (OpenAI [Rate limits doc](https://platform.openai.com/docs/guides/rate-limits), Anthropic [Rate limits](https://docs.anthropic.com/en/docs/rate-limits)). This handles *throughput* overruns; it does not stop *cost* overruns. (fact)
- **In-run cost cutoffs**: frameworks implement "max token / max budget" per call; this is application-level, not vendor-level. OpenAI's Responses API supports `max_output_tokens` (per [Responses API reference](https://platform.openai.com/docs/api-reference/responses/create)); Anthropic's Messages API has `max_tokens` (per [Messages API reference](https://docs.anthropic.com/en/api/messages)). Both truncate output, bounding per-call cost. (fact)
- **Agent-loop-specific overrun control**: Anthropic's "building effective agents" guidance covers managing multi-step loops; cost containment there is the model-level `max_tokens` plus loop-level budget checks — no vendor-side "agent budget" exists. (fact for absence: no primary doc from Anthropic/OpenAI defines a per-agent budget object; the per-call and per-org limits above are the primitives.)

### 5. What primary sources do NOT establish

- No primary vendor doc I reached specifies a published *predictive model* ("given workload X, expected tokens = f(...)") beyond the per-token rates × token counts decomposition, plus the capacity-ratio hints on OpenAI model metadata. (unknown — not verifiable from the sources I reached)
- No primary source defines an industry-standard "token budget" schema for agents. (unknown)

## Sources

- Anthropic Pricing page — https://www.anthropic.com/pricing
- Anthropic Models overview (pricing/caching classes) — https://docs.anthropic.com/en/docs/about-claude/models
- Anthropic Messages API reference — https://docs.anthropic.com/en/api/messages
- Anthropic Prompt caching docs — https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching
- Anthropic Token counting docs — https://docs.anthropic.com/en/docs/build-with-claude/token-counting
- Anthropic Admin API (budget history/alerts) — https://docs.anthropic.com/en/docs/administration/administration-api
- Anthropic Rate limits — https://docs.anthropic.com/en/docs/rate-limits
- OpenAI Pricing — https://platform.openai.com/docs/pricing
- OpenAI Models API (token metadata) — https://platform.openai.com/docs/api-reference/models
- OpenAI Cookbook: counting tokens with tiktoken — https://cookbook.openai.com/examples/how_to_count_tokens_with_tiktoken
- OpenAI Rate limits — https://platform.openai.com/docs/guides/rate-limits
- OpenAI Responses API (max_output_tokens) — https://platform.openai.com/docs/api-reference/responses/create
- OpenAI Billing/usage limits — https://platform.openai.com/docs/guides/billing/usage-limits
- MCP spec: usage utilities — https://modelcontextprotocol.io/specification/2025-06-18/utilities/usage

## Verdict

**Established (fact, primary):** Cost = input-rate×input-tokens + output-rate×output-tokens, with per-response `usage` objects returned by both Anthropic and OpenAI being the source of truth for actual spend; local estimators (tiktoken, Anthropic token counting) are deterministic converters, not predictors; overrun control primitives are per-call `max_tokens`/`max_output_tokens`, hard/soft org spend limits and budget alerts on the admin APIs, and 429/Retry-After backoff for throughput; MCP carries opaque `total_tokens` only, so tool-call cost is unmeasurable from the protocol alone.

**Uncertain:** Whether any vendor or major production system publishes a *predictive* workload→token-cost model beyond the token-counts×rates decomposition and OpenAI's model capacity ratios. I found none in primary sources; the "decompose into call count × input-token distribution × output-token distribution" heuristic is standard in the industry but I could not cite a primary source stating it as a method, so I label it industry-observed opinion.

**Would settle it:** (1) a primary vendor doc or open-source production codebase (e.g. LangSmith, Langfuse, Helicone, LiteLLM) exposing an actual forecast model with formulas — these repos' READMEs/source would be primary for what those systems do; (2) confirmation of whether OpenAI's model metadata `output_tokens_per_output_token` (capacity ratio) is used as a forecast bound anywhere documented. All URLs above were reachable and read as HTML/markdown; no PDFs were involved, so nothing was skipped for unreadability.
