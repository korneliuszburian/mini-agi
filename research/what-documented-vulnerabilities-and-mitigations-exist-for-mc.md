## Findings

Note: claims are tagged **[fact]** when taken directly from the cited primary source, **[estimate]** for numbers produced by the cited studies themselves, and **[opinion]** for syntheses.

### 1. Official trust model and what is intentionally *not* a vulnerability

- **[fact]** MCP's own security policy documents the trust assumptions: "MCP clients trust MCP servers they connect to"; "Local MCP servers are trusted like any other software you install" and run with the same privileges as the client; the decision to connect rests with the user/administrator. Source: SECURITY.md "Intended Behaviors and Trust Model", `github.com/modelcontextprotocol/modelcontextprotocol/blob/main/SECURITY.md`.
- **[fact]** Command execution to launch STDIO servers, server side effects (filesystem, git, DB, network, system commands), resource access, and LLM-driven tool invocation are declared *intended behaviors*, not reportable vulnerabilities. Source: SECURITY.md (same file).
- **[fact]** Out-of-scope reports include "one stdio peer can crash, hang, exhaust resources of, or otherwise deny service to the other"; the SDK's stdio transport "is not a sandbox", and deployments running stdio servers at reduced privilege are responsible for enforcing isolation. Source: SECURITY.md, "STDIO Transport Trust Boundary".
- **[fact]** In-scope categories are protocol-level flaws, auth/authz bypasses, SDK implementation bugs, sandbox escapes, session hijacking, token theft/leakage, and cross-tenant access. Source: SECURITY.md, "What Remains In Scope".
- **[fact]** `clientInfo`/`serverInfo` are self-reported, unverified, and must not be used for security decisions. Source: MCP Specification 2026-07-28, Basic/Overview, `_meta` section (`modelcontextprotocol.io/specification/2026-07-28/basic/`).
- **[fact]** Tool annotations "**MUST** [be] considered untrusted unless they come from trusted servers". Source: MCP Spec 2026-07-28, Tools page, Data Types/Tool.

### 2. Documented attack classes and mandated mitigations (spec security best practices)

The official Security Best Practices page (`modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices`) documents these attacks and required/suggested countermeasures:

- **Confused Deputy (OAuth proxy)** — a proxy server using a static client ID with a third-party AS, combined with dynamic client registration and consent cookies, lets an attacker steal an authorization code and act as the user. Mitigations (**MUST**): per-client consent stored server-side, exact-string `redirect_uri` validation, cryptographically random single-use `state` set only after consent, `__Host-`-prefixed consent cookies with `Secure`/`HttpOnly`/`SameSite=Lax`, `frame-ancestors` CSP / `X-Frame-Options: DENY` against clickjacking.
- **Token passthrough / audience validation** — servers "**MUST NOT** accept any tokens that were not explicitly issued for the MCP server"; accepting tokens for other resources and forwarding them downstream is forbidden (breaks audit, rate limiting, trust boundaries). Corroborated by the Authorization Security Considerations page, which adds that clients **MUST** send the RFC 8707 `resource` parameter and servers **MUST** validate audience.
- **SSRF during OAuth metadata discovery** — clients fetch `resource_metadata` (from `WWW-Authenticate`), `authorization_servers`, and token/authorization endpoints supplied by a server; a malicious server can point these at `169.254.169.254` cloud metadata, localhost, or private ranges. Mitigations: require HTTPS except loopback in dev, block private/link-local/loopback IP ranges (RFC 9728 §7.7), validate redirect targets, egress proxies (e.g., Smokescreen), DNS-pinning to counter TOCTOU. Authorization servers fetching Client ID Metadata Documents face the same SSRF risk.
- **State handle hijacking** — because MCP is stateless, servers mint state handles returned as tool arguments; "**MUST NOT** treat possession of a state handle as authentication", must verify per-request authorization and bind handles to the authenticated user (`<user_id>:<handle>`), use secure non-deterministic handles, and expire them.
- **Local server compromise** — malicious startup commands (e.g., `npx malicious-package && curl ... ~/.ssh/id_rsa`), payloads in the server, or DNS rebinding against an unauthenticated localhost HTTP server. Mitigations: mandatory pre-execution consent showing the exact untruncated command, sandboxing, least privilege, warn on dangerous patterns (`sudo`, `rm -rf`, network ops); server-side, prefer stdio, or gate HTTP transports behind tokens / restricted unix sockets.
- **OAuth authorization URL validation (XSS/RCE)** — malicious servers can supply `javascript:`/`data:`/`file:` URLs that execute in `window.open()` or in a shell. Mitigations (**MUST**): allow only `http`/`https` (https in production), reject dangerous schemes via allowlist, never open URLs via shell commands, apply CSP (`script-src 'self'`).
- **stdio proxy privilege escalation** — in proxy architectures, XSS → theft of the proxy auth token → arbitrary command spawning via stdio. Mitigation is defense-in-depth (CSP, input sanitization, sandboxing spawned processes, least privilege).
- **Mix-up attacks** — `iss`-based Authorization Response Validation mitigates; "PKCE alone does not prevent this attack" because the client transmits the verifier to the attacker's token endpoint.
- **Localhost redirect URI impersonation** — a CIMD proves domain control, not which local process listens on `localhost`; authorization servers must warn on `localhost`-only redirect URIs.
- **Scope minimization** — poor scope design expands blast radius; guidance: progressive least-privilege scope sets, incremental `WWW-Authenticate scope="..."` step-up challenges, avoid omnibus scopes, server-side authorization logic (never trust claimed scopes alone).

