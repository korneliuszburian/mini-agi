# Cloudflare's browser offering for AI agents: Browser Run (ex-Browser Rendering)

Research date: 2026-08-09. Sources: Cloudflare primary docs (developers.cloudflare.com) and the
Cloudflare blog announcement. All claims below are **fact** as stated by Cloudflare on
the cited pages on this date (the docs' own "Last updated" stamps are quoted per page).

## Findings

### 1. Identity and rename: Browser Rendering → Browser Run (April 2026)

- **Fact** — On 2026-04-15 Cloudflare renamed Browser Rendering to **Browser Run** and
  shipped: Live View (real-time view of what the agent sees), Human in the Loop,
  a Chrome DevTools Protocol (CDP) endpoint, session recordings, WebMCP support, a
  `/crawl` REST endpoint, and 4x higher concurrency (120 concurrent browsers,
  up from 30). Source: cloudflare.com blog "Browser Run: give your agents a browser",
  published 2026-04-15, updated 2026-07-22.
- **Fact** — Docs still recognize both names: "Browser Run, formerly known as Browser
  Rendering"; API tokens still reference a `Browser Rendering - Edit` permission, and the
  API path is still `.../browser-rendering/...`. Source: developers.cloudflare.com/browser-run/
  (index, "Last updated Apr 21, 2026") and /browser-run/get-started/.

### 2. Deployment model and architecture

- **Fact** — Browser Run runs headless Chrome on Cloudflare's global network; browser
  sessions open "close to users" for low latency and scale up/down on demand, with a
  "global pool of browsers" and "low cold-start time". The product page claims "scale to
  thousands of browsers". Source: developers.cloudflare.com/browser-run/ (index) and
  cloudflare.com/products/browser-rendering (product page).
- **Fact** — Two integration categories:
  - **Quick Actions** — stateless, single HTTP request tasks (screenshot, PDF, markdown,
    crawl, …). Two access modes: the REST API
    (`https://api.cloudflare.com/client/v4/accounts/<accountId>/browser-rendering/<action>`,
    authenticated with a custom API token with `Browser Rendering - Edit`) or a Workers
    **browser binding** (`env.BROWSER.quickAction(...)`, token not needed). The `.quickAction()`
    method requires `compatibility_date` 2026-03-24 or later, and local dev requires
    `npx wrangler dev --remote` or `"remote": true` in the binding (else error
    "The RPC receiver does not implement the method quickAction"). Quick Actions handle the
    session lifecycle automatically. Source: /browser-run/quick-actions/ ("Last updated Jul 20, 2026").
  - **Browser Sessions** — full browser control via Cloudflare's fork of Puppeteer
    (`@cloudflare/puppeteer`), a Playwright fork, raw CDP, or Stagehand. Deployed inside
    Cloudflare Workers (through the `browser` binding + `nodejs_compat` flag) or from any
    external environment via CDP WebSocket. Source: /browser-run/ index + /browser-run/get-started/.
- **Fact** — session state patterns: default = new browser instance per request; reuse via
  `browser.disconnect()` (browser stays alive, reconnected with `puppeteer.connect(...)`),
  or persist a long-lived browser in a **Durable Object** for stateful routing (docs page
  "Browser Run with Durable Objects"). Integration with KV, R2, D1, Queues is documented.
  Source: /browser-run/features/reuse-sessions/ ("Last updated Apr 23, 2026"),
  /browser-run/get-started/.
- **Fact** — sessions are evicted after 60 seconds of inactivity by default; `keep_alive`
  extends it to 10 minutes; there is no fixed maximum session lifetime while active —
  sessions also close on Browser Run releases. Browsers without explicit `browser.close()`
  keep burning browser time until the inactivity timeout. Source: /browser-run/limits/
  FAQ ("Last updated May 20, 2026"), /browser-run/pricing/.

### 3. Official API surface

- **Fact** — Quick Actions REST endpoints (all under `.../browser-rendering/`): `/content`
  (HTML), `/screenshot`, `/pdf`, `/markdown` (Markdown extraction), `/snapshot` (multiple
  page formats — screenshot, html, domSnapshot, …), `/accessibilityTree`, `/scrape` (HTML
  elements by selector), `/json` (AI-extracted structured data via Workers AI), `/links`,
  and `/crawl` (site crawl; **REST only**). Each response carries an `X-Browser-Ms-Used`
  header (browser time in ms) for cost accounting. Source: /browser-run/quick-actions/,
  /browser-run/quick-actions/crawl-endpoint/, /browser-run/quick-actions/json-endpoint/.
- **Fact** — CDP endpoints (`/devtools`): `POST /devtools/browser` creates a session;
  `GET /devtools/browser/{session_id}/json/list` lists tabs; `PUT .../json/new` opens a
  tab; `DELETE .../json/close/{target_id}` closes a tab; `DELETE /devtools/browser/{session_id}`
  closes the session. The WebSocket endpoint is
  `wss://api.cloudflare.com/client/v4/accounts/<ACCOUNT_ID>/browser-rendering/devtools/browser?keep_alive=600000`
  (blog example; `keep_alive` in ms) and works with any CDP client — Puppeteer, Playwright,
  MCP clients, or custom WebSocket implementations, from local machines, CI/CD, or external
  servers. Source: /browser-run/cdp/ ("Last updated May 28, 2026"), Cloudflare blog
  (2026-04-15).
- **Fact** — Puppeteer fork additions ("session management"): `puppeteer.launch(binding)`,
  `puppeteer.connect(binding, sessionId)`, `puppeteer.sessions(binding)` (list live sessions
  incl. `connectionId`), `puppeteer.history()`, `browser.sessionId()`, `browser.disconnect()`,
  `keep_alive`, `waitForTimeout`. Source: /browser-run/puppeteer/ + reuse-sessions page code samples.
- **Fact** — Agent-facing MCP: the Chrome DevTools team's `chrome-devtools-mcp` server can
  point at Browser Run via `--wsEndpoint` + `--wsHeaders` (Authorization Bearer token); the
  blog shows an explicit Claude Desktop config. There is also a Cloudflare Playwright MCP.
  Source: Cloudflare blog (2026-04-15), /browser-run/cdp/mcp-clients/, /browser-run/playwright/playwright-mcp/.
- **Fact** — WebMCP support: Chromium 146+ APIs `navigator.modelContext` (websites register
  tools) and `navigator.modelContextTesting` (agents discover/execute them) — a browser API,
  not a Cloudflare-specific API, but served through Browser Run sessions. Source: Cloudflare
  blog (2026-04-15), /browser-run/features/webmcp.mdx.

### 4. Capabilities (for agents)

- **Fact** — Screenshots, PDFs, rendered-HTML and Markdown extraction, link lists, element
  scraping, accessibility tree, AI structured extraction (`/json`), site-wide crawling with
  depth/limit/pattern controls and incremental mode. Source: quick-actions index + endpoint pages.
- **Fact** — `/crawl` is a "well-behaved crawler": respects `robots.txt` and AI Crawl Control,
  follows site-owner preferences, uses a non-customizable User-Agent, is a cryptographically
  signed agent (Web Bot Auth, distinct bot ID), and does not bypass Cloudflare bot protections
  or CAPTCHAs. Output formats: HTML, Markdown, structured JSON; returns a crawl job ID polled
  for results. Source: Cloudflare blog (2026-04-15), /browser-run/quick-actions/crawl-endpoint/.
- **Fact** — Live View: human can watch and control a running browser session in real time;
  live-view URLs are valid ~5 minutes (doc: "about five minutes"), refreshed via
  `cdp.getLiveViewUrl()`. Intended for login/MFA/CAPTCHA/sensitive-input handoffs.
  Source: /agents/tools/browser/ ("Last updated Jun 24, 2026"), Cloudflare blog.
- **Fact** — Human in the Loop pattern: the agent surfaces a live-view link to the user, makes
  an approval-gated call; the run pauses durably (Code Mode runtime) and resumes against the
  same browser session with tabs and cookies intact. Source: /agents/tools/browser/.
- **Fact** — Session recording: opt-in `recording: true`; sessions are recorded as structured
  rrweb events, retained 30 days, capped at two hours per session, fetched post-close via
  `getBrowserRecording({accountId, apiToken, sessionId})` from the REST API.
  Source: /agents/tools/browser/.
- **Fact** — Cookies/state isolation: docs recommend incognito browser contexts
  (`browser.createBrowserContext()`) to isolate cookies and cache between sessions; host-side
  Quick Action options can pass `cookies`, `authenticate`, `gotoOptions`, `viewport` once per
  request (not exposed to the model). Reused sessions keep their cookies (that is the point
  of reuse, e.g. post-login state), and `dynamic` session promotion lets a run "keep" a
  session after logging in. Source: /browser-run/limits/ FAQ, /agents/tools/browser/.

### 5. Pricing (both Workers plans)

- **Fact** — **Workers Free**: 10 browser-minutes per day ($0 beyond that until upgrade);
  Quick Actions + Sessions share the same browser-hours pool. **Workers Paid**: 10 browser
  hours/month included, then **$0.09 per additional hour**. Browser Sessions additionally
  charge for concurrency: Paid includes 10 concurrent browsers (monthly average of daily
  peaks), then **$2.00 per additional browser-month**. (Free: max 3 concurrent.) Source:
  /browser-run/pricing/ ("Last updated Apr 21, 2026").
- **Fact** — Billing mechanics: usage totaled daily in seconds, summed per month, rounded to
  the nearest whole hour (≥1800s rounds up). Concurrency = monthly average of daily peak
  concurrent browsers. Failed Quick Actions (e.g. `waitForTimeout` errors) are not charged.
  Source: /browser-run/pricing/ FAQ.
- **Fact** — Worked example from the docs: Paid plan, 50 session-hours + 20 concurrent
  browsers for 15 days → $3.60 browser hours + $10.00 concurrency = $13.60/month.
  Source: /browser-run/pricing/.

### 6. Limits

- **Fact** — **Workers Free**: 10 min browser/day; 3 concurrent browsers/account (Sessions
  only); 1 new browser instance every 20 s; 60 s browser inactivity timeout; Quick Actions
  1 request/10 s; `/crawl`: 5 crawl jobs/day, 100 pages per crawl. **Workers Paid**: unlimited
  browser hours (metered); 120 concurrent browsers/account (default; increase on request via
  a form); 1 new instance/s; 60 s timeout (extendable to 10 min via `keep_alive`);
  Quick Actions 10 req/s. Rate-limit errors return HTTP 429 with `Retry-After`.
  Source: /browser-run/limits/ ("Last updated May 20, 2026").
- **Fact** — "Browser time limit exceeded for today" (429) = Workers Free daily cap hit;
  docs recommend `browser.close()` and `puppeteer.history()`/`playwright.history()` to detect
  sessions closed by `BrowserIdle` instead of `NormalClosure` (the #1 cause of surprising usage).
  Source: /browser-run/limits/ Troubleshooting.

### 7. Documented usage patterns as a web-browsing tool for agents

- **Fact** — Cloudflare Agents SDK: `createBrowserTools({ctx, browser, loader})` /
  `createBrowserRuntime` (from `agents/browser/ai`) expose one durable CDP tool
  (`browser_execute` — sandboxed model-written code driving a live session via the `cdp.*`
  namespace: `cdp.send`, `cdp.attachToTarget`, `cdp.spec()` (live normalized protocol spec),
  `cdp.getDebugLog`, `cdp.getLiveViewUrl`, session controls) plus stateless Quick Action tools
  (`browser_markdown`, `browser_extract`, `browser_links`, `browser_scrape`), each bounded by
  `maxChars` to protect the context window. Source: /agents/tools/browser/.
- **Fact** — Agent session modes: `one-shot` (fresh session per execution, deterministic
  teardown), `reuse` (named shared session persisting across executions), `dynamic`
  (starts one-shot; model can promote via `cdp.startSession()`, e.g. after login). Sessions
  live in Durable Object storage, surviving hibernation and approval pauses; a run paused for
  human approval resumes with browser session, tabs, and cookies intact (if Browser Run
  expired the session meanwhile, resume surfaces a clear error and the model restarts).
  Host-side: `connector.sweep()` reclaims stale sessions; `runtime.expirePaused()` rejects
  stale approvals. Source: /agents/tools/browser/.
- **Fact** — MCP-based pattern (any agent): point `chrome-devtools-mcp` at the CDP WebSocket
  with an Authorization header — enables Claude Desktop and other MCP clients to control
  sessions; combined with Live View for human fallback. WebMCP lets sites expose typed tools
  that agents call directly instead of screenshot-analyze-click loops (Cloudflare positions
  this as "replacing fragile screenshot-analyze-click loops with direct function calls").
  Source: Cloudflare blog (2026-04-15), /browser-run/features/webmcp.mdx.
- **Fact** — RAG/scraping pattern: `/crawl` + `/markdown` + `/json` as a one-request content
  ingestion pipeline (Cloudflare docs present the crawl endpoint with depth/limit/patterns,
  incremental "skip unchanged pages", static HTML mode). Good behavior: respects robots.txt
  + AI Crawl Control, no CAPTCHA bypass. Source: /browser-run/quick-actions/crawl-endpoint/,
  Cloudflare blog.
- **Fact** — Local/test pattern: browser bindings accept `"remote": true` for `wrangler dev`
  so local development hits real headless browsers. Source: /browser-run/get-started/.

## Sources

1. https://developers.cloudflare.com/browser-run/ (index; Apr 21, 2026)
2. https://developers.cloudflare.com/browser-run/get-started/ (May 29, 2026)
3. https://developers.cloudflare.com/browser-run/quick-actions/ (Jul 20, 2026)
4. https://developers.cloudflare.com/browser-run/cdp/ (May 28, 2026)
5. https://developers.cloudflare.com/browser-run/pricing/ (Apr 21, 2026)
6. https://developers.cloudflare.com/browser-run/limits/ (May 20, 2026)
7. https://developers.cloudflare.com/browser-run/features/reuse-sessions/ (Apr 23, 2026)
8. https://developers.cloudflare.com/browser-run/puppeteer/, /playwright/, /stagehand/,
   /features/live-view/, /features/session-recording/, /features/webmcp.mdx
9. https://developers.cloudflare.com/agents/tools/browser/ (Jun 24, 2026)
10. https://blog.cloudflare.com/browser-run-for-ai-agents/ (published 2026-04-15,
    updated 2026-07-22)
11. https://www.cloudflare.com/products/browser-rendering (product page)

## Verdict

**Established (fact):** Browser Rendering is now Browser Run (April 2026) — managed headless
Chrome on the Cloudflare edge; two access tiers (stateless Quick Actions REST/binding vs full
CDP/Puppeteer/Playwright sessions); CDP WebSocket + HTTP `/devtools` for external envs; free
tier = 10 browser-min/day (3 concurrent), Paid = 10 h/month then $0.09/h plus $2/browser
concurrency over 10; hard 60 s inactivity timeout extendable to 10 min; well-behaved `/crawl`;
first-party agent patterns (Agents SDK `browser_execute` with durable sessions, MCP via
chrome-devtools-mcp, WebMCP, Live View HITL, rrweb session recordings).

**Uncertain:** The docs' limits/pricing pages are revised frequently (several "Last updated"
stamps within 2026); numbers should be re-checked before budgeting. WebMCP and Live View are
rolling out incrementally (Chromium 146+ requirement). Concurrency "120 default" is a
documented default, and higher limits come via a manual request form (timing/approval unknown —
not documented).

**What would settle remaining questions:** a live account test — run a Paid-plan session for
a day and compare the dashboard usage numbers against `X-Browser-Ms-Used` sums; and a
side-by-side of `chrome-devtools-mcp` behavior for login flows (cookie persistence across
reconnects) against the docs' reuse-session guarantees.