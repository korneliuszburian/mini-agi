//! Live supervision dashboard (D4): a std-only HTTP server in the
//! BINARY crate serving a self-refreshing page over an `api` route.
//!
//! No deps, no async (AGENTS.md / ADR-0012 keep the kernel std-only): one
//! `TcpListener`, one thread per connection, HTTP/1.1 with
//! Content-Length. The page polls every 2.5s, so it is LIVE without
//! manual refresh; writes stay in the terminal/MCP (HITL) — the page
//! offers copy-the-command affordances first, click-to-run second.
//!
//! Design (2026-08 redesign): Rust OWNS the evidence — run verification
//! state, journal pairing, worker/workdir classification, staging
//! application receipts, queue resolution, attention policy, and path
//! validation. The browser ONLY renders classified states; it never
//! infers truth, and it formats but never truncates decision content.
//!
//! Security invariants (regression-critical):
//! - every path-bearing action argument passes `plain_path_segment`;
//! - unknown `/api/act/*` routes return 400 without execution or audit;
//! - POST routes require the `X-Mini-Agi-UI: 1` header (no cross-origin
//!   form can drive localhost writes) and the read buffer is sliced to
//!   the ACTUAL read length (the NUL-padding regression never returns);
//! - all dynamic HTML is 5-char escaped; no inline event handlers.
//!
//! The human-review gate (F-011): frontend is the user's domain; this
//! module is the kernel-side seam and ships WITH the user in the loop.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

/// Poll cadence announced to the browser (same as the JS default).
const POLL_AFTER_MS: u64 = 2_500;
/// A signoff queue older than this is a stale human block.
const QUEUE_STALE_MS: u64 = 24 * 60 * 60 * 1_000;
/// The in-process memory integrity snapshot gets stale after this.
const MEMORY_SCAN_STALE_MS: u64 = 5 * 60 * 1_000;
/// Bounded HTTP request envelope (headers + body).
const MAX_HTTP_REQUEST_BYTES: usize = 64 * 1_024;
const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta name="color-scheme" content="dark">
  <title>mini-agi · live supervision</title>
  <style>