### 3. Transport-level security requirements

- **[fact]** Auth applies to HTTP transports (server **SHOULD** conform); STDIO implementations **SHOULD NOT** use the OAuth spec and instead take credentials from the environment. All authorization-server endpoints must be HTTPS; redirect URIs must be `localhost` or HTTPS; PKCE with `S256` is **MUST** and clients must refuse to proceed if `code_challenge_methods_supported` is absent. Source: Spec 2026-07-28 Basic/Overview ("Auth") and Basic/Authorization/security-considerations.
- **[fact]** Tools security section (Spec 2026-07-28, Tools page): servers **MUST** validate all tool inputs, implement access controls, rate-limit tool invocations, and sanitize tool outputs. Clients **SHOULD** prompt for confirmation on sensitive operations, show tool inputs before calling, validate tool results before passing to the LLM, implement timeouts, and log tool usage for audit.
- **[fact]** Human-in-the-loop: "for trust & safety and security, there **SHOULD** always be a human in the loop with the ability to deny tool invocations", with UI making exposed tools visible and confirmation prompts for operations. Source: Spec 2026-07-28, Tools page, "User Interaction Model".
- **[fact]** Tools list "**MUST NOT** vary per-connection or as a side effect of other requests", but "**MAY** vary by the authorization presented on the request" (i.e., capability exposure can be scope-filtered). Source: Spec 2026-07-28, Tools page.
- **[fact]** JSON Schema `$ref` resolution must not auto-dereference network URIs by default; composition keywords can be a DoS vector and need depth/sub-schema/time bounds. Source: Spec 2026-07-28, Basic/Overview, JSON Schema Usage.
- **[fact]** `x-mcp-header` (tool params mirrored into HTTP headers): constrained to token-safe names, primitive types, and must be rejected if malformed; server devs **SHOULD NOT** mark secrets/PII, since header values are visible to intermediaries. Source: Spec 2026-07-28, Tools page.
- **[fact]** Icon URIs/bytes are untrusted: reject unsafe schemes, fetch without credentials, same-origin checks, magic-byte MIME validation, resource-exhaustion limits (SVG can contain embedded JavaScript). Source: Spec 2026-07-28, Basic/Overview, `icons`.
- **[fact]** Design principle: "Servers should not be able to read the whole conversation, nor 'see into' other servers"; the host enforces security boundaries and isolation between servers. Source: Spec 2026-07-28, Architecture.

### 4. Published CVEs in the official SDKs (first-party advisories)

TypeScript SDK (`@modelcontextprotocol/sdk`, via `api.github.com/repos/modelcontextprotocol/typescript-sdk/security-advisories`):
- **[fact]** CVE-2026-25536 (GHSA-345p-7cg4-v4c7, high, CVSS 7.1): reusing a `StreamableHTTPServerTransport` or a single `McpServer` across clients causes JSON-RPC ID-collision response misrouting / cross-client data leak (CWE-362). Patched in 1.26.0.
- **[fact]** CVE-2026-0621 (GHSA-cqwc-fm46-7fff, high): ReDoS in `UriTemplate.partToRegExp()` on exploded array patterns (`{/id*}`, `{?tags*}`) → 100% CPU / DoS via `resources/read` (CWE-1333). Patched in 1.25.2.
- **[fact]** CVE-2025-66414 (GHSA-w48q-cv73-mx4w, high): DNS-rebinding protection disabled by default for HTTP servers on localhost (CWE-1188). Patched in 1.24.0.

