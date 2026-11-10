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
  import { ago, format, hotkey } from "../../lib/format";
  import Icon from "../../lib/Icon.svelte";

  let {
    settings,
    global,
    engagements,
    onterms,
    onsettings,
  }: {
    settings: AppSettings;
    global: Config;
    engagements: Engagement[];
    onterms: () => void;
    onsettings: () => void;
  } = $props();

  let overview = $state<Overview | null>(null);
  let copied = $state(false);

  const active = $derived(engagements.find((e) => e.id === settings.active_engagement) ?? null);
  const last = $derived(overview?.last ?? null);
  const found = $derived(last?.categories.reduce((n, [, c]) => n + c, 0) ?? 0);

  const words = (c: Config) => c.client.length + c.terms.length + c.users.length + c.hosts.length;
  const total = $derived(words(global) + (active ? words(active.config) : 0));

  const FORGOTTEN: Record<ForgottenReason, string> = {
    expired: "was forgotten automatically",
    cleared: "was forgotten",
    freed: "was cleared to free memory",
    disabled: "is not kept (turned off in Settings)",
  };

  async function refresh() {
    overview = await api.getOverview();
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

  onMount(() => {
    refresh();
    const unlisten = listen("overview-changed", refresh);
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<header>
  <div>
    <h2>{active?.name ?? "No engagement"}</h2>
    <button class="profile-line" onclick={onterms}>
      <Icon name="terms" size={14} />
      {total} {total === 1 ? "word" : "words"} hidden
      <span class="edit">Edit terms</span>
    </button>
  </div>
  <button class="btn primary redact" onclick={() => api.redactNow()}>
    <Icon name="clipboard" />
    Redact clipboard
    <kbd>{hotkey(settings.hotkey)}</kbd>
  </button>
</header>

<section class="card last" aria-label="Last redaction">
  {#if last}
    <div class="last-head">
      <h3>Last redaction</h3>
      <span class="muted">{ago(last.age_secs)}</span>
      <span class="muted">{format(last.format)}</span>
      <span class="muted">{found} {found === 1 ? "value" : "values"} hidden</span>
    </div>
    <pre>{last.output}</pre>
    <div class="foot">
      <div class="chips">
        {#each last.categories as [cat, n] (cat)}
          <span class="chip">{CATEGORY_LABELS[cat]} <span class="muted">{n}</span></span>
        {/each}
      </div>
      <div class="buttons">
        <button class="btn" onclick={forget}><Icon name="trash" /> Forget</button>
        <button class="btn primary" onclick={copyAgain}>
          <Icon name={copied ? "check" : "copy"} />
          {copied ? "Copied" : "Copy again"}
        </button>
      </div>
    </div>
  {:else}
    <div class="empty">
      {#if overview?.forgotten}
        <p>The last redaction {FORGOTTEN[overview.forgotten.reason]} {ago(overview.forgotten.secs_ago)}</p>
      {:else}
        <p>Nothing redacted yet</p>
      {/if}
      <p class="muted">
        Copy some traffic and press <kbd>{hotkey(settings.hotkey)}</kbd> from any app
      </p>
      {#if overview?.forgotten?.reason === "expired" || !settings.keep_last}
        <button class="link" onclick={onsettings}>Change how long it is kept</button>
      {/if}
    </div>
  {/if}
</section>

<style>
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 20px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 20px;
    font-weight: 600;
  }
  .profile-line {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .profile-line .edit {
    margin-left: 4px;
    color: var(--accent-strong);
  }
  .profile-line:hover .edit {
    text-decoration: underline;
  }
  .redact {
    height: 40px;
    padding: 0 16px;
    font-size: 14px;
  }
  /* Fills the window: long output scrolls inside the card. */
  .last {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 16px;
  }
  .last-head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 12px;
    font-size: 12px;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  pre {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 12px;
    overflow: auto;
    border-radius: 8px;
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
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .buttons {
    display: flex;
    flex-shrink: 0;
    gap: 8px;
  }
  .empty {
    flex: 1;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    text-align: center;
  }
  .empty p {
    margin: 0;
  }
</style>