:root {
      color-scheme: light;
      --canvas: #FFFFFF;
      --chrome: #EEEBE4;
      --surface-workspace: #FBFAF6;
      --surface-content: #F9F6F1;
      --surface-subtle: #F7F4EF;
      --surface-control: #F2F0EC;
      --surface-chip: #EBE7E3;

      --border-strong: #D9D6D1;
      --border-standard: #E3E0DB;
      --border-card: #D8D8D6;
      --border-soft: #E9E6E1;

      --ink-strong: #181512;
      --ink-primary: #211E1A;
      --ink-secondary: #44413C;
      --ink-tertiary: #63605B;
      --ink-quiet: #83807B;
      --ink-faint: #A6A39E;
      --ink-on-dark: #F7F4EF;
      --button-dark: #14100D;

      --brand: #623533;
      --review: #6A3D38;
      --review-bg: #F8E5E2;
      --approved: #3B4C3A;
      --approved-bg: #E8F2E7;
      --ai: #816648;
      --ai-bg: #F7ECDB;
      --draft: #6E6A69;
      --draft-bg: #EBE7E3;
      --filter-selected-bg: #F3E0DD;
      --filter-selected-ink: #4D3331;
      --proposal-accent: #816C4D;
      --confidence: #364A33;
      --confidence-empty: #BFC1B9;
      --source-green: #41533D;
      --source-amber: #887251;
      --focus: #C9A561;

      --shadow-window:
        0 34px 42px rgb(24 21 18 / 24%),
        0 12px 20px rgb(24 21 18 / 12%),
        0 2px 5px rgb(24 21 18 / 8%);

      --space-1: .25rem;
      --space-2: .5rem;
      --space-3: .75rem;
      --space-4: 1rem;
      --space-5: 1.5rem;
      --space-6: 2rem;
      --space-7: 3rem;
      --text-xs: .75rem;
      --text-sm: .8125rem;
      --text-md: .9375rem;
      --text-lg: 1.125rem;
      --text-xl: clamp(1.35rem, 2vw, 1.7rem);
      --leading-tight: 1.2;
      --leading-snug: 1.35;
      --leading-base: 1.55;
      --radius-s: .375rem;
      --radius-m: .625rem;
      --radius-l: .75rem;
      --measure: 76ch;
      --measure-compact: 58ch;
      --transition-fast: 140ms ease;
      --transition-base: 220ms ease;
      --font-sans: Inter, "Helvetica Neue", Arial, ui-sans-serif, system-ui, sans-serif;
      --font-serif: "Instrument Serif", Georgia, "Iowan Old Style", "Times New Roman", serif;
      --font-mono: "IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    }

    *, *::before, *::after { box-sizing: border-box; }
    html { background: var(--canvas); scroll-behavior: smooth; }
    body {
      margin: 0;
      min-width: 20.625rem;
      min-height: 100vh;
      background: var(--canvas);
      color: var(--ink-primary);
      font-family: var(--font-sans);
      font-size: var(--text-md);
      line-height: var(--leading-base);
      -webkit-font-smoothing: antialiased;
    }
    button, input { font: inherit; }
    button { color: inherit; }
    [hidden] { display: none !important; }
    ::selection { background: var(--review); color: var(--ink-on-dark); }
    :focus-visible { outline: .125rem solid var(--focus); outline-offset: .1875rem; }
    h1, h2, h3, p, dl, dd { margin: 0; }
    h1, h2, h3 { line-height: var(--leading-tight); }
    code, pre, .data { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }
    code { overflow-wrap: anywhere; }

    .wrapper {
      width: min(100%, 98rem);
      margin-inline: auto;
      padding-inline: clamp(1rem, 3vw, 2.5rem);
    }
    .flow > * + * { margin-block-start: var(--flow-space, var(--space-4)); }
    .cluster {
      display: flex;
      flex-wrap: wrap;
      align-items: var(--cluster-align, center);
      gap: var(--cluster-gap, var(--space-2));
    }
    .repel {
      display: flex;
      flex-wrap: wrap;
      align-items: var(--repel-align, center);
      justify-content: space-between;
      gap: var(--repel-gap, var(--space-4));
    }
    .visually-hidden {
      position: absolute;
      width: .0625rem;
      height: .0625rem;
      padding: 0;
      margin: -.0625rem;
      overflow: hidden;
      clip: rect(0 0 0 0);
      white-space: nowrap;
      border: 0;
    }

    .window {
      position: relative;
      max-width: 2000px;
      margin-inline: auto;
      min-height: 100vh;
      background: var(--canvas);
    }
    @media (min-width: 76rem) {
      .window {
        margin: 2.5rem auto;
        min-height: calc(100vh - 5rem);
        border: .0625rem solid var(--border-strong);
        border-radius: 1.5rem;
        overflow: hidden;
        box-shadow: var(--shadow-window);
      }
    }

    .title-bar {
      display: flex;
      align-items: center;
      gap: var(--space-4);
      height: 3.625rem;
      padding-inline: 1.25rem;
      background: var(--chrome);
      border-bottom: .0625rem solid var(--border-strong);
    }
    .traffic { display: flex; gap: .5rem; }
    .traffic span { width: .8125rem; height: .8125rem; border-radius: 50%; }
    .traffic span:nth-child(1) { background: #E86869; }
    .traffic span:nth-child(2) { background: #F3C93B; }
    .traffic span:nth-child(3) { background: #67C667; }
    .title-bar__label {
      position: absolute;
      left: 50%;
      transform: translateX(-50%);
      font-size: 1rem;
      font-weight: 600;
      letter-spacing: -.01em;
      color: var(--ink-primary);
    }
    .title-bar__label span { font-weight: 400; color: var(--ink-tertiary); }
    .title-bar__status { margin-left: auto; }

    .connection {
      display: inline-flex;
      align-items: center;
      gap: var(--space-2);
      min-height: 1.75rem;
      padding-inline: .625rem .75rem;
      border: .0625rem solid var(--border-card);
      border-radius: 999rem;
      color: var(--ink-quiet);
      background: var(--canvas);
      font-family: var(--font-mono);
      font-size: .6875rem;
      font-weight: 550;
      letter-spacing: .08em;
      text-transform: uppercase;
    }
    .connection::before { content: ""; width: .4375rem; height: .4375rem; border-radius: 50%; background: currentColor; }
    .connection[data-state="live"] { color: var(--approved); border-color: color-mix(in srgb, var(--approved), transparent 55%); background: var(--approved-bg); }
    .connection[data-state="retrying"] { color: var(--source-amber); border-color: color-mix(in srgb, var(--source-amber), transparent 55%); background: var(--ai-bg); }
    .connection[data-state="stale"] { color: var(--review); border-color: color-mix(in srgb, var(--review), transparent 55%); background: var(--review-bg); }

    .window-body {
      display: grid;
      grid-template-columns: 19rem minmax(0, 1fr);
      min-height: calc(100vh - 3.625rem);
    }

    .sidebar {
      display: flex;
      flex-direction: column;
      gap: var(--space-5);
      padding: var(--space-5) 1.25rem 0;
      background: var(--surface-workspace);
      border-right: .0625rem solid var(--border-strong);
    }
    .brand { display: flex; align-items: center; gap: .75rem; padding-block: .5rem; }
    .brand__mark {
      display: grid;
      place-items: center;
      width: 2.375rem;
      height: 2.375rem;
      border-radius: 50%;
      background: var(--brand);
      color: var(--ink-on-dark);
      font-family: var(--font-serif);
      font-style: italic;
      font-size: 1.25rem;
    }
    .brand__name { font-size: 1.125rem; font-weight: 650; letter-spacing: -.015em; color: var(--ink-strong); }
    .brand__role { margin-block-start: .125rem; font-family: var(--font-mono); font-size: .6875rem; font-weight: 500; letter-spacing: .12em; text-transform: uppercase; color: var(--ink-quiet); }

    .sidebar__group { display: flex; flex-direction: column; gap: var(--space-1); }
    .sidebar__heading {
      padding-inline: .75rem;
      margin-block-end: .375rem;
      font-family: var(--font-mono);
      font-size: .75rem;
      font-weight: 500;
      letter-spacing: .14em;
      text-transform: uppercase;
      color: var(--ink-quiet);
    }
    .nav-row {
      display: flex;
      align-items: center;
      gap: .8125rem;
      min-height: 2.5rem;
      padding: .375rem .75rem;
      border-radius: var(--radius-m);
      color: var(--ink-secondary);
      font-size: 1rem;
    }
    .nav-row svg { flex: none; stroke: var(--ink-tertiary); }
    .nav-row[data-state="warn"] svg, .nav-row[data-state="warn"] .nav-row__label { color: var(--source-amber); stroke: var(--source-amber); }
    .nav-row__count {
      margin-left: auto;
      padding: .125rem .375rem;
      border-radius: .3125rem;
      background: var(--surface-chip);
      font-family: var(--font-mono);
      font-size: .75rem;
      font-variant-numeric: tabular-nums;
      color: var(--ink-tertiary);
    }
    .source-dot { width: .5rem; height: .5rem; border-radius: 50%; background: var(--source-green); flex: none; }
    .source-dot[data-state="amber"] { background: var(--source-amber); }
    .source-dot[data-state="off"] { background: var(--confidence-empty); }
    .source-dot[data-state="bad"] { background: var(--review); }

    .sidebar__foot {
      display: flex;
      align-items: center;
      gap: .75rem;
      margin-top: auto;
      padding-block: .875rem;
      border-top: .0625rem solid var(--border-soft);
    }
    .sidebar__avatar {
      display: grid;
      place-items: center;
      width: 2.375rem;
      height: 2.375rem;
      border: .0625rem solid var(--border-card);
      border-radius: 50%;
      background: var(--chrome);
      color: var(--ink-tertiary);
      font-family: var(--font-serif);
      font-style: italic;
      font-size: 1.125rem;
    }
    .sidebar__name { font-size: 1rem; font-weight: 600; color: var(--ink-primary); }
    .sidebar__role { margin-block-start: .0625rem; font-family: var(--font-mono); font-size: .625rem; font-weight: 500; letter-spacing: .12em; text-transform: uppercase; color: var(--ink-quiet); }
    .sidebar__foot svg { margin-left: auto; stroke: var(--ink-quiet); }

    .workspace {
      display: flex;
      flex-direction: column;
      min-width: 0;
      background: var(--surface-content);
    }
    .toolbar {
      display: flex;
      align-items: center;
      gap: var(--space-4);
      min-height: 3.5rem;
      padding-inline: clamp(1.25rem, 2.5vw, 2.25rem);
      background: var(--surface-content);
      border-bottom: .0625rem solid var(--border-strong);
    }
    .breadcrumb { display: flex; align-items: baseline; gap: .5rem; font-size: 1rem; white-space: nowrap; }
    .breadcrumb__part { color: var(--ink-quiet); }
    .breadcrumb__sep { color: var(--ink-faint); font-size: .75rem; }
    .breadcrumb__current { font-weight: 600; color: var(--ink-strong); }

    .status-strip {
      display: flex;
      flex-wrap: wrap;
      gap: .375rem 1.25rem;
      margin-left: auto;
      color: var(--ink-quiet);
      font-family: var(--font-mono);
      font-size: .8125rem;
      font-variant-numeric: tabular-nums;
    }
    .metric { display: flex; align-items: baseline; gap: .375rem; }
    .metric dt { font-size: .75rem; letter-spacing: .06em; text-transform: uppercase; }
    .metric dd { font-weight: 600; color: var(--ink-primary); }
    .metric__detail { font-weight: 400; color: var(--ink-quiet); }

    .page-head {
      padding: 1.5rem clamp(1.25rem, 2.5vw, 2.25rem) 1.25rem;
      background: var(--surface-content);
      border-bottom: .0625rem solid var(--border-standard);
    }
    .page-head__title {
      font-family: var(--font-serif);
      font-style: italic;
      font-weight: 400;
      font-size: clamp(1.75rem, 3vw, 2.25rem);
      letter-spacing: -.025em;
      line-height: 1.15;
      color: var(--ink-strong);
    }
    .page-head__title[data-state="critical"] { color: var(--review); }
    .page-head__title[data-state="warning"], .page-head__title[data-state="warn"] { color: var(--source-amber); }
    .page-head__title[data-state="ok"] { color: var(--approved); }
    .page-head__title[data-state="unknown"] { color: var(--ink-quiet); }
    .page-head__dial { width: .625rem; height: .625rem; border-radius: 50%; background: var(--ink-quiet); }
    .page-head__dial[data-state="critical"] { background: var(--review); }
    .page-head__dial[data-state="warning"], .page-head__dial[data-state="warn"] { background: var(--source-amber); }
    .page-head__dial[data-state="ok"] { background: var(--approved); }
    .page-head__summary {
      margin-block-start: .375rem;
      font-family: var(--font-mono);
      font-size: .8125rem;
      letter-spacing: .05em;
      color: var(--ink-quiet);
    }
    .page-head__filters { margin-block-start: 1rem; gap: .5rem; }
    .pill {
      display: inline-flex;
      align-items: center;
      gap: .375rem;
      min-height: 2.0625rem;
      padding-inline: .875rem;
      border: .0625rem solid var(--border-card);
      border-radius: 999rem;
      background: var(--surface-content);
      color: var(--ink-secondary);
      font-size: .875rem;
      font-weight: 450;
      font-variant-numeric: tabular-nums;
    }
    .pill[data-state="review"] {
      background: var(--filter-selected-bg);
      border-color: transparent;
      color: var(--filter-selected-ink);
    }
    .pill[data-state="ok"] { color: var(--approved); }
    .pill strong { font-weight: 650; }

    .content {
      display: grid;
      grid-template-columns: minmax(0, 7fr) minmax(22rem, 4fr);
      flex: 1;
    }
    .master { min-width: 0; padding: 1.5rem clamp(1.25rem, 2.5vw, 2.25rem); display: flex; flex-direction: column; gap: var(--space-5); }
    .detail {
      min-width: 0;
      padding: 1.5rem clamp(1.25rem, 2.5vw, 2.25rem);
      display: flex;
      flex-direction: column;
      gap: var(--space-5);
      background: var(--surface-workspace);
      border-left: .0625rem solid var(--border-strong);
    }

    .panel {
      min-width: 0;
      display: flex;
      flex-direction: column;
      gap: var(--space-4);
      padding: 1.25rem 1.5rem;
      border: .0625rem solid var(--border-card);
      border-radius: var(--radius-l);
      background: var(--canvas);
      scroll-margin-top: 4.5rem;
    }
    .panel__head { --repel-align: start; }
    .panel__eyebrow {
      color: var(--ink-tertiary);
      font-family: var(--font-mono);
      font-size: .75rem;
      font-weight: 500;
      letter-spacing: .14em;
      text-transform: uppercase;
    }
    .panel__title {
      margin-block-start: .25rem;
      font-family: var(--font-serif);
      font-style: italic;
      font-weight: 400;
      font-size: var(--text-xl);
      letter-spacing: -.02em;
      color: var(--ink-strong);
    }
    .panel__summary { align-self: end; color: var(--ink-quiet); font-family: var(--font-mono); font-size: .75rem; }
    .empty { padding-block: var(--space-3); color: var(--ink-quiet); font-family: var(--font-mono); font-size: var(--text-sm); }

    .table-shell { overflow-x: auto; }
    table { width: 100%; border-collapse: collapse; }
    th, td { padding: .75rem 1rem; border-bottom: .0625rem solid var(--border-standard); text-align: left; vertical-align: top; }
    tr:last-child td { border-bottom: 0; }
    th {
      color: var(--ink-quiet);
      font-family: var(--font-mono);
      font-size: .6875rem;
      font-weight: 500;
      letter-spacing: .1em;
      text-transform: uppercase;
      white-space: nowrap;
    }
    td[data-align="right"], th[data-align="right"] { text-align: right; font-variant-numeric: tabular-nums; }
    tbody tr { transition: background var(--transition-fast); }
    tbody tr:hover { background: var(--surface-workspace); }
    tbody tr[data-state="critical"] { box-shadow: inset .25rem 0 0 var(--review); }
    tbody tr[data-state="warning"] { box-shadow: inset .25rem 0 0 var(--source-amber); }

    .badge {
      display: inline-flex;
      align-items: center;
      gap: .25rem;
      min-height: 1.5rem;
      padding-inline: .5rem;
      border-radius: .3125rem;
      color: var(--draft);
      background: var(--draft-bg);
      font-family: var(--font-mono);
      font-size: .6875rem;
      font-weight: 550;
      letter-spacing: .08em;
      text-transform: uppercase;
      white-space: nowrap;
    }
    .badge::before { content: ""; width: .375rem; height: .375rem; border-radius: 50%; background: currentColor; }
    .badge[data-state="ok"], .badge[data-state="verified"], .badge[data-state="finished"], .badge[data-state="resolved_pass"], .badge[data-state="approved"] { color: var(--approved); background: var(--approved-bg); }
    .badge[data-state="warn"], .badge[data-state="warning"], .badge[data-state="required"], .badge[data-state="pending"], .badge[data-state="working"], .badge[data-state="in_progress"], .badge[data-state="ai"] { color: var(--ai); background: var(--ai-bg); }
    .badge[data-state="bad"], .badge[data-state="critical"], .badge[data-state="disagrees"], .badge[data-state="crashed"], .badge[data-state="anomaly"], .badge[data-state="resolved_fail"] { color: var(--review); background: var(--review-bg); }
    .badge[data-state="info"], .badge[data-state="unverified"] { color: var(--ink-tertiary); background: var(--surface-chip); }

    .button {
      min-height: 2.375rem;
      padding: .4375rem .875rem;
      border: .0625rem solid var(--border-card);
      border-radius: .5rem;
      background: var(--canvas);
      color: var(--ink-secondary);
      cursor: pointer;
      font-family: var(--font-sans);
      font-size: .9375rem;
      font-weight: 500;
      transition: border-color var(--transition-fast), color var(--transition-fast), background var(--transition-fast), transform var(--transition-fast);
    }
    .button:hover { border-color: var(--ink-tertiary); color: var(--ink-strong); }
    .button:active { transform: translateY(.0625rem); }
    .button:disabled { cursor: wait; opacity: .55; }
    .button[data-variant="primary"], .button[data-variant="danger"] {
      border-color: var(--button-dark);
      background: var(--button-dark);
      color: var(--ink-on-dark);
    }
    .button[data-variant="primary"]:hover, .button[data-variant="danger"]:hover { background: var(--ink-strong); color: var(--ink-on-dark); }
    .button[data-variant="quiet"] {
      min-height: 1.625rem;
      padding-inline: .375rem;
      border-color: transparent;
      background: transparent;
      color: var(--ink-tertiary);
      font-family: var(--font-mono);
      font-size: .75rem;
      font-weight: 500;
    }
    .button[data-variant="quiet"]:hover { color: var(--ink-strong); border-color: transparent; }

    .command {
      --cluster-gap: var(--space-2);
      margin-block-start: var(--space-3);
      padding: var(--space-3);
      border: .0625rem solid var(--border-soft);
      border-radius: var(--radius-s);
      background: var(--surface-subtle);
    }
    .command code { flex: 1 1 22rem; min-width: 0; color: var(--ink-primary); font-size: .75rem; }
    .action-log { min-height: 1.4rem; color: var(--ink-quiet); font-family: var(--font-mono); font-size: .75rem; }
    .action-log[data-state="ok"] { color: var(--approved); }
    .action-log[data-state="bad"] { color: var(--review); }

    details { border-top: .0625rem solid var(--border-standard); }
    details:first-child { border-top: 0; }
    summary {
      display: flex;
      align-items: baseline;
      gap: var(--space-2);
      padding-block: var(--space-3);
      cursor: pointer;
      color: var(--ink-secondary);
      list-style-position: outside;
      transition: color var(--transition-fast);
    }
    summary:hover { color: var(--ink-strong); }
    summary::marker { color: var(--ink-faint); }
    .detail__body { padding: 0 0 var(--space-4) var(--space-4); color: var(--ink-secondary); }
    .detail__body p { max-width: var(--measure); white-space: pre-wrap; }
    .detail__meta { display: grid; grid-template-columns: minmax(7rem, auto) minmax(0, 1fr); gap: var(--space-1) var(--space-3); }
    .detail__meta dt { color: var(--ink-quiet); }
    .detail__meta dd { min-width: 0; overflow-wrap: anywhere; font-family: var(--font-mono); color: var(--ink-primary); }
    .row-title { font-weight: 700; color: var(--ink-strong); }
    .row-subtitle { color: var(--ink-quiet); font-size: .75rem; }
    .score { font-family: var(--font-mono); font-variant-numeric: tabular-nums; font-weight: 650; color: var(--ink-primary); }
    .score[data-state="bad"] { color: var(--review); }
    .score[data-state="warn"] { color: var(--source-amber); }
    .score[data-state="ok"] { color: var(--approved); }
    .progress-track {
      width: 100%;
      min-width: 8rem;
      height: .875rem;
      border-radius: .1875rem;
      background: repeating-linear-gradient(90deg, var(--confidence-empty) 0 .3125rem, transparent .3125rem .5rem);
    }
    .progress-track__fill {
      display: block;
      height: 100%;
      width: 0;
      max-width: 100%;
      border-radius: .1875rem;
      background: repeating-linear-gradient(90deg, var(--confidence) 0 .3125rem, transparent .3125rem .5rem);
    }
    .progress-track[data-progress-band="1"] .progress-track__fill { width: 10%; }
    .progress-track[data-progress-band="2"] .progress-track__fill { width: 20%; }
    .progress-track[data-progress-band="3"] .progress-track__fill { width: 30%; }
    .progress-track[data-progress-band="4"] .progress-track__fill { width: 40%; }
    .progress-track[data-progress-band="5"] .progress-track__fill { width: 50%; }
    .progress-track[data-progress-band="6"] .progress-track__fill { width: 60%; }
    .progress-track[data-progress-band="7"] .progress-track__fill { width: 70%; }
    .progress-track[data-progress-band="8"] .progress-track__fill { width: 80%; }
    .progress-track[data-progress-band="9"] .progress-track__fill { width: 90%; }
    .progress-track[data-progress-band="10"] .progress-track__fill { width: 100%; }
    .progress-track[data-state="bad"] .progress-track__fill { background: repeating-linear-gradient(90deg, var(--review) 0 .3125rem, transparent .3125rem .5rem); }
    .progress-track[data-state="warn"] .progress-track__fill { background: repeating-linear-gradient(90deg, var(--source-amber) 0 .3125rem, transparent .3125rem .5rem); }

    .stack { display: flex; flex-direction: column; }
    .stack__item { padding: var(--space-4); border-top: .0625rem solid var(--border-standard); }
    .stack__item:first-child { border-top: 0; }
    .stack__item[data-state="critical"] { box-shadow: inset .25rem 0 0 var(--review); }
    .stack__item[data-state="warning"] { box-shadow: inset .25rem 0 0 var(--source-amber); }
    .candidate { padding-block: var(--space-3); border-top: .0625rem dashed var(--border-card); }
    .candidate:first-child { border-top: 0; }
    .candidate__body { margin-block-start: var(--space-2); max-width: var(--measure); white-space: pre-wrap; color: var(--ink-primary); }
    .candidate__reason { margin-block-start: var(--space-2); color: var(--ink-quiet); white-space: pre-wrap; }

    .attention {
      padding: 1.25rem;
      border: .0625rem solid var(--border-card);
      border-left: .25rem solid var(--proposal-accent);
      border-radius: var(--radius-l);
      background: var(--canvas);
      box-shadow: none;
    }
    .attention[data-state="quiet"] { border-left-color: var(--approved); background: var(--surface-content); }
    .attention__heading {
      font-family: var(--font-mono);
      font-size: .75rem;
      font-weight: 550;
      letter-spacing: .14em;
      text-transform: uppercase;
      color: var(--ink-tertiary);
    }
    .attention[data-state="quiet"] .attention__heading { color: var(--ink-quiet); }
    .attention__count { color: var(--ink-quiet); font-family: var(--font-mono); font-size: .75rem; }
    .attention__list { margin: var(--space-4) 0 0; padding: 0; list-style: none; counter-reset: attention; }
    .attention__item {
      counter-increment: attention;
      display: grid;
      grid-template-columns: 2rem minmax(0, 1fr);
      gap: var(--space-3);
      padding-block: var(--space-4);
      border-top: .0625rem solid var(--border-standard);
    }
    .attention__item::before {
      content: counter(attention, decimal-leading-zero);
      color: var(--ink-faint);
      font-family: var(--font-mono);
      font-size: .75rem;
      padding-block-start: .125rem;
    }
    .attention__item[data-severity="critical"] .attention__title { color: var(--review); }
    .attention__item[data-severity="warning"] .attention__title { color: var(--source-amber); }
    .attention__item[data-severity="info"] .attention__title { color: var(--ink-tertiary); }
    .attention__title { font-weight: 700; color: var(--ink-strong); }
    .attention__detail { margin-block-start: var(--space-1); max-width: var(--measure); color: var(--ink-secondary); }

    .journal-event { display: grid; grid-template-columns: minmax(7rem, auto) minmax(0, 1fr) auto; gap: var(--space-3); align-items: baseline; }
    .journal-event__time { color: var(--ink-quiet); font-family: var(--font-mono); font-size: .75rem; }
    .journal-event__label { min-width: 0; overflow-wrap: anywhere; font-family: var(--font-mono); color: var(--ink-primary); }

    .chat-layout { display: grid; grid-template-columns: minmax(16rem, 22rem) minmax(0, 1fr); gap: var(--space-5); }
    .thread-list { max-height: 28rem; overflow-y: auto; display: flex; flex-direction: column; gap: var(--space-2); }
    .thread-list .button { width: 100%; justify-content: flex-start; text-align: left; }
    .chat-log {
      display: flex;
      flex-direction: column;
      gap: var(--space-2);
      min-height: 16rem;
      max-height: 32rem;
      overflow-y: auto;
      padding: var(--space-4);
      border: .0625rem solid var(--border-card);
      border-radius: var(--radius-m);
      background: var(--surface-content);
    }
    .chat-message {
      max-width: var(--measure);
      padding: .625rem .75rem;
      border: .0625rem solid var(--border-soft);
      border-radius: var(--radius-m);
      background: var(--canvas);
      align-self: flex-start;
    }
    .chat-message[data-role="human"] { align-self: flex-end; background: var(--ai-bg); border-color: color-mix(in srgb, var(--ai), transparent 60%); }
    .chat-message__role {
      color: var(--ai);
      font-family: var(--font-mono);
      font-size: .6875rem;
      font-weight: 550;
      letter-spacing: .1em;
      text-transform: uppercase;
    }
    .chat-message[data-role="human"] .chat-message__role { color: var(--source-amber); }
    .chat-message__text { margin-block-start: var(--space-1); white-space: pre-wrap; overflow-wrap: anywhere; color: var(--ink-primary); }
    .chat-message__time { margin-block-start: var(--space-1); color: var(--ink-quiet); font-family: var(--font-mono); font-size: .75rem; }
    .chat-form { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--space-2); margin-block-start: var(--space-3); }
    .chat-form input {
      min-width: 0;
      padding: .625rem .875rem;
      border: .0625rem solid var(--border-card);
      border-radius: .625rem;
      background: var(--canvas);
      color: var(--ink-primary);
      transition: border-color var(--transition-fast);
    }
    .chat-form input:focus { border-color: var(--focus); outline: none; }
    .chat-form input::placeholder { color: var(--ink-faint); }

    .boot {
      min-height: 14rem;
      display: grid;
      place-items: center;
      color: var(--ink-quiet);
      font-family: var(--font-mono);
    }
    .boot[data-state="error"] { color: var(--review); }

    .site-foot {
      padding-block: 1.25rem clamp(1.25rem, 2.5vw, 2.25rem);
      color: var(--ink-faint);
      font-family: var(--font-mono);
      font-size: .75rem;
    }

    @media (max-width: 72rem) {
      .window-body { grid-template-columns: 15rem minmax(0, 1fr); }
      .content { grid-template-columns: 1fr; }
      .detail { border-left: 0; border-top: .0625rem solid var(--border-strong); }
    }
    @media (max-width: 52rem) {
      .window-body { grid-template-columns: 1fr; }
      .sidebar { display: none; }
      .title-bar__label { display: none; }
      .chat-layout { grid-template-columns: 1fr; }
      .status-strip { display: none; }
      th[data-optional], td[data-optional] { display: none; }
      .journal-event { grid-template-columns: 1fr auto; }
      .journal-event__time { grid-column: 1 / -1; }
      .chat-form { grid-template-columns: 1fr; }
    }
    @media (prefers-reduced-motion: reduce) {
      *, *::before, *::after { scroll-behavior: auto !important; transition-duration: .001ms !important; }
    }
    .nav-row[data-scroll] { cursor: pointer; }

  </style>
</head>
<body>
<div class="window">
    <div class="title-bar">
      <div class="traffic" aria-hidden="true"><span></span><span></span><span></span></div>
      <p class="title-bar__label">mini-agi <span>· live supervision</span></p>
      <div class="title-bar__status cluster">
        <span id="kernel-badge" class="badge" data-state="info">kernel unknown</span>
        <span id="connection" class="connection" data-state="connecting">CONNECTING</span>
      </div>
    </div>

    <div class="window-body">
      <aside class="sidebar" aria-label="Navigation">
        <div class="brand">
          <span class="brand__mark" aria-hidden="true">μ</span>
          <div>
            <p class="brand__name">mini-agi</p>
            <p class="brand__role">Memory kernel</p>
          </div>
        </div>

        <div class="sidebar__group">
          <p class="sidebar__heading">Memory core</p>
          <div class="nav-row" data-scroll="panel-gaps">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg>
            <span class="nav-row__label">All runs</span>
            <span class="nav-row__count" id="nav-entries">0</span>
          </div>
          <div class="nav-row" data-scroll="panel-gaps">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10.3 3.9a1.8 1.8 0 0 1 3.4 0l1.2 3.6a1.8 1.8 0 0 0 1.1 1.1l3.6 1.2a1.8 1.8 0 0 1 0 3.4l-3.6 1.2a1.8 1.8 0 0 0-1.1 1.1l-1.2 3.6a1.8 1.8 0 0 1-3.4 0l-1.2-3.6a1.8 1.8 0 0 0-1.1-1.1l-3.6-1.2a1.8 1.8 0 0 1 0-3.4l3.6-1.2a1.8 1.8 0 0 0 1.1-1.1z"/></svg>
            <span class="nav-row__label">Pending review</span>
            <span class="nav-row__count" id="nav-pending">0</span>
          </div>
          <div class="nav-row" data-scroll="panel-gaps">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1"/><circle cx="12" cy="12" r="3.5"/></svg>
            <span class="nav-row__label">Open gaps</span>
            <span class="nav-row__count" id="nav-gaps">0</span>
          </div>
        </div>

        <div class="sidebar__group">
          <p class="sidebar__heading">Sources</p>
          <div class="nav-row">
            <span class="source-dot" id="dot-repo" data-state="off" aria-hidden="true"></span>
            <span class="nav-row__label">Repository</span>
            <span class="nav-row__count" id="nav-repo">—</span>
          </div>
          <div class="nav-row">
            <span class="source-dot" id="dot-workers" data-state="off" aria-hidden="true"></span>
            <span class="nav-row__label">Workers</span>
            <span class="nav-row__count" id="nav-workers">—</span>
          </div>
          <div class="nav-row">
            <span class="source-dot" id="dot-heartbeat" data-state="off" aria-hidden="true"></span>
            <span class="nav-row__label">Verifier heartbeat</span>
            <span class="nav-row__count" id="nav-heartbeat">—</span>
          </div>
        </div>

        <div class="sidebar__foot">
          <span class="sidebar__avatar" aria-hidden="true">μ</span>
          <div>
            <p class="sidebar__name">mini-agi</p>
            <p class="sidebar__role">Local kernel</p>
          </div>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>
        </div>
      </aside>

      <div class="workspace">
        <div class="toolbar">
          <nav class="breadcrumb" aria-label="Breadcrumb">
            <span class="breadcrumb__part">Kernel</span>
            <span class="breadcrumb__sep" aria-hidden="true">⁄</span>
            <span class="breadcrumb__current">Live supervision</span>
          </nav>
          <dl id="status-strip" class="status-strip" hidden>
            <div class="metric"><dt>Runs</dt><dd id="metric-runs">0</dd></div>
            <div class="metric"><dt>Verified / claimed</dt><dd><span id="metric-verified">0</span> <span class="metric__detail">/ <span id="metric-claimed">0</span></span></dd></div>
            <div class="metric"><dt>Cost</dt><dd id="metric-cost">$0.0000</dd></div>
            <div class="metric"><dt>Tokens</dt><dd id="metric-tokens">0</dd></div>
            <div class="metric"><dt>Heartbeat</dt><dd id="metric-heartbeat">never</dd></div>
            <div class="metric"><dt>Sync</dt><dd><time id="metric-sync" data-relative-ms="0">—</time></dd></div>
          </dl>
        </div>

        <div class="page-head">
          <div class="repel">
            <div>
              <h1 id="hero-state" class="page-head__title" data-state="unknown">Awaiting kernel</h1>
              <p id="hero-summary" class="page-head__summary">Connecting to the local kernel…</p>
            </div>
            <div class="cluster">
              <span id="hero-dial" class="page-head__dial" data-state="unknown" aria-hidden="true"></span>
            </div>
          </div>
          <div class="page-head__filters cluster">
            <span class="pill" data-state="review">Needs review (<strong id="pill-review">0</strong>)</span>
            <span class="pill" data-state="ok">Verified (<strong id="pill-verified">0</strong>)</span>
            <span class="pill">Pending (<strong id="pill-pending">0</strong>)</span>
          </div>
        </div>

        <div id="boot" class="boot" role="status" aria-live="polite">Connecting to the local kernel…</div>

        <main id="dashboard" class="content" hidden>
          <div class="master">
            <div id="panel-gaps" class="panel" role="region" aria-labelledby="heading-gaps">
              <div class="panel__head repel">
                <div><p class="panel__eyebrow">Critical path</p><h2 id="heading-gaps" class="panel__title">Loop &amp; Gaps</h2></div>
                <p id="summary-gaps" class="panel__summary"></p>
              </div>
              <div id="gaps"></div>
            </div>

            <div id="panel-runs" class="panel" role="region" aria-labelledby="heading-runs">
              <div class="panel__head repel">
                <div><p class="panel__eyebrow">Evidence</p><h2 id="heading-runs" class="panel__title">Runs &amp; Verification</h2></div>
                <p id="summary-runs" class="panel__summary"></p>
              </div>
              <div id="runs"></div>
            </div>

            <div id="panel-brain" class="panel" role="region" aria-labelledby="heading-brain">
              <div class="panel__head repel">
                <div><p class="panel__eyebrow">Knowledge intake</p><h2 id="heading-brain" class="panel__title">Brain &amp; Staging</h2></div>
                <p id="summary-brain" class="panel__summary"></p>
              </div>
              <div id="brain"></div>
            </div>

            <div id="panel-journal" class="panel" role="region" aria-labelledby="heading-journal">
              <div class="panel__head repel">
                <div><p class="panel__eyebrow">Coherence protection</p><h2 id="heading-journal" class="panel__title">Checkpoint Journal</h2></div>
                <p id="summary-journal" class="panel__summary"></p>
              </div>
              <div id="journal"></div>
            </div>
          </div>

          <div class="detail">
            <div id="attention"></div>

            <div id="panel-memory" class="panel" role="region" aria-labelledby="heading-memory">
              <div class="panel__head repel">
                <div><p class="panel__eyebrow">Canonical truth</p><h2 id="heading-memory" class="panel__title">Memory &amp; Human Queues</h2></div>
                <p id="summary-memory" class="panel__summary"></p>
              </div>
              <div id="memory"></div>
            </div>

            <div id="panel-workers" class="panel" role="region" aria-labelledby="heading-workers">
              <div class="panel__head repel">
                <div><p class="panel__eyebrow">Detached execution</p><h2 id="heading-workers" class="panel__title">Workers</h2></div>
                <p id="summary-workers" class="panel__summary"></p>
              </div>
              <div id="workers"></div>
            </div>

            <div id="panel-repository" class="panel" role="region" aria-labelledby="heading-repository">
              <div class="panel__head repel">
                <div><p class="panel__eyebrow">Execution context</p><h2 id="heading-repository" class="panel__title">Repository</h2></div>
                <p id="summary-repository" class="panel__summary"></p>
              </div>
              <div id="repository"></div>
            </div>

            <div id="panel-tickets" class="panel" role="region" aria-labelledby="heading-tickets">
              <div class="panel__head repel">
                <div><p class="panel__eyebrow">Work graph</p><h2 id="heading-tickets" class="panel__title">Tickets</h2></div>
                <p id="summary-tickets" class="panel__summary"></p>
              </div>
              <div id="tickets"></div>
            </div>
          </div>

          <div class="master">
            <div id="panel-chat" class="panel" role="region" aria-labelledby="heading-chat">
              <div class="panel__head repel">
                <div><p class="panel__eyebrow">Read-only planning agent</p><h2 id="heading-chat" class="panel__title">Kernel Console</h2></div>
                <p class="panel__summary">threads persisted · memory anchored</p>
              </div>
              <div class="chat-layout">
                <div class="flow">
                  <div class="repel"><h3 class="row-title">Threads</h3><button class="button" type="button" data-chat-new>New thread</button></div>
                  <div id="threadbar" class="thread-list flow"></div>
                </div>
                <div>
                  <div id="chatlog" class="chat-log" aria-live="polite"><p class="empty">Select a thread or start a new one.</p></div>
                  <div class="chat-form">
                    <label class="visually-hidden" for="chatinput">Message to the kernel</label>
                    <input id="chatinput" type="text" maxlength="3000" autocomplete="off" placeholder="Ask the read-only, memory-anchored planning agent">
                    <button id="chatsend" class="button" data-variant="primary" type="button">Send</button>
                  </div>
                </div>
              </div>
            </div>

            <p id="action-log" class="action-log" role="status" aria-live="polite"></p>
          </div>
        </main>
      </div>
    </div>
  </div>

  <footer class="site-foot"><div class="wrapper repel"><p>localhost supervision surface</p><p>claims are not proof · verifier evidence is</p></div></footer>

  <script>
  (function () {
    "use strict";

    var POLL_MS = 2500;
    var timer = null;
    var polling = false;
    var failures = 0;
    var firstResponse = false;
    var latest = null;
    var expandedRuns = false;
    var expandedTickets = false;
    var pendingRuns = {};
    var chatThread = null;

    function byId(id) { return document.getElementById(id); }
    function ESC(value) {
      return String(value === null || value === undefined ? "" : value).replace(/[&<>"']/g, function (character) {
        return {"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;","'":"&#39;"}[character];
      });
    }
    function number(value) { return Number(value || 0).toLocaleString("en-US"); }
    function money(value, digits) { return "$" + Number(value || 0).toFixed(digits); }
    function score(value) { return value === null || value === undefined ? "—" : Number(value).toFixed(4); }
    function badge(state, label) { return '<span class="badge" data-state="' + ESC(state) + '">' + ESC(label) + '</span>'; }
    function timeHtml(value, fallback) {
      if (value === null || value === undefined || value === 0) return '<span class="row-subtitle">' + ESC(fallback || "unknown") + '</span>';
      return '<time class="row-subtitle" data-relative-ms="' + ESC(value) + '">' + ESC(fallback || "—") + '</time>';
    }
    function isoTimeHtml(value) {
      if (!value) return '<span class="row-subtitle">never</span>';
      var parsed = Date.parse(value);
      return '<time class="row-subtitle" data-relative-ms="' + ESC(isNaN(parsed) ? 0 : parsed) + '" title="' + ESC(value) + '">' + ESC(value) + '</time>';
    }
    function ageLabel(milliseconds) {
      var delta = Math.max(0, Date.now() - Number(milliseconds || 0));
      var seconds = Math.floor(delta / 1000);
      if (seconds < 10) return "just now";
      if (seconds < 60) return seconds + "s ago";
      var minutes = Math.floor(seconds / 60);
      if (minutes < 60) return minutes + "m ago";
      var hours = Math.floor(minutes / 60);
      if (hours < 48) return hours + "h ago";
      return Math.floor(hours / 24) + "d ago";
    }
    function refreshAges() {
      var nodes = document.querySelectorAll("[data-relative-ms]");
      var index;
      for (index = 0; index < nodes.length; index += 1) {
        var value = Number(nodes[index].getAttribute("data-relative-ms") || 0);
        nodes[index].textContent = value > 0 ? ageLabel(value) : "unknown";
      }
    }
    function keyedNodes(root, selector, keyName) {
      var output = {};
      var nodes = root.querySelectorAll(selector);
      var index;
      for (index = 0; index < nodes.length; index += 1) {
        var key = nodes[index].getAttribute(keyName);
        if (key) output[key] = nodes[index];
      }
      return output;
    }
    function patch(id, html) {
      var node = byId(id);
      if (!node || node.__snapshotHtml === html) return;
      var open = {};
      var details = node.querySelectorAll("details[data-detail-key]");
      var index;
      for (index = 0; index < details.length; index += 1) {
        if (details[index].open) open[details[index].getAttribute("data-detail-key")] = true;
      }
      var focusKey = null;
      if (document.activeElement && node.contains(document.activeElement)) focusKey = document.activeElement.getAttribute("data-focus-key");
      node.innerHTML = html;
      node.__snapshotHtml = html;
      var restored = keyedNodes(node, "details[data-detail-key]", "data-detail-key");
      Object.keys(open).forEach(function (key) { if (restored[key]) restored[key].open = true; });
      if (focusKey) {
        var focusNodes = node.querySelectorAll("[data-focus-key]");
        for (index = 0; index < focusNodes.length; index += 1) {
          if (focusNodes[index].getAttribute("data-focus-key") === focusKey) { focusNodes[index].focus(); break; }
        }
      }
    }
    function setText(id, value) { var node = byId(id); if (node && node.textContent !== String(value)) node.textContent = String(value); }
    function pad3(value) { var text = String(value); while (text.length < 3) text = "0" + text; return text; }
    function closestButton(node) {
      while (node && node !== document) {
        if (node.tagName === "BUTTON") return node;
        node = node.parentNode;
      }
      return null;
    }
    function commandHtml(command, actionPath, confirmation) {
      if (!command) return "";
      var html = '<div class="command cluster"><code>' + ESC(command) + '</code>';
      html += '<button class="button" data-variant="quiet" type="button" data-copy data-command="' + ESC(command) + '">Copy</button>';
      if (actionPath) {
        html += '<button class="button" data-variant="danger" type="button" data-action="' + ESC(actionPath) + '" data-command="' + ESC(command) + '"';
        if (confirmation) html += ' data-confirm="' + ESC(confirmation) + '"';
        html += '>Run</button>';
      }
      return html + "</div>";
    }
    function stateSeverity(state) {
      if (state === "crashed" || state === "disagrees" || state === "anomaly" || state === "receipt_mismatch" || state === "audit_incomplete") return "critical";
      if (state === "working" || state === "required" || state === "pending") return "warning";
      return "";
    }

    function renderHeader(data) {
      setText("metric-runs", number(data.totals.runs));
      setText("metric-verified", number(data.totals.verified_achieved));
      setText("metric-claimed", number(data.totals.claimed_achieved));
      setText("metric-cost", money(data.totals.total_cost_usd, 4));
      setText("metric-tokens", number(data.totals.total_tokens));
      var heartbeat = data.heartbeat || {};
      setText("metric-heartbeat", heartbeat.state === "never" ? "never" : heartbeat.state + " · " + (heartbeat.case || "unknown"));
      var sync = byId("metric-sync");
      sync.setAttribute("data-relative-ms", String(data.generated_at_ms));
      var kernel = byId("kernel-badge");
      kernel.setAttribute("data-state", data.kernel.state === "ok" ? "ok" : data.kernel.state === "critical" ? "bad" : "warn");
      kernel.textContent = "kernel " + data.kernel.state;
      byId("status-strip").hidden = false;
      var hero = byId("hero-state");
      if (hero) {
        var heroState = data.kernel.state;
        hero.setAttribute("data-state", heroState);
        hero.textContent = heroState === "ok" ? "Healthy" : heroState === "critical" ? "Intervention needed" : "Watch";
        var dial = byId("hero-dial");
        if (dial) dial.setAttribute("data-state", heroState);
        var machine = data.kernel.machine || {};
        var summary = data.kernel.summary || "";
        if (machine.verdict && machine.verdict !== "OK") summary = (summary ? summary + " · " : "") + "machine " + machine.verdict;
        setText("hero-summary", summary || "Idle — nobody requires attention.");
      }
    }

    function renderAttention(data) {
      var items = data.attention || [];
      if (!items.length) {
        patch("attention", '<div class="attention" data-state="quiet"><div class="repel"><h2 class="attention__heading">Attention</h2>' + badge("ok", "quiet") + '</div><p class="empty">No human action required.</p></div>');
        return;
      }
      var html = '<div class="attention"><div class="repel"><h2 class="attention__heading">Attention</h2><p class="attention__count">' + number(items.length) + ' item' + (items.length === 1 ? "" : "s") + '</p></div><ol class="attention__list">';
      items.forEach(function (item) {
        html += '<li class="attention__item" data-severity="' + ESC(item.severity) + '"><div><div class="cluster">' + badge(item.severity, item.kind.replace(/_/g, " ")) + '<span class="attention__title">' + ESC(item.title) + '</span></div><p class="attention__detail">' + ESC(item.detail) + '</p>';
        if (item.target_panel) html += '<button class="button" data-variant="quiet" type="button" data-scroll-panel="' + ESC(item.target_panel) + '">Open panel</button>';
        html += commandHtml(item.command, item.execute_path, item.confirmation) + '</div></li>';
      });
      patch("attention", html + "</ol></div>");
    }

    function renderGaps(data) {
      var gaps = data.gaps || [];
      setText("summary-gaps", gaps.length ? gaps.length + " open · target " + Number(data.target).toFixed(2) : "target " + Number(data.target).toFixed(2) + " · quiet");
      if (!gaps.length) { patch("gaps", '<p class="empty">Idle · no open gaps.</p>'); return; }
      var html = '<div class="table-shell"><table><thead><tr><th>Case</th><th data-align="right">Best / target</th><th data-align="right">Distance</th><th>Attempts</th><th data-optional>Owner</th></tr></thead><tbody>';
      gaps.forEach(function (gap) {
        var best = gap.best_composite === null ? gap.composite : gap.best_composite;
        var ratio = data.target > 0 ? Math.max(0, Math.min(100, 100 * best / data.target)) : 0;
        var band = Math.round(ratio / 10);
        var rowState = gap.exhausted || gap.ticket_status === "CLOSED" ? "critical" : (!gap.ticket || !gap.claimant ? "warning" : "");
        html += '<tr data-state="' + rowState + '"><td><div class="row-title">' + ESC(gap.case) + '</div><div class="cluster">' + (gap.ticket_status === "CLOSED" ? badge("bad", "closed ticket") : gap.exhausted ? badge("bad", "exhausted") : badge("info", gap.repair_signal || "gap")) + (gap.ticket ? '<button class="button" data-variant="quiet" type="button" data-scroll-panel="tickets">' + ESC(gap.ticket) + '</button>' : badge("warn", "no ticket")) + '</div></td>';
        html += '<td data-align="right"><span class="score" data-state="' + (gap.delta > 0 ? "warn" : "ok") + '">' + score(best) + ' / ' + score(gap.target) + '</span><div class="progress-track" data-state="' + (gap.exhausted ? "bad" : "warn") + '" data-progress-band="' + band + '"><span class="progress-track__fill"></span></div></td>';
        html += '<td data-align="right" class="score" data-state="' + (gap.exhausted ? "bad" : "warn") + '">−' + score(gap.delta) + '</td>';
        html += '<td class="data">' + number(gap.attempts) + (gap.max_attempts === null ? "" : " / " + number(gap.max_attempts)) + '</td>';
        html += '<td data-optional>' + (gap.claimant ? '<div class="row-title">' + ESC(gap.claimant) + '</div><div class="row-subtitle">' + ESC(gap.ticket_status || "OPEN") + '</div>' : '<span class="row-subtitle">unclaimed</span>') + '</td></tr>';
      });
      patch("gaps", html + "</tbody></table></div>");
    }

    function verificationBadge(row) {
      if (pendingRuns[row.case]) return badge("pending", "pending");
      return badge(row.verification.state, row.verification.state.replace(/_/g, " "));
    }
    function renderRuns(data) {
      var rows = (data.runs.rows || []);
      var shown = expandedRuns ? rows : rows.slice(0, 12);
      setText("summary-runs", data.runs.verified_achieved_runs + " verified · " + data.runs.achieved_runs + " claimed");
      if (!rows.length) { patch("runs", '<p class="empty">Idle · no indexed runs.</p>'); return; }
      var html = '<div class="table-shell"><table><thead><tr><th>Run</th><th>Claim</th><th>Verification</th><th data-align="right">Composite</th><th data-align="right" data-optional>Cost</th><th data-optional>Freshness</th></tr></thead><tbody>';
      shown.forEach(function (row) {
        var critical = row.verification.state === "disagrees";
        var warning = row.achieved && row.verification.state !== "verified";
        html += '<tr data-state="' + (critical ? "critical" : warning ? "warning" : "") + '"><td><details data-detail-key="run-' + ESC(row.case) + '"><summary><span class="row-title data">' + ESC(row.case) + '</span></summary><div class="detail__body flow"><p>' + ESC(row.goal) + '</p><dl class="detail__meta"><dt>run file</dt><dd>' + ESC(row.run_file) + '</dd><dt>worker</dt><dd>' + ESC(row.worker || "unreported") + '</dd><dt>steps</dt><dd>' + number(row.n_steps) + '</dd><dt>tokens</dt><dd>' + number(row.tokens_total) + '</dd><dt>declared gate</dt><dd>' + ESC(row.verification.command || "none") + '</dd><dt>target</dt><dd>' + ESC(row.verification.target || "none") + '</dd><dt>run fingerprint</dt><dd>' + ESC(row.verification.run_sha256) + '</dd></dl>';
        if (row.verification.evidence) html += '<p>Evidence: ' + ESC(row.verification.evidence.status) + ' · ' + ESC(row.verification.evidence.at) + ' · <span class="data">' + ESC(row.verification.evidence.log) + '</span></p>';
        if (row.verification.legacy_evidence && !row.verification.evidence) html += '<p>Legacy check exists but is not bound to these run bytes; verification is still required.</p>';
        html += commandHtml(row.verification.command_text, row.verification.execute_path, "This executes the run-declared verifier command in its declared target and writes attribution evidence. Continue?") + '</div></details></td>';
        html += '<td>' + badge(row.achieved ? "warn" : "info", row.achieved ? "achieved claim" : "failed claim") + '</td><td>' + verificationBadge(row) + '</td>';
        html += '<td data-align="right" class="score">' + score(row.composite) + '</td><td data-align="right" data-optional class="data">' + money(row.cost_usd, 5) + '</td><td data-optional>' + timeHtml(row.modified_at_ms, "unknown") + '</td></tr>';
      });
      html += "</tbody></table></div>";
      if (rows.length > 12) html += '<p><button class="button" type="button" data-expand-runs data-focus-key="expand-runs">' + (expandedRuns ? "Show newest 12" : "Show all " + rows.length) + '</button></p>';
      patch("runs", html);
    }

    function renderBrain(data) {
      var staging = data.staging || [];
      var pending = staging.filter(function (item) { return item.state !== "applied"; }).length;
      setText("summary-brain", staging.length ? pending + " unapplied · " + staging.length + " total" : "idle");
      var html = "";
      if (!staging.length) html = '<p class="empty">Idle · no staged candidates.</p>';
      else {
        html = '<div class="stack">';
        staging.forEach(function (batch) {
          var severity = stateSeverity(batch.state);
          html += '<div class="stack__item" data-state="' + severity + '"><div class="repel"><div><div class="row-title data">' + ESC(batch.file) + '</div><div class="row-subtitle">' + number(batch.candidates) + ' candidates · ' + number(batch.verdicts) + ' verdicts · ' + timeHtml(batch.modified_at_ms, "unknown") + '</div></div>' + badge(batch.state === "applied" ? "ok" : batch.state, batch.state.replace(/_/g, " ")) + '</div>';
          if (batch.receipt) html += '<p class="row-subtitle">receipt: ' + number(batch.receipt.promoted) + ' promoted · ' + number(batch.receipt.queued) + ' queued · ' + number(batch.receipt.skipped) + ' skipped · ' + ESC(batch.receipt.at) + '</p>';
          html += '<details data-detail-key="stage-' + ESC(batch.day + "-" + batch.name) + '"><summary>Review full candidates and auditor reasons</summary><div class="detail__body">';
          (batch.candidates_detail || []).forEach(function (candidate) {
            html += '<div class="candidate"><div class="cluster"><span class="data">S-' + pad3(candidate.index) + '</span>' + badge(candidate.verdict === "promote" ? "ok" : candidate.verdict === "reject" ? "bad" : "warn", candidate.verdict || "missing verdict") + '<span class="row-subtitle">' + ESC(candidate.domain) + '</span></div><p class="candidate__body">' + ESC(candidate.body) + '</p>';
            if (candidate.reason) html += '<p class="candidate__reason">Auditor: ' + ESC(candidate.reason) + '</p>';
            if (candidate.existing_id) html += '<p class="row-subtitle data">existing: ' + ESC(candidate.existing_id) + '</p>';
            html += "</div>";
          });
          html += "</div></details>";
          if (batch.command) html += commandHtml(batch.command, batch.execute_path, "Promotion applies recorded verdicts and can append canonical facts or human-queue entries. Review every candidate first. Continue?");
          html += "</div>";
        });
        html += "</div>";
      }
      html += '<div class="flow"><h3>Idle intake</h3><p class="row-subtitle">Runs the distiller and auditor only when machine load and freshness guards permit. It can incur model cost and write staging.</p>' + commandHtml("mini-agi dream --idle", "/api/act/dream-idle", "This can invoke model workers, incur cost, and write a new staging batch. Continue?") + '</div>';
      patch("brain", html);
    }

    function renderMemory(data) {
      var memory = data.memory || {};
      var queues = data.queues || [];
      var pending = 0;
      queues.forEach(function (queue) { pending += Number(queue.pending_count || 0); });
      setText("summary-memory", number(memory.facts) + " facts · " + number(pending) + " pending signoff");
      var verify = memory.verification || {};
      var html = '<div class="stack"><div class="stack__item"><div class="repel"><div><div class="row-title">Integrity scan</div><div class="row-subtitle">' + number(memory.entries) + ' entries · ' + number(memory.facts) + ' facts · ' + number(memory.derived_views) + ' derived · ' + number(memory.superseded) + ' superseded · ' + number(memory.preserved) + ' preserved</div></div>' + badge(verify.state === "ok" ? "ok" : verify.state === "findings" ? "bad" : "warn", verify.state || "unknown") + '</div><p class="row-subtitle">checked ' + timeHtml(verify.checked_at_ms, "unknown") + ' · read-only cache ttl ' + number(verify.cache_ttl_seconds) + 's</p>';
      if ((verify.findings || []).length) html += '<details data-detail-key="memory-findings"><summary>Show ' + verify.findings.length + ' integrity finding' + (verify.findings.length === 1 ? "" : "s") + '</summary><div class="detail__body"><ol>' + verify.findings.map(function (finding) { return "<li>" + ESC(finding) + "</li>"; }).join("") + "</ol></div></details>";
      html += commandHtml(verify.command || "mini-agi mem verify", verify.execute_path || "/api/act/mem-verify", null) + "</div>";
      if (!pending) html += '<div class="stack__item"><p class="empty">Queue idle · nothing waits for human signoff.</p></div>';
      queues.forEach(function (queue) {
        html += '<div class="stack__item" data-state="' + (queue.pending_count ? "warning" : "") + '"><div class="repel"><div><div class="row-title data">' + ESC(queue.file) + '</div><div class="row-subtitle">' + number(queue.pending_count) + ' pending · ' + number(queue.resolved_count) + ' resolved · updated ' + timeHtml(queue.updated_at_ms, "unknown") + '</div></div>' + badge(queue.pending_count ? "required" : "ok", queue.pending_count ? "human signoff" : "resolved") + '</div>';
        (queue.items || []).filter(function (item) { return item.state === "pending"; }).forEach(function (item) {
          html += '<details data-detail-key="queue-' + ESC((queue.day || "root") + "-" + queue.name + "-" + item.index) + '"><summary><span class="data">[' + number(item.index) + '] ' + ESC(item.digest) + '</span> · review full fact</summary><div class="detail__body"><p>' + ESC(item.payload) + '</p>' + commandHtml(item.command, item.execute_path, "Signoff appends this exact fact to canonical memory. Review the full payload first. Continue?") + '</div></details>';
        });
        if (queue.resolved_count) html += '<p class="row-subtitle">' + number(queue.resolved_count) + ' queue block' + (queue.resolved_count === 1 ? " is" : "s are") + ' already present in canonical memory and hidden from actions.</p>';
        html += "</div>";
      });
      patch("memory", html + "</div>");
    }

    function renderWorkers(data) {
      var workers = data.workers || [];
      var active = workers.filter(function (worker) { return worker.state === "working"; }).length;
      setText("summary-workers", workers.length ? active + " working · " + workers.length + " discovered" : "idle");
      if (!workers.length) { patch("workers", '<p class="empty">Idle · no detached workers.</p>'); return; }
      var html = '<div class="stack">';
      workers.forEach(function (worker) {
        var severity = worker.state === "crashed" ? "critical" : worker.stale ? "warning" : "";
        var state = worker.stale && worker.state === "working" ? "warning" : worker.state;
        html += '<div class="stack__item" data-state="' + severity + '"><div class="repel"><div><div class="row-title data">' + ESC(worker.id) + '</div><div class="row-subtitle">updated ' + timeHtml(worker.updated_at_ms, "unknown") + ' · started ' + timeHtml(worker.started_at_ms, "unknown") + '</div></div>' + badge(state, worker.stale && worker.state === "working" ? "working · stale" : worker.state) + '</div><details data-detail-key="worker-' + ESC(worker.id) + '"><summary>Worker evidence</summary><div class="detail__body"><dl class="detail__meta"><dt>workdir</dt><dd>' + ESC(worker.workdir || "unknown") + '</dd><dt>handle</dt><dd>' + ESC(worker.handle) + '</dd><dt>report</dt><dd>' + ESC(worker.report || "not ready") + '</dd><dt>stale threshold</dt><dd>' + number(worker.stale_after_seconds) + 's</dd></dl>';
        if (worker.progress_tail) html += '<p>' + ESC(worker.progress_tail) + '</p>';
        html += "</div></details></div>";
      });
      patch("workers", html + "</div>");
    }

    function renderJournal(data) {
      var journal = data.journal || {state:"absent",events:[],anomalies:[],counts:{}};
      setText("summary-journal", journal.state + " · " + number((journal.events || []).length) + " recent events");
      if (!journal.events.length) { patch("journal", '<p class="empty">Idle · checkpoint journal absent or empty.</p>'); return; }
      var html = '<div class="stack">';
      journal.events.forEach(function (event) {
        html += '<div class="stack__item journal-event" data-state="' + (event.state === "anomaly" ? "critical" : event.state === "in_progress" ? "warning" : "") + '"><span class="journal-event__time">' + isoTimeHtml(event.at) + '</span><span class="journal-event__label">' + ESC(event.label) + '</span>' + badge(event.state, event.kind.replace(/_/g, " ")) + '</div>';
      });
      html += "</div>";
      if (journal.anomalies.length) html += '<details data-detail-key="journal-anomalies"><summary>Show audit findings</summary><div class="detail__body"><ol>' + journal.anomalies.map(function (item) { return '<li>' + ESC(item.severity + " · line " + item.line_no + " · " + item.message) + '</li>'; }).join("") + '</ol></div></details>';
      patch("journal", html);
    }

    function renderRepository(data) {
      var repo = data.repository || {};
      setText("summary-repository", repo.state || "unavailable");
      var html = '<div class="stack"><div class="stack__item"><div class="repel"><div><div class="row-title">' + ESC(repo.name || "repository") + '</div><div class="row-subtitle data">' + ESC(repo.branch || "detached") + ' @ ' + ESC(repo.revision || "unknown") + '</div></div>' + badge(repo.state === "clean" ? "ok" : repo.state === "dirty" ? "warn" : "bad", repo.state || "unavailable") + '</div><dl class="detail__meta"><dt>root</dt><dd>' + ESC(repo.root || "unknown") + '</dd><dt>changed files</dt><dd>' + (repo.changed_files === null ? "unknown" : number(repo.changed_files)) + '</dd><dt>loop target</dt><dd>' + score(repo.target_composite) + '</dd><dt>rerun bound</dt><dd>' + (repo.max_rerun_attempts === null ? "unbounded" : number(repo.max_rerun_attempts)) + '</dd><dt>worker idle cap</dt><dd>' + (repo.max_idle_seconds === null ? "disabled" : number(repo.max_idle_seconds) + "s") + '</dd><dt>approval gate</dt><dd>' + (repo.require_approval ? "required" : "not required") + '</dd></dl></div></div>';
      patch("repository", html);
    }

    function renderTickets(data) {
      var tickets = data.tickets || [];
      var open = tickets.filter(function (ticket) { return ticket.status === "OPEN"; }).length;
      setText("summary-tickets", open + " open · " + tickets.length + " total");
      var ordered = tickets.slice().sort(function (left, right) {
        if ((left.status === "OPEN") !== (right.status === "OPEN")) return left.status === "OPEN" ? -1 : 1;
        return left.id.localeCompare(right.id);
      });
      var shown = expandedTickets ? ordered : ordered.filter(function (ticket) { return ticket.status === "OPEN"; }).slice(0, 12);
      if (!shown.length) { patch("tickets", '<p class="empty">Quiet · no open tickets.</p>'); return; }
      var html = '<div class="stack">';
      shown.forEach(function (ticket) {
        html += '<div class="stack__item"><details data-detail-key="ticket-' + ESC(ticket.id) + '"><summary><span class="cluster">' + badge(ticket.status === "CLOSED" ? "ok" : "warn", ticket.status) + '<span class="row-title data">' + ESC(ticket.id) + '</span><span>' + ESC(ticket.title) + '</span></span></summary><div class="detail__body flow"><p>' + ESC(ticket.goal) + '</p><dl class="detail__meta"><dt>claimant</dt><dd>' + ESC(ticket.claimant || "unclaimed") + '</dd><dt>claimed since</dt><dd>' + ESC(ticket.claimed_since || "—") + '</dd><dt>blocked by</dt><dd>' + ESC((ticket.blocked_by || []).join(", ") || "none") + '</dd><dt>scope</dt><dd>' + ESC((ticket.scope || []).join(", ") || "unspecified") + '</dd></dl>' + commandHtml(ticket.command, null, null) + '</div></details></div>';
      });
      html += "</div>";
      if (ordered.length > shown.length || expandedTickets) html += '<p><button class="button" type="button" data-expand-tickets data-focus-key="expand-tickets">' + (expandedTickets ? "Show open tickets" : "Show all " + ordered.length) + '</button></p>';
      patch("tickets", html);
    }

    function setDot(id, state) {
      var node = document.getElementById(id);
      if (node) node.setAttribute("data-state", state);
    }
    function renderNav(data) {
      var totals = data.totals || {};
      var attention = (data.attention || []).length;
      var gaps = (data.gaps || []).length;
      setText("nav-entries", number(totals.runs));
      setText("nav-pending", number(attention + gaps));
      setText("nav-gaps", number(gaps));
      setText("pill-review", number(attention));
      setText("pill-verified", number(totals.verified_achieved));
      var pending = 0;
      (data.staging || []).forEach(function (item) { if (item.state !== "applied") pending += 1; });
      setText("pill-pending", number(pending));
      var repo = data.repository || {};
      setText("nav-repo", repo.state || "unavailable");
      setDot("dot-repo", repo.state === "clean" ? "ok" : repo.state === "dirty" ? "amber" : "bad");
      var workers = data.workers || [];
      var active = workers.filter(function (worker) { return worker.state === "working"; }).length;
      setText("nav-workers", workers.length ? active + "/" + workers.length + " active" : "idle");
      setDot("dot-workers", workers.some(function (worker) { return worker.state === "crashed" || worker.stale; }) ? "amber" : (workers.length ? "ok" : "off"));
      var heartbeat = data.heartbeat || {};
      setText("nav-heartbeat", heartbeat.state === "never" ? "never" : heartbeat.state + " · " + (heartbeat.case || "unknown"));
      setDot("dot-heartbeat", heartbeat.state === "never" ? "off" : "ok");
    }

    function render(data) {
      latest = data;
      renderHeader(data);
      renderAttention(data);
      renderGaps(data);
      renderRuns(data);
      renderBrain(data);
      renderMemory(data);
      renderWorkers(data);
      renderJournal(data);
      renderRepository(data);
      renderTickets(data);
      renderNav(data);
      refreshAges();
    }

    function connection(state, label) {
      var node = byId("connection");
      node.setAttribute("data-state", state);
      node.textContent = label;
    }
    function reveal() {
      if (firstResponse) return;
      firstResponse = true;
      byId("boot").hidden = true;
      byId("dashboard").hidden = false;
    }
    function schedule() {
      if (timer !== null) window.clearTimeout(timer);
      timer = null;
      if (!document.hidden) timer = window.setTimeout(poll, POLL_MS);
    }
    function poll() {
      if (polling || document.hidden) { schedule(); return; }
      polling = true;
      fetch("/api/status", {cache:"no-store"}).then(function (response) {
        if (!response.ok) throw new Error("HTTP " + response.status);
        return response.json();
      }).then(function (data) {
        if (data.schema_version !== 2) throw new Error("unsupported status schema");
        failures = 0;
        connection("live", "LIVE");
        render(data);
        reveal();
      }).catch(function (error) {
        failures += 1;
        connection(failures >= 2 ? "stale" : "retrying", failures >= 2 ? "STALE" : "RETRYING");
        if (!firstResponse) {
          var boot = byId("boot");
          boot.setAttribute("data-state", "error");
          boot.textContent = "Kernel status unavailable: " + String(error.message || error);
        }
      }).then(function () {
        polling = false;
        refreshAges();
        schedule();
      });
    }

    function copyText(text) {
      if (navigator.clipboard && navigator.clipboard.writeText) return navigator.clipboard.writeText(text);
      return new Promise(function (resolve, reject) {
        var area = document.createElement("textarea");
        area.value = text;
        area.setAttribute("readonly", "readonly");
        area.className = "visually-hidden";
        document.body.appendChild(area);
        area.select();
        try { document.execCommand("copy") ? resolve() : reject(new Error("copy refused")); }
        catch (error) { reject(error); }
        document.body.removeChild(area);
      });
    }
    function action(path, command, button) {
      var runCase = path.indexOf("/api/act/run-verify?case=") === 0 ? path.split("=")[1] : null;
      if (runCase) pendingRuns[runCase] = true;
      if (latest) renderRuns(latest);
      button.disabled = true;
      setAction("", "Running: " + command);
      fetch(path, {method:"POST", headers:{"Content-Type":"application/json", "X-Mini-Agi-UI":"1"}, body:"{}"}).then(function (response) {
        return response.json().then(function (body) { return {ok:response.ok && body.ok, body:body}; });
      }).then(function (result) {
        setAction(result.ok ? "ok" : "bad", (result.ok ? "Completed: " : "Failed: ") + command + (result.body.output ? " · " + result.body.output.slice(0, 500) : ""));
      }).catch(function (error) {
        setAction("bad", "Action failed: " + String(error.message || error));
      }).then(function () {
        if (runCase) delete pendingRuns[runCase];
        button.disabled = false;
        window.setTimeout(poll, 200);
      });
    }
    function setAction(state, text) { var node = byId("action-log"); node.setAttribute("data-state", state); node.textContent = text; }

    function chatList() {
      fetch("/api/threads", {cache:"no-store"}).then(function (response) {
        if (!response.ok) throw new Error("HTTP " + response.status);
        return response.json();
      }).then(function (threads) {
        if (!threads.length) { patch("threadbar", '<p class="empty">No persisted threads.</p>'); return; }
        patch("threadbar", threads.map(function (thread) {
          return '<button class="button" type="button" data-thread="' + ESC(thread[0]) + '"><span class="data">' + ESC(thread[1] || thread[0]) + '</span> · ' + number(thread[2]) + '</button>';
        }).join(""));
      }).catch(function (error) { patch("threadbar", '<p class="empty">Thread list unavailable: ' + ESC(error.message || error) + '</p>'); });
    }
    function chatOpen(id) {
      fetch("/api/threads/" + id, {cache:"no-store"}).then(function (response) {
        if (!response.ok) throw new Error("HTTP " + response.status);
        return response.json();
      }).then(function (thread) {
        chatThread = thread.id;
        renderTranscript(thread.messages || []);
      }).catch(function (error) { renderTranscript([{role:"error",text:String(error.message || error),ts:""}]); });
    }
    function renderTranscript(messages) {
      var log = byId("chatlog");
      while (log.firstChild) log.removeChild(log.firstChild);
      if (!messages.length) { var empty = document.createElement("p"); empty.className = "empty"; empty.textContent = "New thread · the next message creates it."; log.appendChild(empty); return; }
      messages.forEach(function (message) { appendChat(message.role, message.text, message.ts, false); });
      log.scrollTop = log.scrollHeight;
    }
    function appendChat(role, text, timestamp, scroll) {
      var log = byId("chatlog");
      if (log.querySelector(".empty")) log.innerHTML = "";
      var item = document.createElement("div");
      item.className = "chat-message";
      item.setAttribute("data-role", role);
      var label = document.createElement("div");
      label.className = "chat-message__role";
      label.textContent = role === "agent" ? "agent" : role === "human" ? "you" : role;
      var body = document.createElement("div");
      body.className = "chat-message__text";
      body.textContent = text;
      item.appendChild(label);
      item.appendChild(body);
      if (timestamp) { var at = document.createElement("div"); at.className = "chat-message__time"; at.textContent = timestamp; item.appendChild(at); }
      log.appendChild(item);
      if (scroll !== false) log.scrollTop = log.scrollHeight;
      return item;
    }
    function chatNew() { chatThread = null; renderTranscript([]); byId("chatinput").focus(); }
    function chatSend() {
      var input = byId("chatinput");
      var message = input.value.replace(/^\s+|\s+$/g, "");
      if (!message) return;
      input.value = "";
      appendChat("human", message, "", true);
      var thinking = appendChat("agent", "thinking…", "", true);
      byId("chatsend").disabled = true;
      var body = JSON.stringify({message:message, thread:chatThread});
      fetch("/api/chat", {method:"POST", headers:{"Content-Type":"application/json", "X-Mini-Agi-UI":"1"}, body:body}).then(function (response) {
        return response.json().then(function (payload) { if (!response.ok) throw new Error(payload.output || "chat failed"); return payload; });
      }).then(function (payload) {
        if (thinking.parentNode) thinking.parentNode.removeChild(thinking);
        chatThread = payload.thread;
        appendChat("agent", payload.output || "(empty)", "", true);
        chatList();
      }).catch(function (error) {
        if (thinking.parentNode) thinking.parentNode.removeChild(thinking);
        appendChat("error", String(error.message || error), "", true);
      }).then(function () { byId("chatsend").disabled = false; input.focus(); });
    }

    function scrollToPanel(name) {
      var panel = byId("panel-" + name);
      if (panel) { panel.scrollIntoView({ block: "start" }); panel.setAttribute("tabindex", "-1"); panel.focus(); }
    }

    document.addEventListener("click", function (event) {
      var scroller = event.target;
      while (scroller && scroller !== document && !scroller.hasAttribute("data-scroll")) scroller = scroller.parentNode;
      if (scroller && scroller !== document) { scrollToPanel(scroller.getAttribute("data-scroll")); return; }
      var button = closestButton(event.target);
      if (!button) return;
      if (button.hasAttribute("data-copy")) {
        copyText(button.getAttribute("data-command") || "").then(function () { setAction("ok", "Command copied."); }).catch(function (error) { setAction("bad", "Copy failed: " + String(error.message || error)); });
      } else if (button.hasAttribute("data-action")) {
        var confirmation = button.getAttribute("data-confirm");
        if (!confirmation || window.confirm(confirmation + "\n\n" + button.getAttribute("data-command"))) action(button.getAttribute("data-action"), button.getAttribute("data-command"), button);
      } else if (button.hasAttribute("data-scroll-panel")) {
        scrollToPanel(button.getAttribute("data-scroll-panel"));
      } else if (button.hasAttribute("data-expand-runs")) {
        expandedRuns = !expandedRuns; if (latest) renderRuns(latest);
      } else if (button.hasAttribute("data-expand-tickets")) {
        expandedTickets = !expandedTickets; if (latest) renderTickets(latest);
      } else if (button.hasAttribute("data-thread")) {
        chatOpen(button.getAttribute("data-thread"));
      } else if (button.hasAttribute("data-chat-new")) {
        chatNew();
      }
    });
    byId("chatsend").addEventListener("click", chatSend);
    byId("chatinput").addEventListener("keydown", function (event) { if (event.key === "Enter") { event.preventDefault(); chatSend(); } });
    document.addEventListener("visibilitychange", function () { if (document.hidden) { if (timer !== null) window.clearTimeout(timer); timer = null; } else { poll(); } });

    chatList();
    poll();
  }());
  </script>
</body>
</html>"#;

/// One classified attention item: the human's action list, computed by
/// Rust from filesystem truth and the kernel's policies.
#[derive(Debug, Clone, serde::Serialize)]
struct AttentionItem {
    id: String,
    /// `critical` | `warning` | `info`.
    severity: String,
    kind: String,
    title: String,
    detail: String,
    /// Which panel the item belongs to (scroll target).
    target_panel: Option<String>,
    /// Exact pasteable command resolving this item.
    command: Option<String>,
    /// Same-origin execute route, when the action is one-click safe.
    execute_path: Option<String>,
    /// Confirmation text for the Run button.
    confirmation: Option<String>,
}

/// Push one classified attention item.
#[allow(clippy::too_many_arguments)]
fn attention(
    items: &mut Vec<AttentionItem>,
    id: impl Into<String>,
    severity: &str,
    kind: &str,
    title: impl Into<String>,
    detail: impl Into<String>,
    target_panel: Option<&str>,
    command: Option<String>,
    execute_path: Option<String>,
    confirmation: Option<&str>,
) {
    items.push(AttentionItem {
        id: id.into(),
        severity: severity.to_string(),
        kind: kind.to_string(),
        title: title.into(),
        detail: detail.into(),
        target_panel: target_panel.map(str::to_string),
        command,
        execute_path,
        confirmation: confirmation.map(str::to_string),
    });
}

/// File mtime as epoch milliseconds, `None` when unreadable.
fn path_modified_ms(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .map(crate::status::system_time_ms)
}

/// Repository-relative path for display (absolute paths stay in
/// expandable evidence, not primary labels).
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Every staged `.md` batch under `memory/staging/<day>/`, sorted.
fn staging_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let staging_root = root.join(mini_agi_core::dream::STAGING_REL);
    if let Ok(days) = std::fs::read_dir(staging_root) {
        for day in days.flatten() {
            let Ok(entries) = std::fs::read_dir(day.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|extension| extension == "md") {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

/// Every human-review queue under `memory/review/`: the writer's FLAT
/// layout (`contested-<date>.md` files directly under review/) and one
/// optional plain-segment day directory. Returns `(day, path)` pairs.
fn queue_files(root: &Path) -> Vec<(Option<String>, PathBuf)> {
    let mut files = Vec::new();
    let review = root.join(mini_agi_core::memory::REVIEW_REL);
    let Ok(entries) = std::fs::read_dir(review) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|extension| extension == "md") {
            files.push((None, path));
            continue;
        }
        let day = entry.file_name().to_string_lossy().into_owned();
        if !path.is_dir() || !crate::status::plain_path_segment(&day) {
            continue;
        }
        let Ok(children) = std::fs::read_dir(path) else {
            continue;
        };
        for child in children.flatten() {
            let child_path = child.path();
            if child_path.is_file()
                && child_path
                    .extension()
                    .is_some_and(|extension| extension == "md")
            {
                files.push((Some(day.clone()), child_path));
            }
        }
    }
    files.sort_by(|left, right| left.1.cmp(&right.1));
    files
}

/// The API payload: everything the page renders, computed fresh per
/// request from the filesystem (no state, no cache beyond the bounded
/// memory-integrity snapshot).
#[derive(serde::Serialize)]
struct ApiPayload {
    schema_version: u8,
    generated_at_ms: u64,
    poll_after_ms: u64,
    target: f64,
    kernel: serde_json::Value,
    heartbeat: serde_json::Value,
    totals: serde_json::Value,
    attention: Vec<AttentionItem>,
    runs: serde_json::Value,
    gaps: Vec<serde_json::Value>,
    workers: Vec<crate::status::WorkerStatus>,
    journal: crate::status::JournalSnapshot,
    /// Raw tail stays as a compatibility alias; the UI renders `journal`.
    journal_tail: Vec<String>,
    queues: Vec<serde_json::Value>,
    staging: Vec<serde_json::Value>,
    tickets: Vec<serde_json::Value>,
    memory: serde_json::Value,
    repository: crate::status::RepositoryStatus,
}

fn api_payload(root: &Path) -> ApiPayload {
    let now_ms = crate::status::system_time_ms(SystemTime::now());
    let config = mini_agi_core::config::Config::load(root);
    let target = config.target_composite;
    let mut snapshot_errors = Vec::new();
    let mut attention_items = Vec::new();

    // Runs and immutable verifier evidence (claim vs proof).
    let run_index = crate::status::index_runs(&root.join("evals/cases"), root);
    let claimed_achieved_runs = run_index.achieved_runs;
    let verified_achieved_runs = run_index
        .rows
        .iter()
        .filter(|row| row.achieved && row.verification.state == "verified")
        .count();
    let verification_required_runs = run_index
        .rows
        .iter()
        .filter(|row| row.verification.state == "required")
        .count();
    for row in run_index
        .rows
        .iter()
        .filter(|row| row.verification.state == "disagrees")
    {
        attention(
            &mut attention_items,
            format!("verification-disagrees-{}", row.case),
            "critical",
            "verification_disagrees",
            format!("Verifier disagrees for {}", row.case),
            "The deterministic gate and the run's achieved claim disagree. The run is not trusted.",
            Some("runs"),
            Some(row.verification.command_text.clone()),
            row.verification.execute_path.clone(),
            Some("This executes the run-declared verifier in its declared target."),
        );
    }
    if let Some(newest) = run_index.rows.first()
        && newest.achieved
        && newest.verification.state != "verified"
        && newest.verification.state != "disagrees"
    {
        let (severity, detail) = if newest.verification.declared {
            (
                "warning",
                "The newest run claims achieved and declares a verifier, but no current fingerprint-bound evidence exists.",
            )
        } else {
            (
                "critical",
                "The newest run claims achieved but declares no deterministic verifier. Its outcome cannot become trusted.",
            )
        };
        attention(
            &mut attention_items,
            format!("latest-run-untrusted-{}", newest.case),
            severity,
            "latest_run_untrusted",
            format!("Newest achieved claim is untrusted: {}", newest.case),
            detail,
            Some("runs"),
            Some(newest.verification.command_text.clone()),
            newest.verification.execute_path.clone(),
            newest
                .verification
                .declared
                .then_some("This executes the run-declared verifier in its declared target."),
        );
    }
    let additional_required = run_index
        .rows
        .iter()
        .skip(1)
        .filter(|row| row.verification.state == "required")
        .count();
    if additional_required > 0 {
        attention(
            &mut attention_items,
            "verification-required-aggregate",
            "warning",
            "verification_required",
            format!("{additional_required} additional run(s) require verification"),
            "Use the exact command on each run row; old case-level evidence is not accepted for changed run bytes.",
            Some("runs"),
            None,
            None,
            None,
        );
    }

    // Detached workers: state derived in status.rs from process identity,
    // report presence, and artifact mtimes.
    let workers = crate::status::live_workers(root);
    for worker in &workers {
        if worker.state == "crashed" {
            attention(
                &mut attention_items,
                format!("worker-crashed-{}", worker.id),
                "critical",
                "worker_crashed",
                format!("Detached worker crashed: {}", worker.id),
                format!(
                    "The supervisor is dead and no report exists. Inspect {} and its run.out.",
                    worker.workdir.as_deref().unwrap_or(&worker.handle)
                ),
                Some("workers"),
                None,
                None,
                None,
            );
        } else if worker.stale {
            attention(
                &mut attention_items,
                format!("worker-stale-{}", worker.id),
                "warning",
                "worker_stale",
                format!("Worker has no recent activity: {}", worker.id),
                format!(
                    "No tracked worker artifact changed within {} seconds.",
                    worker.stale_after_seconds
                ),
                Some("workers"),
                None,
                None,
                None,
            );
        }
    }

    // Checkpoint semantics: typed events, resolved pairs, anomalies.
    let journal = crate::status::journal_snapshot(root, 14);
    let journal_tail = crate::status::journal_tail(root, 14);
    let bad_journal = journal
        .anomalies
        .iter()
        .filter(|anomaly| anomaly.severity == "bad")
        .count();
    if bad_journal > 0 {
        attention(
            &mut attention_items,
            "journal-anomaly",
            "critical",
            "journal_anomaly",
            format!("{bad_journal} current checkpoint journal anomaly/anomalies"),
            "Repair through checkpoint tooling. Never hand-edit or restore the journal through Git.",
            Some("journal"),
            None,
            None,
            None,
        );
    }

    // Loop/gaps: best score vs target, attempts, exhaustion, ownership.
    let loop_status = match mini_agi_core::loopcmd::status(root) {
        Ok(status) => Some(status),
        Err(error) => {
            snapshot_errors.push(format!("loop status unavailable: {error}"));
            None
        }
    };
    let mut gaps = Vec::new();
    if let Some(status) = loop_status {
        for row in status.cases {
            let best = row.best_composite.unwrap_or(row.composite);
            if best >= target {
                continue;
            }
            let delta = (target - best).max(0.0);
            let repair_signal = row.repair_signal.map(|signal| signal.to_string());
            if row.status.as_deref() == Some("CLOSED") {
                attention(
                    &mut attention_items,
                    format!("gap-closed-ticket-{}", row.case),
                    "critical",
                    "gap_closed_ticket",
                    format!("Below-target gap maps to a CLOSED ticket: {}", row.case),
                    format!(
                        "Best {best:.4} is {delta:.4} below target {target:.4}; reconcile or replace the closed ticket.",
                    ),
                    Some("gaps"),
                    row.ticket
                        .as_ref()
                        .map(|ticket| format!("mini-agi ticket show {ticket}")),
                    None,
                    None,
                );
            } else if row.exhausted {
                attention(
                    &mut attention_items,
                    format!("gap-exhausted-{}", row.case),
                    "critical",
                    "gap_exhausted",
                    format!("Gap exhausted: {}", row.case),
                    format!(
                        "Best {best:.4} is {delta:.4} below target {target:.4} after {} attempt(s). Further blind retry is blocked.",
                        row.attempts
                    ),
                    Some("gaps"),
                    row.ticket
                        .as_ref()
                        .map(|ticket| format!("mini-agi ticket show {ticket}")),
                    None,
                    None,
                );
            } else if row.ticket.is_none() || row.claimant.is_none() {
                attention(
                    &mut attention_items,
                    format!("gap-unowned-{}", row.case),
                    "warning",
                    "gap_unowned",
                    format!("Gap has no active owner: {}", row.case),
                    format!(
                        "Best {best:.4}; {delta:.4} below target. {}.",
                        if row.ticket.is_none() {
                            "No ticket is mapped"
                        } else {
                            "Ticket is unclaimed"
                        }
                    ),
                    Some("gaps"),
                    row.ticket
                        .as_ref()
                        .map(|ticket| format!("mini-agi ticket show {ticket}")),
                    None,
                    None,
                );
            }
            gaps.push(serde_json::json!({
                "case": row.case,
                "composite": row.composite,
                "best_composite": row.best_composite,
                "ticket": row.ticket,
                "attempts": row.attempts,
                "exhausted": row.exhausted,
                "target": target,
                "delta": delta,
                "max_attempts": config.max_rerun_attempts,
                "repair_signal": repair_signal,
                "ticket_status": row.status,
                "claimant": row.claimant,
            }));
        }
    }

    // Human signoff queues. Only digests NOT present in canonical count
    // as pending; queue indices stay the original one-based block
    // indices while resolved entries collapse visually.
    let known: HashSet<String> = mini_agi_core::memory::existing_fact_ids(root)
        .into_iter()
        .collect();
    let mut queues = Vec::new();
    let mut pending_queue_items = 0usize;
    for (day, path) in queue_files(root) {
        let name = path
            .file_name()
            .map_or_else(String::new, |value| value.to_string_lossy().into_owned());
        let segments_safe = crate::status::plain_path_segment(&name)
            && day.as_deref().is_none_or(crate::status::plain_path_segment);
        let queue_relative = day.as_ref().map_or_else(
            || Path::new("memory/review").join(&name),
            |value| Path::new("memory/review").join(value).join(&name),
        );
        let action_day = day
            .as_ref()
            .map_or_else(String::new, |value| format!("day={value}&"));
        let mut pending_count = 0usize;
        let mut resolved_count = 0usize;
        let items: Vec<serde_json::Value> = mini_agi_core::memory::queued_facts(&path)
            .into_iter()
            .enumerate()
            .map(|(offset, (digest, payload))| {
                let index = offset + 1;
                let signed_off = known.contains(&digest);
                if signed_off {
                    resolved_count += 1;
                } else {
                    pending_count += 1;
                }
                let command = (!signed_off && segments_safe)
                    .then(|| format!("mini-agi mem signoff {} {index}", queue_relative.display()));
                let execute_path = (!signed_off && segments_safe)
                    .then(|| format!("/api/act/signoff?{action_day}file={name}&i={index}"));
                serde_json::json!({
                    "index": index,
                    "digest": digest,
                    "payload": payload,
                    "state": if signed_off { "signed_off" } else { "pending" },
                    "command": command,
                    "execute_path": execute_path,
                })
            })
            .collect();
        if pending_count == 0 && resolved_count == 0 {
            continue;
        }
        let updated_at_ms = path_modified_ms(&path);
        pending_queue_items += pending_count;
        if pending_count > 0 {
            let stale = updated_at_ms
                .is_some_and(|updated| now_ms.saturating_sub(updated) > QUEUE_STALE_MS);
            attention(
                &mut attention_items,
                format!("human-queue-{}-{name}", day.as_deref().unwrap_or("root")),
                if stale { "critical" } else { "warning" },
                "human_queue",
                format!("{pending_count} fact(s) await signoff in {name}"),
                if stale {
                    "The queue file has not changed for more than 24 hours. Open Memory and review every full payload."
                } else {
                    "Canonical promotion requires explicit human review. Open Memory and review every full payload."
                },
                Some("memory"),
                None,
                None,
                None,
            );
        }
        queues.push(serde_json::json!({
            "day": day,
            "name": name,
            "file": relative(root, &path),
            "updated_at_ms": updated_at_ms,
            "age_basis": "file_mtime",
            "pending_count": pending_count,
            "resolved_count": resolved_count,
            "items": items,
        }));
    }
    queues.sort_by(|left, right| {
        right["updated_at_ms"]
            .as_u64()
            .cmp(&left["updated_at_ms"].as_u64())
    });

    // Staging with durable application receipts: `applied` only when the
    // receipt's staged-hash matches the current bytes; older pending
    // batches are no longer masked behind an already-applied newest file.
    let files = staging_files(root);
    let next_to_apply = files.iter().rev().find(|path| {
        mini_agi_core::dream::read_promotion_receipt(path)
            .is_none_or(|receipt| !mini_agi_core::dream::receipt_matches_staged(path, &receipt))
    });
    let mut staging = Vec::new();
    for path in files.iter().rev() {
        let facts = crate::read_staged_facts(path).unwrap_or_default();
        let verdicts = mini_agi_core::dream::read_verdicts(&path.with_extension("verdicts.json"));
        let by_index: HashMap<usize, &mini_agi_core::dream::AuditorVerdict> = verdicts
            .iter()
            .map(|verdict| (verdict.index, verdict))
            .collect();
        let coverage_complete = facts.len() == verdicts.len()
            && (0..facts.len()).all(|index| by_index.contains_key(&index));
        let receipt = mini_agi_core::dream::read_promotion_receipt(path);
        let receipt_matches = receipt
            .as_ref()
            .is_some_and(|receipt| mini_agi_core::dream::receipt_matches_staged(path, receipt));
        let state = if receipt.is_some() && !receipt_matches {
            "receipt_mismatch"
        } else if receipt_matches {
            "applied"
        } else if !coverage_complete {
            "audit_incomplete"
        } else {
            "pending"
        };
        let day = path
            .parent()
            .and_then(Path::file_name)
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let is_next_to_apply = next_to_apply.is_some_and(|next| next == path);
        let actionable = state == "pending" && is_next_to_apply;
        let details: Vec<serde_json::Value> = facts
            .iter()
            .enumerate()
            .map(|(index, fact)| {
                let verdict = by_index.get(&index).copied();
                serde_json::json!({
                    "index": index,
                    "domain": fact.domain.clone(),
                    "body": fact.body.clone(),
                    "verdict": verdict.map(|item| item.verdict.clone()),
                    "reason": verdict.and_then(|item| item.reason.clone()),
                    "existing_id": verdict.and_then(|item| item.existing_id.clone()),
                })
            })
            .collect();
        if state == "receipt_mismatch" || state == "audit_incomplete" {
            attention(
                &mut attention_items,
                format!("staging-incomplete-{day}-{name}"),
                "critical",
                "staging_incomplete",
                format!("Staging batch needs investigation: {name}"),
                if state == "receipt_mismatch" {
                    "The staged bytes no longer match their application receipt. No applied claim is accepted."
                } else {
                    "Not every candidate has exactly one auditor verdict. Promotion is disabled."
                },
                Some("brain"),
                None,
                None,
                None,
            );
        } else if actionable {
            attention(
                &mut attention_items,
                format!("staging-pending-{day}-{name}"),
                "warning",
                "staging_pending",
                format!("Staging batch awaits explicit application: {name}"),
                format!(
                    "Review all {} candidates and {} auditor verdicts before promotion.",
                    facts.len(),
                    verdicts.len()
                ),
                Some("brain"),
                Some("mini-agi dream --promote".to_string()),
                Some("/api/act/dream-promote".to_string()),
                Some("Promotion can append canonical facts or human-queue entries."),
            );
        }
        let promoted = receipt.as_ref().is_some_and(|value| value.promoted > 0);
        staging.push(serde_json::json!({
            "day": day,
            "name": name,
            "file": relative(root, path),
            "modified_at_ms": path_modified_ms(path),
            "candidates": facts.len(),
            "verdicts": verdicts.len(),
            "verdict_detail": verdicts,
            "promoted": promoted,
            "applied": receipt_matches,
            "state": state,
            "is_next_to_apply": is_next_to_apply,
            "command": actionable.then_some("mini-agi dream --promote"),
            "execute_path": actionable.then_some("/api/act/dream-promote"),
            "candidates_detail": details,
            "receipt": receipt,
        }));
    }

    // Tickets and claims.
    let claims: HashMap<String, mini_agi_core::ticket::Claim> =
        mini_agi_core::ticket::read_claims(root)
            .unwrap_or_default()
            .into_iter()
            .map(|claim| (claim.ticket.clone(), claim))
            .collect();
    let tickets: Vec<serde_json::Value> = mini_agi_core::ticket::list_tickets(root)
        .unwrap_or_default()
        .into_iter()
        .map(|ticket| {
            let claim = claims.get(&ticket.id);
            let command = format!("mini-agi ticket show {}", ticket.id);
            serde_json::json!({
                "id": ticket.id,
                "title": ticket.title,
                "status": ticket.status,
                "goal": ticket.goal,
                "scope": ticket.scope,
                "blocked_by": ticket.blocked_by,
                "claimant": claim.map(|value| value.claimant.clone()),
                "claimed_since": claim.map(|value| value.since.clone()),
                "command": command,
            })
        })
        .collect();
    let open_tickets = tickets
        .iter()
        .filter(|ticket| ticket["status"].as_str() == Some("OPEN"))
        .count();
    if open_tickets > 0 {
        attention(
            &mut attention_items,
            "open-tickets",
            "info",
            "open_tickets",
            format!("{open_tickets} ticket(s) remain open"),
            "Open Tickets for dependency, scope, and claim details.",
            Some("tickets"),
            None,
            None,
            None,
        );
    }

    // Memory inventory + live integrity signal (truthful, cached).
    let metrics = mini_agi_core::metrics::stats(root).unwrap_or_default();
    let memory_health = crate::status::memory_health(root);
    if memory_health.state != "ok" {
        attention(
            &mut attention_items,
            "memory-integrity",
            "critical",
            "memory_integrity",
            "Canonical memory integrity is not green",
            if memory_health.findings.is_empty() {
                "The integrity state is unknown."
            } else {
                "Open Memory for the complete deterministic findings."
            },
            Some("memory"),
            Some(memory_health.command.clone()),
            Some(memory_health.execute_path.clone()),
            None,
        );
    } else if now_ms.saturating_sub(memory_health.checked_at_ms) > MEMORY_SCAN_STALE_MS {
        attention(
            &mut attention_items,
            "memory-scan-stale",
            "warning",
            "memory_scan_stale",
            "Memory integrity evidence is stale",
            "The last in-process integrity scan is older than five minutes.",
            Some("memory"),
            Some(memory_health.command.clone()),
            Some(memory_health.execute_path.clone()),
            None,
        );
    }
    let verification_state = memory_health.state.clone();
    let memory = serde_json::json!({
        "entries": metrics.entries,
        "facts": metrics.facts,
        "derived_views": metrics.derived_views,
        "superseded": mini_agi_core::memory::superseded_ids(root).len(),
        "preserved": mini_agi_core::memory::preserved_ids(root).len(),
        "verify": verification_state,
        "verification": memory_health,
    });

    // Machine health is intentionally lighter than full `audit`: no eval
    // gate or repository mutation check runs every 2.5 seconds.
    let machine = match mini_agi_core::health::health(root) {
        Ok(report) => {
            for (index, finding) in report.findings.iter().enumerate() {
                attention(
                    &mut attention_items,
                    format!("machine-health-{index}"),
                    if finding.severity == "critical" {
                        "critical"
                    } else {
                        "warning"
                    },
                    "machine_health",
                    "Machine health finding",
                    finding.message.clone(),
                    None,
                    None,
                    None,
                    None,
                );
            }
            serde_json::json!({
                "verdict": report.verdict(),
                "load1": report.load1,
                "nproc": report.nproc,
                "mem_available_frac": report.mem_available_frac,
                "swap_used_frac": report.swap_used_frac,
                "zoo_largest": report.zoo_largest,
                "findings": report.findings.into_iter().map(|finding| serde_json::json!({
                    "severity": finding.severity,
                    "message": finding.message,
                })).collect::<Vec<_>>(),
            })
        }
        Err(error) => {
            snapshot_errors.push(format!("machine health unavailable: {error}"));
            serde_json::json!({
                "verdict": "UNKNOWN",
                "load1": null,
                "nproc": 0,
                "mem_available_frac": null,
                "swap_used_frac": null,
                "zoo_largest": null,
                "findings": [],
            })
        }
    };

    // Last real verifier execution, not the newest run claim.
    let attribution = mini_agi_core::verifier::read_attribution(root).unwrap_or_default();
    let heartbeat = attribution.last().map_or_else(
        || {
            serde_json::json!({
                "state": "never",
                "at": null,
                "case": null,
                "command": null,
                "target": null,
                "run_sha256": null,
                "evidence_log": "memory/episodic/verify.log",
            })
        },
        |row| {
            let state = match row.status.as_str() {
                "verified-failed" => "verified_failed",
                other => other,
            };
            serde_json::json!({
                "state": state,
                "at": row.at,
                "case": row.case,
                "command": row.command,
                "target": row.target,
                "run_sha256": row.run_sha256,
                "evidence_log": "memory/episodic/verify.log",
            })
        },
    );

    let repository = crate::status::repository_status(root);
    let active_workers = workers
        .iter()
        .filter(|worker| worker.state == "working")
        .count();
    attention_items.sort_by(|left, right| {
        fn severity_rank(severity: &str) -> u8 {
            match severity {
                "critical" => 0,
                "warning" => 1,
                _ => 2,
            }
        }
        severity_rank(&left.severity)
            .cmp(&severity_rank(&right.severity))
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.id.cmp(&right.id))
    });
    let kernel_state = if attention_items
        .iter()
        .any(|item| item.severity == "critical")
    {
        "critical"
    } else if !snapshot_errors.is_empty()
        || attention_items
            .iter()
            .any(|item| item.severity == "warning")
    {
        "warn"
    } else {
        "ok"
    };
    let kernel = serde_json::json!({
        "state": kernel_state,
        "summary": match kernel_state {
            "critical" => "human intervention required",
            "warn" => "supervision has warnings",
            _ => "no current supervision findings",
        },
        "machine": machine,
        "snapshot_errors": snapshot_errors,
    });
    let totals = serde_json::json!({
        "runs": run_index.total_runs,
        "claimed_achieved": claimed_achieved_runs,
        "verified_achieved": verified_achieved_runs,
        "verification_required": verification_required_runs,
        "total_cost_usd": run_index.total_cost_usd,
        "total_tokens": run_index.total_tokens,
        "open_gaps": gaps.len(),
        "open_tickets": open_tickets,
        "active_workers": active_workers,
        "pending_queue_items": pending_queue_items,
    });
    let runs = serde_json::json!({
        "rows": &run_index.rows,
        "total_runs": run_index.total_runs,
        "achieved_runs": run_index.achieved_runs,
        "claimed_achieved_runs": claimed_achieved_runs,
        "verified_achieved_runs": verified_achieved_runs,
        "verification_required_runs": verification_required_runs,
        "total_cost_usd": run_index.total_cost_usd,
        "total_tokens": run_index.total_tokens,
        "workers": &workers,
        "journal_tail": journal_tail,
        "target": target,
    });

    ApiPayload {
        schema_version: 2,
        generated_at_ms: now_ms,
        poll_after_ms: POLL_AFTER_MS,
        target,
        kernel,
        heartbeat,
        totals,
        attention: attention_items,
        runs,
        gaps,
        workers,
        journal,
        journal_tail,
        queues,
        staging,
        tickets,
        memory,
        repository,
    }
}
/// Serve the live dashboard on `127.0.0.1:<port>` until killed.
pub fn serve(root: &Path, port: u16) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!("mini-agi ui: http://127.0.0.1:{port} (Ctrl-C to stop)");
    let running = Arc::new(AtomicBool::new(true));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if !running.load(Ordering::Relaxed) {
            break;
        }
        let root = root.to_path_buf();
        let running = Arc::clone(&running);
        std::thread::spawn(move || {
            // The bounded reader slices to the ACTUAL read length and
            // never parses unread buffer padding (the NUL regression).
            let request = read_http_request(&mut stream).unwrap_or_default();
            let mut parts = request.split_whitespace();
            let method = parts.next().unwrap_or("GET").to_string();
            let path = parts.next().unwrap_or("/").to_string();
            let (status, ctype, body) = if method == "POST" && path.starts_with("/api/act/") {
                // HITL actions: the HUMAN clicks on localhost (header
                // `X-Mini-Agi-UI: 1` proves the page, not a cross-origin
                // form); the server executes the exact kernel command the
                // human could type. Unknown routes are never executed.
                if request_header(&request, "X-Mini-Agi-UI") == Some("1") {
                    let action = act(&root, &path);
                    (
                        if action.ok {
                            "200 OK"
                        } else {
                            "400 Bad Request"
                        },
                        "application/json",
                        serde_json::json!({"ok": action.ok, "output": action.output}).to_string(),
                    )
                } else {
                    (
                        "403 Forbidden",
                        "application/json",
                        serde_json::json!({"error": "missing X-Mini-Agi-UI"}).to_string(),
                    )
                }
            } else if method == "POST" && path.starts_with("/api/chat") {
                // Chat with the agent: the human's message runs through a
                // bounded worker seeded with the thread's own history +
                // the kernel's memory context (resume block). Threads
                // persist under memory/episodic/chat/ and CONTINUE across
                // turns; the response always carries the resolved thread
                // id so the browser never forks a thread by accident.
                if request_header(&request, "X-Mini-Agi-UI") == Some("1") {
                    let parsed = request_body(&request)
                        .and_then(|body| serde_json::from_str::<ChatRequest>(body).ok());
                    let chat = match parsed {
                        Some(input)
                            if !input.message.trim().is_empty()
                                && input.message.len() <= 16_384
                                && input.thread.as_deref().is_none_or(|id| {
                                    crate::status::plain_path_segment(id)
                                }) =>
                        {
                            chat_with_kernel(&root, input.thread.as_deref(), &input.message)
                        }
                        _ => ChatResult {
                            ok: false,
                            output: "chat request must contain a non-empty message up to 16384 bytes and an optional plain thread id".to_string(),
                            thread: None,
                        },
                    };
                    (
                        if chat.ok { "200 OK" } else { "400 Bad Request" },
                        "application/json",
                        serde_json::json!({
                            "ok": chat.ok,
                            "output": chat.output,
                            "thread": chat.thread,
                        })
                        .to_string(),
                    )
                } else {
                    (
                        "403 Forbidden",
                        "application/json",
                        serde_json::json!({"error": "missing X-Mini-Agi-UI"}).to_string(),
                    )
                }
            } else if method == "POST" {
                (
                    "405 Method Not Allowed",
                    "text/plain",
                    "method not allowed".to_string(),
                )
            } else {
                match path.as_str() {
                    "/" => ("200 OK", "text/html; charset=utf-8", INDEX_HTML.to_string()),
                    "/api/status" => {
                        let payload = api_payload(&root);
                        (
                            "200 OK",
                            "application/json",
                            serde_json::to_string(&payload).unwrap_or_else(|error| {
                                serde_json::json!({
                                    "error": format!("status serialization failed: {error}")
                                })
                                .to_string()
                            }),
                        )
                    }
                    // Transcript route (read-only; ids are plain segments).
                    path if path.starts_with("/api/threads/") => {
                        let id = path.strip_prefix("/api/threads/").unwrap_or("");
                        if !crate::status::plain_path_segment(id) {
                            (
                                "400 Bad Request",
                                "application/json",
                                serde_json::json!({"error": "invalid thread id"}).to_string(),
                            )
                        } else if let Some(messages) = read_thread(&root, id) {
                            (
                                "200 OK",
                                "application/json",
                                serde_json::json!({"id": id, "messages": messages}).to_string(),
                            )
                        } else {
                            (
                                "404 Not Found",
                                "application/json",
                                serde_json::json!({"error": "thread not found"}).to_string(),
                            )
                        }
                    }
                    "/api/threads" => {
                        let threads = list_threads(&root);
                        (
                            "200 OK",
                            "application/json",
                            serde_json::to_string(&threads).unwrap_or_default(),
                        )
                    }
                    _ => ("404 Not Found", "text/plain", "not found".to_string()),
                }
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\n\
                 Content-Type: {ctype}\r\n\
                 Content-Length: {}\r\n\
                 Content-Security-Policy: default-src 'self'; connect-src 'self'; \
                 img-src 'self' data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'; \
                 object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'\r\n\
                 X-Content-Type-Options: nosniff\r\n\
                 Referrer-Policy: no-referrer\r\n\
                 Cache-Control: no-store\r\n\
                 Connection: close\r\n\
                 \r\n\
                 {body}",
                body.len()
            );
            let _ = running.load(Ordering::Relaxed);
        });
    }
    Ok(())
}

/// Bounded HTTP request reader with a split-TCP body. Buffers append the
/// ACTUAL byte count only (`extend_from_slice(&buffer[..read])` — the
/// NUL-padding regression must never return) and stops once headers +
/// declared body length are complete.
fn read_http_request(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut request = Vec::new();
    loop {
        let mut buffer = [0u8; 8192];
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.len() > MAX_HTTP_REQUEST_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "HTTP request exceeds 64 KiB",
            ));
        }
        if let Some(headers_len) = header_end(&request) {
            let headers = String::from_utf8_lossy(&request[..headers_len]);
            let body_len = content_length(&headers).unwrap_or(0);
            let total = headers_len.saturating_add(body_len);
            if total > MAX_HTTP_REQUEST_BYTES {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "HTTP body exceeds 64 KiB",
                ));
            }
            if request.len() >= total {
                request.truncate(total);
                break;
            }
        }
    }
    Ok(String::from_utf8_lossy(&request).into_owned())
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|at| at + 4)
}

