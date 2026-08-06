## Findings

- **deepseek-v4-flash output price: $0.28 per 1M tokens.** The official DeepSeek API pricing table lists `deepseek-v4-flash` with "1M OUTPUT TOKENS — $0.28". Source: DeepSeek API Docs — Models & Pricing (official first-party page), fetched 2026-08-06: `https://api-docs.deepseek.com/quick_start/pricing`.

- Context from the same table (fact, same source):
  - 1M input tokens (cache hit): **$0.0028**
  - 1M input tokens (cache miss): **$0.14**
  - 1M output tokens: **$0.28**
  - Model version listed: `DeepSeek-V4-Flash-0731`; context length 1M; max output 384K.

- Caveat (fact, same source): the page warns "We plan to raise the overall pricing for DeepSeek API services in the near future, with a significant increase expected." So $0.28 is the price as of 2026-08-06 but is announced as subject to change.

## Sources

- DeepSeek API Docs — Models & Pricing: `https://api-docs.deepseek.com/quick_start/pricing` (official, first-party). Retrieved 2026-08-06.

## Verdict

- **Established:** As of 2026-08-06, the official DeepSeek price for `deepseek-v4-flash` output tokens is **$0.28 per 1M tokens** ($0.28/M).
- **Uncertain:** Whether the announced near-term price increase has already taken effect (the page warns of a "significant increase" in the near future but still shows $0.28).
- **What would settle it:** A fresh fetch of `https://api-docs.deepseek.com/quick_start/pricing` at the time of billing to confirm the current rate.
