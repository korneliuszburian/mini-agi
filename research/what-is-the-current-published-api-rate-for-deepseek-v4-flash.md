## Findings

**Claim 1 (fact): deepseek-v4-flash input pricing, per 1M tokens, published by DeepSeek**
- 1M input tokens, **cache hit**: **$0.0028 USD**
- 1M input tokens, **cache miss**: **$0.14 USD**
- 1M output tokens: **$0.28 USD**

Source: "Models & Pricing" — DeepSeek API Docs, pricing table for `deepseek-v4-flash`, fetched live 2026-08-06. URL: https://api-docs.deepseek.com/quick_start/pricing

**Claim 2 (fact): the rate is current as of the "DeepSeek-V4-Flash-0731" model version**
- The same pricing page lists the model version as `DeepSeek-V4-Flash-0731`; the "Your First API Call" page confirms the `deepseek-v4-flash` alias now points at DeepSeek-V4-Flash-0731.
- Sources: https://api-docs.deepseek.com/quick_start/pricing and https://api-docs.deepseek.com/

**Claim 3 (fact): DeepSeek itself warns the published rates are about to change upward**
- Footnote (2) on the pricing page: "We plan to raise the overall pricing for DeepSeek API services in the near future, with a significant increase expected... The specific pricing plan will be subject to official notice."
- Source: https://api-docs.deepseek.com/quick_start/pricing

**Claim 4 (fact): cache-hit vs cache-miss is the only pricing split; there is no separate "standard vs thinking" input tier for flash**
- The table shows exactly three price cells per model (cache-hit input, cache-miss input, output); thinking mode is supported but billed through the same input/output rates.
- Source: https://api-docs.deepseek.com/quick_start/pricing

## Sources

1. DeepSeek API Docs — "Models & Pricing" (primary, official). https://api-docs.deepseek.com/quick_start/pricing
2. DeepSeek API Docs — "Your First API Call" (primary, official; model version note). https://api-docs.deepseek.com/

## Verdict

**Established:** DeepSeek's official API pricing page currently lists `deepseek-v4-flash` at **$0.14 per 1M input tokens (cache miss)** and **$0.0028 per 1M input tokens (cache hit)**, USD, output at $0.28/1M. The page is the primary source and the sole authority on this rate.

**Uncertain:** The rate is time-sensitive — DeepSeek states pricing will rise "significantly" in the near future, so any figure is only valid until an official notice changes the page. The cache-hit figure depends on your traffic actually hitting the cache; a naive "input rate" answer without that caveat would overstate/understate actual billing depending on caching behavior.

**What would settle it:** Re-fetching https://api-docs.deepseek.com/quick_start/pricing at the moment of billing; any other statement (blogs, reseller dashboards, benchmarks) is secondary and should not be trusted over this page.