fn content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })
}

/// The body (everything after the header terminator).
fn request_body(request: &str) -> Option<&str> {
    request.split_once("\r\n\r\n").map(|(_, body)| body)
}

/// A named HTTP header value (request line excluded).
fn request_header<'a>(request: &'a str, expected: &str) -> Option<&'a str> {
    let headers = request
        .split_once("\r\n\r\n")
        .map_or(request, |(head, _)| head);
    headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(expected).then_some(value.trim())
    })
}

/// One HITL action result.
struct ActResult {
    ok: bool,
    output: String,
}

/// A `name=value` pair from the query string.
fn query_param<'a>(path: &'a str, key: &str) -> Option<&'a str> {
    path.split_once('?')?.1.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    })
}

/// Wrap a child-process result; success = exit status, never "printed
/// anything".
fn command_result(command: &str, result: std::io::Result<std::process::Output>) -> ActResult {
    match result {
        Err(error) => ActResult {
            ok: false,
            output: format!("cannot execute {command}: {error}"),
        },
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            if text.trim().is_empty() {
                text = format!("exit {}", output.status.code().map_or(-1, |code| code));
            }
            ActResult {
                ok: output.status.success(),
                output: text.trim().to_string(),
            }
        }
    }
}

/// Execute a human-triggered kernel action from the dashboard. Every
/// route constructs fixed argv (no generic command parameter); every
/// path-bearing argument passes `plain_path_segment` before joining.
/// `act` is a pure router: unknown actions are a 400, never executed,
/// never audit-logged.
fn act(root: &Path, path: &str) -> ActResult {
    let action = path
        .strip_prefix("/api/act/")
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    let (args, command): (Vec<String>, String) = match action {
        "dream-promote" => (
            vec!["dream".to_string(), "--promote".to_string()],
            "mini-agi dream --promote".to_string(),
        ),
        "dream-idle" => (
            vec!["dream".to_string(), "--idle".to_string()],
            "mini-agi dream --idle".to_string(),
        ),
        "mem-verify" => (
            vec!["mem".to_string(), "verify".to_string()],
            "mini-agi mem verify".to_string(),
        ),
        "run-verify" => {
            let Some(case) = query_param(path, "case") else {
                return ActResult {
                    ok: false,
                    output: "run verify: missing case".to_string(),
                };
            };
            if !crate::status::plain_path_segment(case) {
                return ActResult {
                    ok: false,
                    output: "run verify: case must be a plain path segment".to_string(),
                };
            }
            let relative = Path::new("evals/cases").join(case).join("run.json");
            if !root.join(&relative).is_file() {
                return ActResult {
                    ok: false,
                    output: format!("run verify: run not found: {}", relative.display()),
                };
            }
            let command = format!("mini-agi run verify {}", relative.display());
            (
                vec![
                    "run".to_string(),
                    "verify".to_string(),
                    relative.to_string_lossy().into_owned(),
                ],
                command,
            )
        }
        "signoff" => {
            let day = query_param(path, "day");
            let file = query_param(path, "file");
            let index = query_param(path, "i");
            let day_ok = day.is_none() || crate::status::plain_path_segment(day.unwrap_or(""));
            let (Some(file), Some(index)) = (file, index) else {
                return ActResult {
                    ok: false,
                    output: "signoff: expect file, optional day, and positive index".to_string(),
                };
            };
            if !crate::status::plain_path_segment(file)
                || !day_ok
                || index.is_empty()
                || !index.bytes().all(|byte| byte.is_ascii_digit())
                || index.parse::<usize>().ok().is_none_or(|value| value == 0)
            {
                return ActResult {
                    ok: false,
                    output: "signoff: day/file must be plain segments and index must be positive"
                        .to_string(),
                };
            }
            let relative = day.map_or_else(
                || Path::new("memory/review").join(file),
                |day| Path::new("memory/review").join(day).join(file),
            );
            if !root.join(&relative).is_file() {
                return ActResult {
                    ok: false,
                    output: format!("signoff: queue not found: {}", relative.display()),
                };
            }
            let command = format!("mini-agi mem signoff {} {index}", relative.display());
            (
                vec![
                    "mem".to_string(),
                    "signoff".to_string(),
                    relative.to_string_lossy().into_owned(),
                    index.to_string(),
                ],
                command,
            )
        }
        _ => {
            return ActResult {
                ok: false,
                output: format!("unknown action: {action}"),
            };
        }
    };

    // Validation is complete: only now is an attempted action
    // audit-logged, and only then executed.
    let _ = mini_agi_core::audit::append_action(root, "ui", "human", &command);
    let result = std::env::current_exe().and_then(|executable| {
        std::process::Command::new(executable)
            .args(&args)
            .current_dir(root)
            .output()
    });
    let response = command_result(&command, result);
    if action == "mem-verify" {
        crate::status::invalidate_memory_health(root);
    }
    response
}

