<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import {
    api,
    CATEGORY_LABELS,
    type AppSettings,
    type Config,
    type Engagement,
    type Overview,
  } from "../../lib/api";
  import { ago, bytes, duration, format, hotkey } from "../../lib/format";
  import NewEngagement from "../components/NewEngagement.svelte";

  type Section = "overview" | "engagements" | "global" | "playground" | "settings";

  let {
    settings,
    global,
    engagements,
    onnavigate,
    onactivate,
    oncreated,
  }: {
    settings: AppSettings;
    global: Config;
    engagements: Engagement[];
    onnavigate: (section: Section, engagement?: string) => void;
    onactivate: (id: string | null) => void;
    oncreated: (id: string, activate: boolean) => void;
  } = $props();

  let overview = $state<Overview | null>(null);
  let notice = $state("");
  let copied = $state(false);

  const active = $derived(engagements.find((e) => e.id === settings.active_engagement) ?? null);
  const systemPct = $derived(
    overview ? Math.round((overview.memory.system_used / overview.memory.system_total) * 100) : 0,
  );
  const appTotal = $derived(
    overview ? overview.memory.app_bytes + overview.memory.webview_bytes : 0,
  );
  const lastLines = $derived(overview?.last?.output.split("\n").length ?? 0);
  const findings = $derived(overview?.last?.categories.reduce((n, [, c]) => n + c, 0) ?? 0);

  function count(config: Config) {
    return (
      config.client.length +
      config.hosts.length +
      config.users.length +
      config.terms.length +
      config.sensitive_headers.length +
      config.sensitive_keys.length
    );
  }

  async function refresh() {
    overview = await api.getOverview();
  }

  async function copyAgain() {
    await api.copyLast();
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  async function clearLast() {
    await api.clearLast();
    await refresh();
  }

  async function free() {
    const before = appTotal;
    const result = await api.freeMemory();
    await refresh();
    const saved = Math.max(0, before - appTotal);
    notice = `Session data cleared · ${bytes(result.released_bytes)} returned to the system${
      saved ? ` · ${bytes(saved)} less in use` : ""
    }`;
    setTimeout(() => (notice = ""), 5000);
  }

  async function clearClipboard() {
    await api.clearClipboard();
    notice = "Clipboard cleared";
    setTimeout(() => (notice = ""), 3000);
  }

  onMount(() => {
    refresh();
    // Memory figures are cheap to read (a local syscall); refresh while visible.
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") refresh();
    }, 3000);
    const unlisten = listen("overview-changed", refresh);
    return () => {
      clearInterval(timer);
      unlisten.then((f) => f());
    };
  });
</script>

<h2>Overview</h2>

