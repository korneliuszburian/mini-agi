## Findings

All claims below are **fact** sourced from primary documents (OpenTelemetry GenAI semantic conventions, fetched 2026-08-06 from the `main` branch of the dedicated repository; the convention set is **not yet stable** — every span, attribute, and metric carries `Development` stability).

### Context / provenance

- The GenAI semantic conventions moved out of the main `opentelemetry/semantic-conventions` repo into a dedicated repository `open-telemetry/semantic-conventions-genai`. The old pages at `opentelemetry.io/docs/specs/semconv/gen-ai/` are redirect stubs pointing there. (Fact — "Moved: Generative AI semantic conventions" page at https://opentelemetry.io/docs/specs/semconv/gen-ai/; README at https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/README.md)
- The GenAI repo covers GenAI (LLM, agent, embeddings, retrieval) and MCP operations; shared attributes and conventions not specific to GenAI (e.g. `server.address`, `error.type`, `db.*`) continue to live in the upstream `semantic-conventions` repo, pinned at a version (docs reference `v1.44.0` / spec `v1.56.0`). (Fact — https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/README.md)
- The repo's Schema URL field is still marked `TODO`. (Fact — https://github.com/open-telemetry/semantic-conventions-genai/blob/main/README.md)

### Span semantics (LLM / inference and related operations)

Document: `docs/gen-ai/gen-ai-spans.md` at https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-spans.md

- **Span model**: GenAI spans represent logical operations as observed by the caller, covering the operation duration including automatic retries. **Inference span** (`gen_ai.inference.client`) SHOULD be span kind `CLIENT` (or `INTERNAL` for same-process models); span name SHOULD be `{gen_ai.operation.name} {gen_ai.request.model}`. (Fact)
- **Required attributes on the inference span**: `gen_ai.operation.name`, `gen_ai.provider.name`. (Fact)
- **Conditionally required**: `error.type` (if error), `gen_ai.conversation.id` (when available), `gen_ai.output.type` (when output format requested), `gen_ai.prompt.name`/`gen_ai.prompt.version` (when named template used), `gen_ai.request.choice.count`, `gen_ai.request.model` (if available), `gen_ai.request.seed`, `gen_ai.request.stream`, `gen_ai.request.top_k`, `server.port` (if `server.address` set). (Fact)
- **Recommended**: `gen_ai.conversation.compacted`, `gen_ai.request.frequency_penalty`, `gen_ai.request.max_tokens`, `gen_ai.request.presence_penalty`, `gen_ai.request.previous_response.id`, `gen_ai.request.reasoning.level`, `gen_ai.request.stop_sequences`, `gen_ai.request.temperature`, `gen_ai.request.top_p`, `gen_ai.response.finish_reasons`, `gen_ai.response.id`, `gen_ai.response.model`, `gen_ai.response.time_to_first_chunk` (streaming), token-usage attributes `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`, `gen_ai.usage.reasoning.output_tokens`, `gen_ai.usage.cache_creation.input_tokens`, `gen_ai.usage.cache_read.input_tokens`, plus `server.address`. (Fact)
- **Opt-In (content, PII-warning)**: `gen_ai.input.messages`, `gen_ai.output.messages`, `gen_ai.prompt.variable.<key>`, `gen_ai.system_instructions`, `gen_ai.tool.definitions`. These MUST follow JSON schemas in `model/gen-ai/`. (Fact)
- **Sampling attributes** (SHOULD be set at span creation time): `gen_ai.operation.name`, `gen_ai.provider.name`, `gen_ai.request.model`, `server.address`, `server.port`. (Fact)
- **`gen_ai.operation.name` well-known values**: `chat`, `create_agent`, `create_memory`, `create_memory_store`, `delete_memory`, `delete_memory_store`, `embeddings`, `execute_tool`, `fetch_response`, `generate_content`, `invoke_agent`, `invoke_workflow`, `plan`, `retrieval`, `search_memory`, `text_completion`, `update_memory`, `upsert_memory`. (Fact)
- **`gen_ai.provider.name` well-known values**: `anthropic`, `aws.bedrock`, `azure.ai.inference`, `azure.ai.openai`, `cohere`, `deepseek`, `gcp.gemini`, `gcp.gen_ai`, `gcp.vertex_ai`, `groq`, `ibm.watsonx.ai`, `mistral_ai`, `moonshot_ai`, `openai`, `perplexity`, `x_ai`. (Fact)
- **`gen_ai.output.type` well-known values**: `text`, `json`, `image`, `speech`. (Fact)
- **Embeddings span** (`gen_ai.embeddings.client`): `gen_ai.operation.name` = `embeddings`; required `gen_ai.operation.name`, `gen_ai.provider.name`; recommended `gen_ai.embeddings.dimension.count`, `gen_ai.request.encoding_formats`, `gen_ai.response.model`, `gen_ai.usage.input_tokens`. (Fact)
- **Retrieval span** (`gen_ai.retrieval.client`): `gen_ai.operation.name` = `retrieval`; span name `{gen_ai.operation.name} {gen_ai.data_source.id}`; required `gen_ai.operation.name`; conditionally required `error.type`, `gen_ai.data_source.id`, `gen_ai.provider.name`, `gen_ai.request.model`; recommended `gen_ai.retrieval.top_k`; opt-in `gen_ai.retrieval.documents`, `gen_ai.retrieval.query.text`. (Fact)
- **Fetch response span** (`gen_ai.fetch_response.client`): `gen_ai.operation.name` = `fetch_response`; span name `{gen_ai.operation.name}` (response id excluded due to cardinality); required `gen_ai.operation.name`, `gen_ai.provider.name`, `gen_ai.response.id`; conditionally required `gen_ai.request.stream_cursor` (when resuming a stream). Token usage MUST NOT be reported for this operation. (Fact)
- **Memory span** (`gen_ai.memory.client`): `gen_ai.operation.name` = one of `create_memory_store`, `search_memory`, `create_memory`, `update_memory`, `upsert_memory`, `delete_memory`, `delete_memory_store`; conditionally required `gen_ai.memory.record.id`, `gen_ai.memory.store.id`; recommended `gen_ai.memory.record.count`; opt-in `gen_ai.memory.query.text`, `gen_ai.memory.records`. (Fact)
- **Execute tool span** (`gen_ai.execute_tool.internal`): `gen_ai.operation.name` = `execute_tool`; span name `execute_tool {gen_ai.tool.name}`; span kind `INTERNAL`; required `gen_ai.operation.name`, `gen_ai.tool.name`; conditionally required `error.type`, `gen_ai.agent.name`; recommended `gen_ai.tool.call.id`, `gen_ai.tool.description`, `gen_ai.tool.type`; opt-in `gen_ai.tool.call.arguments`, `gen_ai.tool.call.result`. (Fact)

### Agent spans

Document: `docs/gen-ai/gen-ai-agent-spans.md` at https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-agent-spans.md

- Agent conventions extend and override the GenAI spans conventions; the definition follows the Kaggle "Agents" whitepaper. (Fact)
- **Create agent span** (`gen_ai.create_agent.client`): span kind `CLIENT`, span name `create_agent {gen_ai.agent.name}`; required `gen_ai.operation.name`, `gen_ai.provider.name`; conditionally required `gen_ai.agent.description`, `gen_ai.agent.id` (provider-assigned stable id; in-memory ids NOT recommended), `gen_ai.agent.name`, `gen_ai.agent.version`, `gen_ai.request.model`. (Fact)
- **Invoke agent client span** (`gen_ai.invoke_agent.client`): remote invocation (e.g. OpenAI Assistants, AWS Bedrock Agents); span name `invoke_agent {gen_ai.agent.name}`; kind `CLIENT`; required `gen_ai.operation.name`, `gen_ai.provider.name`; conditionally required `gen_ai.agent.description`, `gen_ai.agent.id`, `gen_ai.agent.name`, `gen_ai.agent.version`, `gen_ai.conversation.id`, `gen_ai.data_source.id`, `gen_ai.output.type`, `gen_ai.request.choice.count`, `gen_ai.request.seed`; plus the same recommended/opt-in request/response/usage/content attributes as the inference span. (Fact)
- **Invoke agent internal span** (`gen_ai.invoke_agent.internal`): same-process invocation (e.g. LangChain, CrewAI); kind `INTERNAL`; `gen_ai.provider.name` is absent — only `gen_ai.operation.name` is required; conditionally required `error.type`, `gen_ai.agent.description`, `gen_ai.agent.name`, `gen_ai.conversation.id`, `gen_ai.data_source.id`, `gen_ai.output.type`, `gen_ai.request.choice.count`, `gen_ai.request.seed`. (Fact)
- **Invoke workflow span** (`gen_ai.invoke_workflow.internal`): kind `INTERNAL`, span name `invoke_workflow {gen_ai.workflow.name}`; required `gen_ai.operation.name`; conditionally required `error.type`, `gen_ai.workflow.name`; opt-in `gen_ai.input.messages`, `gen_ai.output.messages`. SHOULD only be used when distinguishable from `invoke_agent` (e.g. CrewAI crews yes; ADK workflow-agents report `invoke_agent`). (Fact)
- **Plan span** (`gen_ai.plan.internal`): kind `INTERNAL`, span name `plan {gen_ai.agent.name}`; required `gen_ai.operation.name`; conditionally required `error.type`, `gen_ai.agent.name`. The LLM call generating the plan SHOULD be a child of the plan span; tool/task spans are usually siblings under the same `invoke_agent` span. No `gen_ai.plan.*` attributes exist in the current document. (Fact)

### Metrics

Document: `docs/gen-ai/gen-ai-metrics.md` at https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-metrics.md

All metrics are Histograms; metric-level requirement is **Recommended** for every instrument. (Fact)

**Client metrics:**
- `gen_ai.client.token.usage` — unit `{token}`; required attributes `gen_ai.operation.name`, `gen_ai.provider.name`, `gen_ai.token.type` (well-known values `input` | `output`); conditional `gen_ai.request.model`; recommended `gen_ai.response.model`, `server.address`. MUST report billable token counts when providers distinguish used vs. billable; MAY omit only with an offline-counting opt-in, otherwise MUST NOT report. Buckets `[1, 4, 16, ..., 67108864]`. (Fact)
- `gen_ai.client.operation.duration` — unit `s`; required `gen_ai.operation.name`; conditional `error.type`, `gen_ai.provider.name` (only if the operation calls a GenAI provider), `gen_ai.request.model`, `server.port`; recommended `gen_ai.response.model`, `server.address`. Buckets `[0.01 .. 81.92]`. (Fact)
- `gen_ai.client.operation.time_to_first_chunk` — unit `s`; streaming only; required `gen_ai.operation.name`, `gen_ai.provider.name`; conditional `gen_ai.request.model`; recommended `gen_ai.response.model`, `server.address`. (Fact)
- `gen_ai.client.operation.time_per_output_chunk` — unit `s`; streaming only; time between consecutive chunks; same attribute set as time_to_first_chunk. (Fact)

**Server metrics:**
- `gen_ai.server.request.duration` — unit `s`; required `gen_ai.operation.name`, `gen_ai.provider.name`; conditional `error.type`, `gen_ai.request.model`, `server.port`; recommended `gen_ai.response.model`, `server.address`. (Fact)
- `gen_ai.server.time_to_first_token` — unit `s`; required `gen_ai.operation.name`, `gen_ai.provider.name`; conditional `gen_ai.request.model`; recommended `gen_ai.response.model`, `server.address`; no `error.type` attribute (successful responses only). (Fact)
- `gen_ai.server.time_per_output_token` — unit `s`; time per output token after the first; same attribute set as `time_to_first_token`. (Fact)

**Workflow / agent / tool metrics:**
- `gen_ai.invoke_workflow.duration` — unit `s`; conditional `error.type`, `gen_ai.workflow.name`. (Fact)
- `gen_ai.invoke_agent.duration` — unit `s`; conditional `error.type`, `gen_ai.agent.name`; recommended `gen_ai.request.model`. (Fact)
- `gen_ai.invoke_agent.inference_calls` — unit `{inference_call}`; recommended `gen_ai.agent.name`. (Fact)
- `gen_ai.invoke_agent.tool_calls` — unit `{tool_call}`; recommended `gen_ai.agent.name`. (Fact)
- `gen_ai.execute_tool.duration` — unit `s`; required `gen_ai.tool.name`; conditional `error.type`, `gen_ai.agent.name`; recommended `gen_ai.tool.type`. (Fact)

## Sources

1. OpenTelemetry GenAI semantic conventions repository README — https://github.com/open-telemetry/semantic-conventions-genai/blob/main/README.md
2. GenAI docs index — https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/README.md
3. GenAI spans document — https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-spans.md
4. GenAI agent/framework spans document — https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-agent-spans.md
5. GenAI metrics document — https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-metrics.md
6. Old page redirect stub (opentelemetry.io) — https://opentelemetry.io/docs/specs/semconv/gen-ai/
7. Repo file listing (tree of `docs/gen-ai`, `docs/registry`, `model/`) — https://api.github.com/repos/open-telemetry/semantic-conventions-genai/git/trees/main?recursive=1

Note: pages were fetched from the repo `main` branch on 2026-08-06; content may drift as the conventions are still in Development status.

## Verdict

**Established (fact, primary sources):** The GenAI semantic conventions standardize one core inference span plus operation-specific spans (embeddings, retrieval, fetch_response, memory, execute_tool, create/invoke agent, invoke_workflow, plan), a shared attribute namespace (`gen_ai.*` — operation name, provider, request/response params, token usage, prompt, content) with explicit requirement levels (Required / Conditionally Required / Recommended / Opt-In), and 13 standardized histogram metrics split across client, server, workflow, agent, and tool instruments. The entire convention set is explicitly `Development` status, and the GenAI conventions have been moved out of the main semconv repo into the dedicated `semantic-conventions-genai` repository, with the Schema URL still `TODO`.

**Uncertain:** Precise stability timeline and schema-URL pinning (the conventions are pre-stable and moving); `gen_ai.operation.name` / `gen_ai.provider.name` value lists are "well-known" enumerations that will grow over time, so any fixed list here is a snapshot of `main` on the fetch date.

**What would settle it:** A pinned release/tag of `open-telemetry/semantic-conventions-genai` (currently all development) with a published Schema URL and a `stable`/`experimental` split; provider-specific docs (`docs/gen-ai/openai.md`, `aws-bedrock.md`, `anthropic.md`, `azure-ai-inference.md`, `mcp.md`) for how these attributes map to concrete vendor APIs.