/// Chat result: the agent's reply, the error, and the resolved thread id
/// so the browser can continue the same conversation.
struct ChatResult {
    ok: bool,
    output: String,
    thread: Option<String>,
}

/// One chat message in a thread (persisted JSONL, append-only).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ChatMessage {
    role: String,
    text: String,
    ts: String,
}

/// The POST /api/chat JSON body.
#[derive(serde::Deserialize)]
struct ChatRequest {
    message: String,
    thread: Option<String>,
}

/// The chat worker argv: opencode `run --agent plan --format default
/// -- <prompt>`. Plan agent = read-only (no edit tools), default format
/// = clean text answer (not the JSON event stream the `--format json`
/// worker path produces).
///
/// NO explicit `-m`: with `--agent plan` a pinned model id makes the
/// opencode server fail with "Unexpected server error" (observed
/// 2026-08-09 on opencode 1.18.11 — reproduced bare, no sandbox); the
/// plan agent resolves its own default model and answers fine.
fn chat_worker_args(prompt: &str) -> Vec<String> {
    vec![
        "run".to_string(),
        "--agent".to_string(),
        "plan".to_string(),
        "--format".to_string(),
        "default".to_string(),
        "--".to_string(),
        prompt.to_string(),
    ]
}

/// The chat thread store: `memory/episodic/chat/<thread-id>.jsonl`.
///
/// Threads are first-class: a conversation continues by id, each turn
/// re-seeds the worker with the FULL thread transcript (bounded to the
/// latest `CHAT_CONTEXT_TURNS` turns) plus the canonical memory resume
/// block. Files are append-only JSONL (one message per line) like the
/// checkpoint journal; thread ids are plain segments (no traversal).
const CHAT_DIR_REL: &str = "memory/episodic/chat";
const CHAT_CONTEXT_TURNS: usize = 20;

