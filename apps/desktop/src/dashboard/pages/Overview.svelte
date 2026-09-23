<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import {
    api,
    CATEGORY_LABELS,
    type AppSettings,
    type Config,
    type Engagement,
    type ForgottenReason,
    type Overview,
  } from "../../lib/api";
  import { ago, bytes, format, hotkey } from "../../lib/format";
  import NewEngagement from "../components/NewEngagement.svelte";

  type Section = "overview" | "restore" | "engagements" | "global" | "playground" | "settings";

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
  let creating = $state(false);

  const active = $derived(engagements.find((e) => e.id === settings.active_engagement) ?? null);
  const last = $derived(overview?.last ?? null);
  const findings = $derived(last?.categories.reduce((n, [, c]) => n + c, 0) ?? 0);
  const systemPct = $derived(
    overview ? Math.round((overview.memory.system_used / overview.memory.system_total) * 100) : 0,
  );
  const appTotal = $derived(
    overview
      ? overview.memory.app_bytes + overview.memory.webview_bytes + overview.memory.ai_bytes
      : 0,
  );

  const FORGOTTEN: Record<ForgottenReason, string> = {
    expired: "was forgotten automatically",
    cleared: "was forgotten",
    freed: "was cleared by Free memory",
    disabled: "is not kept (turned off in Settings)",
  };

  function rules(config: Config) {
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

  function flash(text: string, ms = 4000) {
    notice = text;
    setTimeout(() => (notice = ""), ms);
  }

  async function copyAgain() {
    await api.copyLast();
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  async function forget() {
    await api.clearLast();
    await refresh();
  }

  async function free() {
    const before = appTotal;
    const result = await api.freeMemory();
    await refresh();
    const saved = Math.max(0, before - appTotal);
    flash(
      `Session data cleared · ${bytes(result.released_bytes)} returned${saved ? ` · ${bytes(saved)} less in use` : ""}`,
    );
  }

  async function clearClipboard() {
    await api.clearClipboard();
    flash("Clipboard cleared", 2500);
  }

  onMount(() => {
    refresh();
    // Memory figures are cheap to read (local syscalls); refresh while visible.
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

<header>
  <h2>Overview</h2>
  <button class="btn primary redact" onclick={() => api.redactNow()}>
    Redact clipboard <kbd>{hotkey(settings.hotkey)}</kbd>
  </button>
</header>

<div class="grid">
  <section class="card last">
    <h3>
      Last redaction
      {#if last}
        <span class="muted meta">
          {ago(last.age_secs)} · {format(last.format)} · {findings} values ·
          {engagements.find((e) => e.id === last?.engagement)?.name ?? "global"}
        </span>
      {/if}
    </h3>
    {#if last}
      <pre>{last.output}</pre>
      <div class="foot">
        <div class="chips">
          {#each last.categories as [cat, n] (cat)}
            <span class="chip">{CATEGORY_LABELS[cat]} {n}</span>
          {/each}
        </div>
        <div class="buttons">
          <button class="btn" onclick={forget}>Forget</button>
          <button class="btn primary" onclick={copyAgain}>{copied ? "Copied ✓" : "Copy again"}</button>
        </div>
      </div>
      <p class="muted small">
        Only the redacted text is kept, in memory{settings.forget_last_after_minutes
          ? ` · forgotten after ${settings.forget_last_after_minutes} min`
          : ""}.
      </p>
    {:else}
      <div class="empty">
        {#if overview?.forgotten}
          <p>
            The last redaction {FORGOTTEN[overview.forgotten.reason]}
            {ago(overview.forgotten.secs_ago)}.
          </p>
        {:else}
          <p>Nothing redacted yet in this session.</p>
        {/if}
        <p class="muted small">
          Copy some traffic and press <kbd>{hotkey(settings.hotkey)}</kbd>, or use
          <strong>Redact clipboard</strong>.
        </p>
        {#if overview?.forgotten?.reason === "expired" || !settings.keep_last}
          <button class="link" onclick={() => onnavigate("settings")}>Change how long it is kept →</button>
        {/if}
      </div>
    {/if}
  </section>

  <div class="side">
    <section class="card">
      <h3>Active profile</h3>
      <div class="stat">{active?.name ?? "Global only"}</div>
      <p class="muted small">
        {rules(global)} global rules{active ? ` + ${rules(active.config)} from this engagement` : ""}
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
        <button class="btn small" onclick={() => (creating = !creating)}>
          {creating ? "Cancel" : "+ New engagement"}
        </button>
      </div>
      {#if creating}
        <div class="new">
          <NewEngagement
            oncreated={(id, activate) => {
              creating = false;
              oncreated(id, activate);
            }}
          />
        </div>
      {/if}
    </section>

    <section class="card">
      <h3>Memory <span class="muted small">live</span></h3>
      {#if overview}
        <div class="stat">{bytes(appTotal)}</div>
        <p class="muted small">
          app {bytes(overview.memory.app_bytes)} · web views {bytes(overview.memory.webview_bytes)}
          ({overview.memory.webview_processes})
        </p>
        <div class="bar" title="System memory in use"><span style:width="{systemPct}%"></span></div>
        <p class="muted small">
          System {bytes(overview.memory.system_used)} of {bytes(overview.memory.system_total)} ({systemPct}%)
          · {overview.redactions} redactions this session
        </p>
        <p class="muted small">
          AI: {overview.ai_loaded
            ? `worker running, ${bytes(overview.memory.ai_bytes)} (ends after 2 idle minutes)`
            : overview.ai_installed
              ? "installed, not running"
              : "model not installed"}
        </p>
        <div class="buttons">
          <button
            class="btn small"
            onclick={free}
            title="Forget the last redaction, unload hidden windows and return freed memory to the system"
            >Free memory</button
          >
          <button class="btn small" onclick={clearClipboard}>Clear clipboard</button>
        </div>
        {#if notice}<p class="notice">{notice}</p>{/if}
      {/if}
    </section>
  </div>
</div>

<style>
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
  }
  .redact {
    padding: 10px 18px;
    font-size: 14px;
    border-radius: 8px;
  }
  /* Fills the window: the page never scrolls, long content scrolls inside. */
  .grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(280px, 1fr);
    gap: 14px;
  }
  .last {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    overflow: auto;
  }
  .meta {
    font-weight: 400;
    font-size: 12px;
  }
  pre {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 12px;
    overflow: auto;
    border-radius: 6px;
    background: var(--surface-2);
    font: 12px/1.6 var(--mono);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 12px;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }
  .foot .buttons {
    flex-wrap: nowrap;
    flex-shrink: 0;
    margin: 0;
  }
  .empty {
    flex: 1;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 6px;
    text-align: center;
  }
  .empty p {
    margin: 0;
  }
  .small {
    font-size: 12px;
  }
  p.small {
    margin: 6px 0 0;
  }
  select {
    width: 100%;
    margin-top: 10px;
  }
  .new {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  .bar {
    height: 6px;
    margin: 10px 0 0;
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
</style>