<div class="grid">
  <section class="card last">
    <h3>
      Last redaction
      {#if overview?.last}
        <span class="muted meta">
          {ago(overview.last.age_secs)} · {format(overview.last.format)} · {findings} values ·
          {engagements.find((e) => e.id === overview?.last?.engagement)?.name ?? "global"}
        </span>
      {/if}
    </h3>
    {#if overview?.last}
      <pre>{overview.last.output}</pre>
      <div class="chips cats">
        {#each overview.last.categories as [cat, n] (cat)}
          <span class="chip">{CATEGORY_LABELS[cat]} {n}</span>
        {/each}
      </div>
      <div class="actions">
        <span class="muted small">
          {lastLines} lines · only the redacted text is kept, in memory{settings.forget_last_after_minutes
            ? `, for ${settings.forget_last_after_minutes} min`
            : ""}
        </span>
        <span class="buttons">
          <button class="btn" onclick={clearLast}>Forget</button>
          <button class="btn primary" onclick={copyAgain}>{copied ? "Copied ✓" : "Copy again"}</button>
        </span>
      </div>
    {:else}
      <div class="empty">
        {#if settings.keep_last}
          <p>Nothing yet. Copy some traffic and press <kbd>{hotkey(settings.hotkey)}</kbd>.</p>
        {:else}
          <p>Keeping the last redaction is off.</p>
          <button class="link" onclick={() => onnavigate("settings")}>Change in Settings</button>
        {/if}
      </div>
    {/if}
  </section>

  <section class="card profile">
    <h3>Active profile</h3>
    <div class="stat">{active?.name ?? "Global only"}</div>
    <p class="muted small">
      {count(global)} global rules{active ? ` + ${count(active.config)} from this engagement` : ""}
    </p>
    <select
      class="text"
      value={settings.active_engagement ?? ""}
      onchange={(e) => onactivate(e.currentTarget.value || null)}
      aria-label="Active profile"
    >
      <option value="">Global only</option>
      {#each engagements as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
    </select>
    <div class="buttons">
      {#if active}
        <button class="btn small" onclick={() => onnavigate("engagements", active.id)}>Edit engagement</button>
      {/if}
      <button class="btn small" onclick={() => onnavigate("global")}>Edit global rules</button>
    </div>
    <div class="new">
      <span class="muted small">New engagement</span>
      <NewEngagement {oncreated} />
    </div>
  </section>

  <section class="card">
    <h3>Memory <span class="muted small">live</span></h3>
    {#if overview}
      <div class="stat">{bytes(appTotal)}</div>
      <p class="muted small">
        app {bytes(overview.memory.app_bytes)} · web views {bytes(overview.memory.webview_bytes)}
        ({overview.memory.webview_processes} processes)
      </p>
      <div class="bar" title="System memory in use">
        <span style:width="{systemPct}%"></span>
      </div>
      <p class="muted small">
        System: {bytes(overview.memory.system_used)} of {bytes(overview.memory.system_total)} in use ({systemPct}%)
      </p>
      <div class="buttons">
        <button class="btn small" onclick={free} title="Forget the last redaction, unload hidden windows and return freed memory"
          >Free memory</button
        >
        <button class="btn small" onclick={clearClipboard}>Clear clipboard</button>
      </div>
      {#if notice}<p class="notice">{notice}</p>{/if}
    {/if}
  </section>

  <section class="card">
    <h3>Session</h3>
    {#if overview}
      <div class="stat">{overview.redactions}</div>
      <p class="muted small">redactions since launch · running for {duration(overview.uptime_secs)}</p>
    {/if}
    <dl>
      <dt>Hotkey</dt>
      <dd><kbd>{hotkey(settings.hotkey)}</kbd></dd>
      <dt>Mode</dt>
      <dd>{settings.review ? "Review before copying" : "Replace clipboard silently"}</dd>
      <dt>Windows</dt>
      <dd>{settings.unload_windows ? "Unloaded when closed" : "Kept in memory"}</dd>
    </dl>
    <button class="link" onclick={() => onnavigate("settings")}>Settings →</button>
  </section>

  <section class="card">
    <h3>Global rules <button class="link" onclick={() => onnavigate("global")}>Edit →</button></h3>
    <p class="muted small">Apply to every engagement.</p>
    <div class="chips">
      {#each global.client as c (c)}<span class="chip">{c}</span>{:else}<span class="muted small"
          >No client names yet — add them first.</span
        >{/each}
    </div>
    <p class="muted small counts">
      {global.terms.length} terms · {global.allow_domains.length} allowed domains ·
      {global.sensitive_headers.length + global.sensitive_keys.length} extra headers/keys
    </p>
  </section>

  <section class="card">
    <h3>
      Engagements <button class="link" onclick={() => onnavigate("engagements")}>All {engagements.length} →</button>
    </h3>
    {#each engagements.slice(0, 5) as e (e.id)}
      <div class="eng">
        <button class="link" onclick={() => onnavigate("engagements", e.id)}>{e.name}</button>
        {#if e.id === settings.active_engagement}
          <span class="chip accent">active</span>
        {:else}
          <button class="btn small" onclick={() => onactivate(e.id)}>Activate</button>
        {/if}
      </div>
    {:else}
      <p class="muted small">None yet. Create one from the profile card.</p>
    {/each}
  </section>

  <section class="card wide">
    <h3>Playground <button class="link" onclick={() => onnavigate("playground")}>Open →</button></h3>
    <p class="muted small">Paste any text and see how the active profile redacts it, without touching the clipboard.</p>
  </section>
</div>

<style>
  h2 {
    margin: 0 0 16px;
    font-size: 20px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 14px;
  }
  .last {
    grid-column: span 2;
    grid-row: span 2;
    display: flex;
    flex-direction: column;
  }
  .wide {
    grid-column: span 3;
  }
  .meta {
    font-weight: 400;
    font-size: 12px;
  }
  pre {
    flex: 1;
    min-height: 140px;
    max-height: 300px;
    margin: 0;
    padding: 10px;
    overflow: auto;
    border-radius: 6px;
    background: var(--surface-2);
    font: 11.5px/1.55 var(--mono);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .cats {
    margin-top: 10px;
  }
  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: 12px;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }
  .actions .buttons {
    flex-wrap: nowrap;
    flex-shrink: 0;
    margin: 0;
  }
  .empty {
    display: grid;
    place-content: center;
    flex: 1;
    min-height: 200px;
    text-align: center;
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .profile select {
    width: 100%;
    margin-top: 4px;
  }
  .new {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    display: grid;
    gap: 6px;
  }
  .bar {
    height: 6px;
    margin: 10px 0 6px;
    border-radius: 3px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .notice {
    margin: 8px 0 0;
    color: var(--ok);
    font-size: 12px;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    margin: 12px 0;
    font-size: 12px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }
  .counts {
    margin: 10px 0 0;
  }
  .eng {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 5px 0;
  }
  .eng + .eng {
    border-top: 1px solid var(--border);
  }
  p {
    margin: 4px 0 0;
  }
</style>