Python SDK (`mcp`, via `api.github.com/repos/modelcontextprotocol/python-sdk/security-advisories`):
- **[fact]** CVE-2026-52870 (GHSA-hvrp-rf83-w775, CVSS 7.6): experimental tasks feature lets any client read/cancel other clients' tasks (CWE-862). Patched in 1.27.2.
- **[fact]** CVE-2026-52869 (GHSA-jpw9-pfvf-9f58, CVSS 7.1): SSE/Streamable HTTP session routing without principal verification — anyone knowing a session ID can inject on it (CWE-639). Patched in 1.27.2.
- **[fact]** CVE-2026-59950 (GHSA-vj7q-gjh5-988w): deprecated WebSocket transport had no Host/Origin validation (CWE-346/CWE-1385); cross-origin pages could drive the server. Patched in 1.28.1.
- **[fact]** CVE-2025-66416 (GHSA-9h52-p55h-vw2f): DNS-rebinding protection disabled by default (CWE-1188). Patched in 1.23.0.
- **[fact]** CVE-2025-53366 (GHSA-3qhf-m339-9g5v, CVSS 8.7) and CVE-2025-53365 (GHSA-j975-95f5-7wqh, CVSS 8.7): unhandled exceptions → server crash / DoS. Patched in 1.9.4 and 1.10.0.

Rust SDK (`rmcp`, via `api.github.com/repos/modelcontextprotocol/rust-sdk/security-advisories`):
- **[fact]** CVE-2026-42559 (GHSA-89vp-x53w-74fx, CVSS 8.8): Streamable HTTP server did not validate `Host` → DNS-rebinding attack against loopback servers (CWE-346/CWE-350). Patched in 1.4.0.
- **[fact]** CVE-2026-63128 (GHSA-9pj6-vhgr-3mwh, CVSS 7.5): unauthenticated session-table memory leak → permanent DoS (~75 GB/day at 2k req/s) (CWE-400/401/772). Patched in 2.0.0.
- **[fact]** CVE-2026-63127 (GHSA-33f5-2c5q-wgwj, CVSS 8.2): missing OAuth Protected Resource `resource` validation lets a malicious server redirect the flow to a legitimate AS and steal tokens. Patched in 2.0.0.
- **[fact]** CVE-2026-64684 (GHSA-9g45-5xwm-f3wc): custom headers (e.g., `X-API-Key`) forwarded to cross-origin redirect targets (CWE-200). Patched in 2.1.0.
- **[fact]** GHSA-c9xm-49cp-xcr9 (no CVE listed): rmcp OAuth client fetches server-controlled `resource_metadata=` URLs without origin/private-network validation → SSRF (CWE-918). Patched in 2.0.0.

### 5. Supply-chain / ecosystem controls

- **[fact]** Official MCP Registry moderation is explicitly permissive: it removes only illegal content, malware, spam, and non-functioning servers; it "**won't** remove … servers with security vulnerabilities", and consumers "should assume minimal-to-no moderation". Source: `modelcontextprotocol.io/registry/moderation-policy`.
- **[fact]** Reference servers in `modelcontextprotocol/servers` are "not … production-ready solutions; developers should evaluate their own security requirements". Source: `github.com/modelcontextprotocol/servers` README.
- **[fact]** SEP-1024 (Final, Standards Track) mandates, for one-click local server installation: a consent dialog showing the exact untruncated command, explicit affirmative approval, and a cancel path — motivated by silent command execution, lack of visibility, and arbitrary code execution via crafted configs. Source: `modelcontextprotocol.io/seps/1024-mcp-client-security-requirements-for-local-server-`.

### 6. Independent research (primary research papers, self-reported measurements)

- **[estimate]** "Breaking the Protocol" (arXiv 2601.17549) reports three architectural MCP weaknesses — absence of capability attestation, bidirectional sampling without origin authentication (enables server-side prompt injection), and implicit trust propagation across multi-server configs — and measures that MCP's design amplifies attack success by 23–41% versus non-MCP integrations; a proposed MCPSec extension cuts measured success from 52.8% to 12.4%.
- **[estimate]** Two studies of 7 MCP clients (Claude Desktop, Claude Code, Cursor, Cline, Continue, Gemini CLI, Langflow) identify **tool poisoning** (malicious instructions embedded in tool metadata/descriptions) as the most prevalent client-side vector, with wide disparities (Claude Desktop "implement[s] strong guardrails", Cursor "exhibits high susceptibility to cross-tool poisoning"). Sources: arXiv 2603.21642, arXiv 2603.22489.
- **[fact]** MPMA (arXiv 2505.11154) documents a "Preference Manipulation Attack": a customized MCP server inserts manipulative words into tool names/descriptions to get the LLM to prefer it (advertising/revenue incentive); a genetic-algorithm variant (GAPMA) adds stealth.
- **[fact]** MCP-38 (arXiv 2603.18063) defines a 38-category MCP threat taxonomy (e.g., tool-description poisoning, indirect prompt injection, parasitic tool chaining, dynamic trust violations) mapped to STRIDE and OWASP LLM/Agentic frameworks. A companion SoK (arXiv 2512.08290) systematizes indirect prompt injection and tool poisoning and surveys defenses (cryptographic provenance, runtime intent verification).
- **[estimate]** MCP-Guard (arXiv 2508.10991) reports a multi-stage detection pipeline (96.01% accuracy on adversarial prompts) and the MCP-ATTACKBENCH benchmark (70,448 samples).
- **[estimate]** A measurement study of 6 MCP marketplaces (arXiv 2509.25292, 8,060 servers) found more than half of listed projects invalid or low-value, plus dependency-monoculture and maintenance risks. An empirical MCPApps study (arXiv 2607.25635, 1,723 apps) found only 37.2% gate tool execution behind a blocking approval step, leaving the LLM able to invoke any enabled tool unconditionally in most apps.