fn chat_thread_dir(root: &Path) -> std::path::PathBuf {
    root.join(CHAT_DIR_REL)
}

/// A new thread id: nanosecond timestamp, plain segment (same-seconds
/// collisions impossible).
fn new_thread_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("chat-{nanos}")
}

/// List threads (id, first-message title, message count), newest by
/// JSONL mtime first.
fn list_threads(root: &Path) -> Vec<(String, String, usize)> {
    let dir = chat_thread_dir(root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<(u64, String, String, usize)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
            && let Some(id) = path.file_stem().and_then(|stem| stem.to_str())
            && crate::status::plain_path_segment(id)
            && let Some(messages) = read_thread(root, id)
        {
            let title = messages
                .iter()
                .find(|message| message.role == "human")
                .map_or_else(
                    || id.to_string(),
                    |message| message.text.chars().take(48).collect::<String>(),
                );
            out.push((
                path_modified_ms(&path).unwrap_or(0),
                id.to_string(),
                title,
                messages.len(),
            ));
        }
    }
    out.sort_by_key(|(modified, _, _, _)| std::cmp::Reverse(*modified));
    out.into_iter()
        .map(|(_, id, title, count)| (id, title, count))
        .collect()
}

/// Read a thread's messages; `None` when the thread does not exist or
/// its id is not a plain segment.
fn read_thread(root: &Path, id: &str) -> Option<Vec<ChatMessage>> {
    if !crate::status::plain_path_segment(id) {
        return None;
    }
    let path = chat_thread_dir(root).join(format!("{id}.jsonl"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return None;
    };
    Some(
        text.lines()
            .filter_map(|line| serde_json::from_str::<ChatMessage>(line).ok())
            .collect(),
    )
}

/// Append one message to a thread (create the dir/file as needed).
fn append_thread_message(root: &Path, id: &str, role: &str, text: &str) -> std::io::Result<()> {
    let dir = chat_thread_dir(root);
    std::fs::create_dir_all(&dir)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(
            |_| "unknown".to_string(),
            |duration| duration.as_secs().to_string(),
        );
    let msg = serde_json::to_string(&ChatMessage {
        role: role.to_string(),
        text: text.to_string(),
        ts,
    })?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{id}.jsonl")))?;
    writeln!(f, "{msg}")
}

/// One dashboard chat turn: append the human message, seed a bounded
/// opencode worker with the thread's own history (bounded turns) plus
/// the kernel memory resume block, append the reply and return the
/// resolved thread id. `thread` is optional: a fresh thread is created
/// for the first message.
fn chat_with_kernel(root: &Path, thread: Option<&str>, message: &str) -> ChatResult {
    let message = message.trim();
    if message.is_empty() {
        return ChatResult {
            ok: false,
            output: "empty message".into(),
            thread: None,
        };
    }
    // Resolve (or create) the thread FIRST so the human message lands in
    // storage before any worker call — a crashed worker still leaves the
    // conversation intact for retry.
    let thread_id = match thread {
        Some(id) if crate::status::plain_path_segment(id) => id.to_string(),
        Some(_) => {
            return ChatResult {
                ok: false,
                output: "invalid thread id".into(),
                thread: None,
            };
        }
        None => new_thread_id(),
    };
    if let Err(e) = append_thread_message(root, &thread_id, "human", message) {
        return ChatResult {
            ok: false,
            output: format!("thread write failed: {e}"),
            thread: None,
        };
    }
    // Context: canonical memory resume block + the thread's own history
    // (bounded). The transcript makes the conversation CONTINUE — the
    // agent sees what it said before instead of answering in a vacuum.
    let context = mini_agi_core::insights::resume(root).unwrap_or_else(|_| String::new());
    let history: Vec<ChatMessage> = read_thread(root, &thread_id)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .take(CHAT_CONTEXT_TURNS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut transcript = String::new();
    for message in &history {
        let role = if message.role == "human" {
            "HUMAN"
        } else {
            "AGENT"
        };
        let _ = writeln!(transcript, "{role}: {}", message.text);
    }
    let prompt = format!(
        "You are mini-agi's in-dashboard assistant — a senior engineer \
         working INSIDE this repo, memory-anchored (ADR-0003).\n\n\
         CONTEXT FROM CANONICAL MEMORY:\n{context}\n\n\
         CONVERSATION SO FAR (latest {CHAT_CONTEXT_TURNS} turns):\n{transcript}\n\n\
         Continue the conversation. The human just wrote:\n\n\
         HUMAN: {message}\n\n\
         Answer concisely and concretely: state facts (with evidence from \
         the context or the repo when you can), propose the next action, \
         and NEVER invent facts — say 'unknown' when the context does not \
         answer."
    );
    // The worker runs IN THE REPO ROOT (not a temp dir) so it reads the
    // real canonical memory, journal and brief. Plan agent is read-only
    // by design, so running in-root cannot mutate anything.
    let workdir = root.to_path_buf();
    let args = chat_worker_args(&prompt);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = crate::worker::run_worker_sandboxed(
        "opencode-opencode-go/deepseek-v4-flash",
        &workdir,
        false,
        true,
        Some(120),
        None,
        &arg_refs,
    );
    let output = match result {
        Ok(w) => {
            let output = w.output;
            if w.status == Some(0) {
                output
            } else {
                format!("worker exited {:?} — {output}", w.status)
            }
        }
        Err(e) => format!("worker not available: {e}"),
    };
    if let Err(e) = append_thread_message(root, &thread_id, "agent", &output) {
        return ChatResult {
            ok: false,
            output: format!("thread write failed: {e}"),
            thread: Some(thread_id),
        };
    }
    ChatResult {
        ok: !output.trim().is_empty(),
        output: output.trim().to_string(),
        thread: Some(thread_id),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_serves_self_refreshing_supervised_markup() {
        assert!(INDEX_HTML.contains("/api/status"));
        assert!(INDEX_HTML.contains("schema_version"));
        assert!(INDEX_HTML.contains("poll()"));
        assert!(INDEX_HTML.contains("Attention"));
        assert!(INDEX_HTML.contains("Brain &amp; Staging") || INDEX_HTML.contains("Brain"));
        assert!(INDEX_HTML.contains("Copy"));
        assert!(INDEX_HTML.contains("X-Mini-Agi-UI"));
        assert!(
            !INDEX_HTML.contains("onclick"),
            "no inline handlers allowed"
        );
    }

    #[test]
    fn act_router_rejects_unknown_and_malformed() {
        let root = std::env::temp_dir().join(format!("mag-ui-act-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let r = act(&root, "/api/act/rm-rf");
        assert!(!r.ok);
        assert!(r.output.contains("unknown action"));
        // signoff: missing/empty params fail closed.
        let r = act(&root, "/api/act/signoff?day=2026-08-09&file=none.md&i=abc");
        assert!(!r.ok);
        assert!(r.output.contains("bad signoff") || r.output.contains("index must be positive"));
        let r = act(&root, "/api/act/signoff?day=2026-08-09&file=none.md&i=");
        assert!(!r.ok);
        // signoff: zero index is rejected.
        let r = act(&root, "/api/act/signoff?day=2026-08-09&file=none.md&i=0");
        assert!(!r.ok);
        // signoff: a missing queue file must fail closed.
        let r = act(&root, "/api/act/signoff?day=2026-08-09&file=none.md&i=1");
        assert!(!r.ok, "missing queue file must fail closed");
        assert!(r.output.contains("queue not found"));
        // Root queues (no day subdir) sign off directly under memory/review/.
        let r = act(&root, "/api/act/signoff?file=none-root.md&i=1");
        assert!(!r.ok, "missing root queue file must fail closed");
        assert!(r.output.contains("queue not found"));
        // Traversal: day/file must stay plain segments.
        let r = act(&root, "/api/act/signoff?day=..&file=passwd&i=1");
        assert!(!r.ok, "traversal day must be rejected");
        assert!(r.output.contains("plain segments"));
        let r = act(&root, "/api/act/run-verify");
        assert!(!r.ok, "run-verify without case must fail");
        let r = act(&root, "/api/act/run-verify?case=../etc/passwd");
        assert!(!r.ok, "traversal case must be rejected");
        let r = act(&root, "/api/act/run-verify?case=nope");
        assert!(!r.ok, "missing run must fail");
        assert!(r.output.contains("run not found"));
    }

    #[test]
    fn query_param_parses_name_value_pairs() {
        assert_eq!(
            query_param("/api/act/signoff?day=2026-08-09&file=a.md&i=3", "day"),
            Some("2026-08-09")
        );
        assert_eq!(
            query_param("/api/act/signoff?day=2026-08-09&file=a.md&i=3", "file"),
            Some("a.md")
        );
        assert_eq!(
            query_param("/api/act/signoff?day=2026-08-09&file=a.md&i=3", "i"),
            Some("3")
        );
        assert_eq!(
            query_param("/api/act/signoff?day=2026-08-09&file=a.md&i=3", "nope"),
            None
        );
        assert_eq!(query_param("/api/act/signoff", "day"), None);
    }

    #[test]
    fn read_http_request_slices_to_actual_length() {
        use std::io::Write as _;
        // Regression (observed live): a short POST body used to arrive
        // padded with ~8k of trailing NULs (the reader parsed the whole
        // zero-initialized buffer). The bounded reader appends only the
        // ACTUAL bytes and stops at the declared Content-Length.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_http_request(&mut stream)
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let body = r#"{"message":"hello kernel","thread":null}"#;
        let request = format!(
            "POST /api/chat HTTP/1.1\r\n\
             Host: localhost\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             X-Mini-Agi-UI: 1\r\n\
             \r\n\
             {body}",
            body.len()
        );
        client.write_all(request.as_bytes()).unwrap();
        drop(client);
        let req = handle.join().unwrap().unwrap();
        assert!(!req.contains('\0'), "no NUL padding in the parsed request");
        let parsed: ChatRequest = serde_json::from_str(request_body(&req).unwrap()).unwrap();
        assert_eq!(parsed.message, "hello kernel");
        assert_eq!(parsed.thread, None);
        assert_eq!(request_header(&req, "x-mini-agi-ui"), Some("1"));
    }

    #[test]
    fn request_header_case_insensitive_and_body_splitting() {
        let request = "POST /api/x HTTP/1.1\r\nContent-Length: 4\r\nX-Mini-Agi-UI: 1\r\n\r\nbody";
        assert_eq!(request_header(request, "X-Mini-Agi-UI"), Some("1"));
        assert_eq!(request_header(request, "content-length"), Some("4"));
        assert_eq!(request_header(request, "missing"), None);
        assert_eq!(request_body(request), Some("body"));
        assert_eq!(request_body("GET / HTTP/1.1"), None);
        assert_eq!(content_length("Content-Length: 4\r\n"), Some(4));
        assert_eq!(content_length("no length here"), None);
    }

    #[test]
    fn chat_threads_append_list_and_reject_traversal() {
        let root = std::env::temp_dir().join(format!("mag-ui-chat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert!(list_threads(&root).is_empty(), "no threads yet");
        append_thread_message(&root, "chat-1", "human", "hello").unwrap();
        append_thread_message(&root, "chat-1", "agent", "hi").unwrap();
        append_thread_message(&root, "chat-2", "human", "other").unwrap();
        let threads = list_threads(&root);
        assert_eq!(threads.len(), 2, "both threads listed");
        let msgs = read_thread(&root, "chat-1").unwrap();
        assert_eq!(msgs.len(), 2, "thread transcript persisted");
        assert_eq!(msgs[0].role, "human");
        assert_eq!(msgs[1].role, "agent");
        assert!(msgs[1].text.contains("hi"));
        // Traversal and absolute paths are rejected (plain segments only).
        assert!(read_thread(&root, "../etc/passwd").is_none());
        assert!(read_thread(&root, "/tmp/x").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn chat_parse_outgrows_http_formats() {
        // The 2026-08 redesign moved chat to a structured JSON body; the
        // old header format (x-message/x-thread lines) must not regress
        // into the visible surface.
        let body = r#"{"message":"hello kernel","thread":null}"#;
        let parsed: ChatRequest = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.message, "hello kernel");
        let body2 = r#"{"message":"continue here","thread":"chat-42"}"#;
        let parsed2: ChatRequest = serde_json::from_str(body2).unwrap();
        assert_eq!(parsed2.thread.as_deref(), Some("chat-42"));
    }

    #[test]
    fn api_payload_carries_schema_runs_journal_and_memory() {
        let root = std::env::temp_dir().join(format!(
            "mag-ui-payload-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let run = root.join("evals/cases/a/run.json");
        std::fs::create_dir_all(run.parent().unwrap()).unwrap();
        std::fs::write(
            &run,
            r#"{"goal":"g","scope":["x"],"outcome":{"achieved":true},"tokens_total":1,"cost_usd":0.01,"golden":null,"verify_command":null,"verify_target":null,"trajectory":[{"step":1,"tool":"read","ok":true,"goal_aligned":true,"tokens":1,"output_tokens":1}]}"#,
        )
        .unwrap();
        let payload = api_payload(&root);
        assert_eq!(payload.schema_version, 2, "redesigned schema version");
        assert_eq!(
            payload.runs["total_runs"].as_u64(),
            Some(1),
            "payload must carry the run index"
        );
        assert_eq!(
            payload.totals["runs"].as_u64(),
            Some(1),
            "totals must carry the run count"
        );
        assert!(
            payload.queues.is_empty(),
            "queues field must be present (empty by default)"
        );
        assert_eq!(
            payload.journal.state, "absent",
            "no journal in an empty root"
        );
        assert_eq!(
            payload.memory["verify"].as_str(),
            Some("unknown"),
            "missing canonical entries must report unknown, never ok"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn staging_classifies_next_to_apply_and_receipt_state() {
        let root = std::env::temp_dir().join(format!("mag-ui-stage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("memory/staging/2026-08-09")).unwrap();
        let staged = root.join("memory/staging/2026-08-09/facts.md");
        std::fs::write(
            &staged,
            "## S-001 (general)\n- body: staged fact one\n- domain: general\n",
        )
        .unwrap();
        std::fs::write(
            staged.with_extension("verdicts.json"),
            r#"[{"index":0,"verdict":"promote","reason":"ok"}]"#,
        )
        .unwrap();
        let payload = api_payload(&root);
        let files: Vec<&serde_json::Value> = payload
            .staging
            .iter()
            .filter(|s| s["name"].as_str() == Some("facts.md"))
            .collect();
        assert_eq!(files.len(), 1, "staging file discovered");
        assert_eq!(
            files[0]["state"].as_str(),
            Some("pending"),
            "a covered batch with no receipt is pending"
        );
        assert!(
            files[0]["is_next_to_apply"].as_bool().unwrap_or(false),
            "the only pending batch is next to apply"
        );
        assert!(
            files[0]["execute_path"].is_string(),
            "actionable batch has a route"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