### 7. How agent tools should expose capabilities safely (synthesis of the above)

- **[opinion]** The primary sources converge on: advertise capabilities conservatively and filter tool exposure by the request's authorization/scopes (Spec Tools page); isolate each server so it cannot see the conversation or other servers (Architecture); validate every tool input server-side and sanitize outputs; keep a human in the loop with the ability to deny invocations and clear visual indicators (Tools page); apply least-privilege, progressive scopes and step-up challenges rather than omnibus grants (Security Best Practices); treat all tool metadata/annotations/icons/results as untrusted input (Spec); sandbox server processes and require consent before launching them (SEP-1024, Security Best Practices); and enforce audience-bound tokens, per-request auth, and per-client instance isolation to avoid the classes of bugs the SDK CVEs above exhibit (Authorization Security Considerations + advisories).

## Sources

Official spec, docs, policy:
- MCP Security Best Practices (2026-07-28): `https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices`
- MCP Specification Security Policy: `https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/SECURITY.md`
- MCP Spec 2026-07-28 — Overview (`/specification/2026-07-28/basic/`), Architecture (`/specification/2026-07-28/architecture/`), Tools (`/specification/2026-07-28/server/tools`), stdio (`/specification/2026-07-28/basic/transports/stdio`), Authorization Security Considerations (`/specification/2026-07-28/basic/authorization/security-considerations`)
- MCP Registry Moderation Policy: `https://modelcontextprotocol.io/registry/moderation-policy`
- SEP-1024 (Final): `https://modelcontextprotocol.io/seps/1024-mcp-client-security-requirements-for-local-server-`
- Reference servers README: `https://github.com/modelcontextprotocol/servers`

First-party SDK advisories (GitHub Security Advisories API):
- TS SDK: `https://api.github.com/repos/modelcontextprotocol/typescript-sdk/security-advisories`
- Python SDK: `https://api.github.com/repos/modelcontextprotocol/python-sdk/security-advisories`
- Rust SDK: `https://api.github.com/repos/modelcontextprotocol/rust-sdk/security-advisories`

Research (arXiv):
- 2601.17549 "Breaking the Protocol"; 2603.21642 & 2603.22489 (tool poisoning / client study); 2505.11154 (MPMA); 2603.18063 (MCP-38); 2512.08290 (SoK); 2508.10991 (MCP-Guard); 2509.25292 (measurement study); 2607.25635 (MCPApps study) — all via `https://arxiv.org/abs/<id>`.

## Verdict

**Established:** The MCP project documents its trust model and what is intentionally out of scope (SECURITY.md); the spec and Security Best Practices mandate concrete mitigations for confused-deputy, token passthrough/audience, SSRF, state-handle hijacking, local-server compromise, OAuth URL scheme validation, mix-up, and scope-minimization attacks; the registry explicitly does not moderate for vulnerabilities; and the three official SDKs have 14+ published GHSAs/CVEs (2025–2026) covering cross-client data leaks, DNS-rebinding, ReDoS/DoS, session-principal bypass, header leakage, OAuth metadata token theft, and SSRF. Independent research documents tool-poisoning and indirect prompt injection as the dominant client-side attack classes.

**Uncertain:** (1) How widely the mandated client mitigations are actually implemented across the ecosystem — measurement studies indicate a majority of MCP apps still let the LLM invoke tools without blocking approval; (2) whether the protocol-level weaknesses argued in "Breaking the Protocol" (capability attestation, sampling origin authentication) are accepted by the MCP maintainers or will be addressed in-spec; (3) the current patch status of third-party (non-official) servers/clients is out of scope of the sources I reached and unknown here.

**What would settle it:** A cross-SDK comparison run against the mitigations in the Security Best Practices checklist; the MCP spec's own changelog/SEP history for sampling-origin and capability-attestation proposals; and a CVE-tracker sweep over third-party MCP packages. Note: I could not fully re-verify every advisory's patch-version arithmetic beyond what the GitHub API returned; figures quoted are from the advisory texts.
