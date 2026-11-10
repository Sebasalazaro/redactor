<script lang="ts">
  // The words redactor hides, for the active engagement or for every one.
  // Client names, custom terms, users and hosts live in separate lists in the
  // config; here they are one list, each word with what it becomes.
  import { api, type AppSettings, type Config, type Engagement } from "../../lib/api";
  import type { Autosave } from "../../lib/autosave.svelte";
  import Icon from "../../lib/Icon.svelte";
  import NewEngagement from "../components/NewEngagement.svelte";
  import RulesEditor from "../components/RulesEditor.svelte";

  let {
    settings,
    global = $bindable(),
    engagements,
    autosave,
    onactivate,
    oncreated,
    onchanged,
  }: {
    settings: AppSettings;
    global: Config;
    engagements: Engagement[];
    autosave: Autosave;
    onactivate: (id: string | null) => void;
    oncreated: (id: string, activate: boolean) => void;
    onchanged: () => void;
  } = $props();

  const PRESETS = ["[CLIENT]", "[APP]", "[SYSTEM]", "[HOST]", "[USER]", "[REDACTED]"];
  const CLIENT = "[CLIENT]";
  const DEFAULT = "[REDACTED]";

  const active = $derived(engagements.find((e) => e.id === settings.active_engagement) ?? null);

  // The active engagement's own config, loaded and saved here.
  let engagement = $state<Config | null>(null);
  let loadedId = $state<string | null>(null);

  $effect(() => {
    const id = active?.id ?? null;
    if (id === loadedId) return;
    loadedId = id;
    engagement = null;
    if (!id) return;
    api.getEngagement(id).then((loaded) => {
      if (loadedId !== id) return;
      autosave.mark(`engagement:${id}`, loaded);
      engagement = loaded;
    });
  });

  $effect(() => {
    if (!engagement || !loadedId) return;
    const id = loadedId;
    const value = $state.snapshot(engagement);
    autosave.track(`engagement:${id}`, value, async () => {
      await api.saveEngagement(id, value);
      onchanged();
    });
  });

  // Where new words go: the active engagement unless "Global" is picked.
  let scope = $state<"engagement" | "global">("engagement");
  const target = $derived(scope === "engagement" && active ? engagement : global);
  const onEngagement = $derived(scope === "engagement" && !!active);

  type Row =
    | { kind: "client"; value: string }
    | { kind: "term"; value: string; index: number }
    | { kind: "user" | "host"; value: string };

  function rows(c: Config): Row[] {
    return [
      ...c.client.map((value) => ({ kind: "client" as const, value })),
      ...c.terms.map((t, index) => ({ kind: "term" as const, value: t.value, index })),
      ...c.users.map((value) => ({ kind: "user" as const, value })),
      ...c.hosts.map((value) => ({ kind: "host" as const, value })),
    ].filter((r) => r.value.trim());
  }

  let filter = $state("");
  const visible = $derived(
    target
      ? rows(target).filter((r) => r.value.toLowerCase().includes(filter.trim().toLowerCase()))
      : [],
  );
  const inherited = $derived(onEngagement ? rows(global) : []);

  // The add row.
  let word = $state("");
  let replacement = $state(DEFAULT);
  let error = $state("");
  let wordInput = $state<HTMLInputElement | null>(null);

  function has(c: Config, value: string) {
    const v = value.toLowerCase();
    return rows(c).some((r) => r.value.toLowerCase() === v);
  }

  function add(e?: SubmitEvent) {
    e?.preventDefault();
    const value = word.trim();
    if (!value || !target) return;
    if (has(target, value)) {
      error = `${value} is already on this list`;
      return;
    }
    const tag = replacement.trim() || DEFAULT;
    if (tag === CLIENT) target.client.push(value);
    else target.terms.push({ value, replacement: tag === DEFAULT ? null : tag });
    word = "";
    error = "";
    wordInput?.focus();
  }

  function remove(row: Row) {
    if (!target) return;
    if (row.kind === "term") target.terms.splice(row.index, 1);
    else {
      const list = row.kind === "client" ? target.client : row.kind === "user" ? target.users : target.hosts;
      const i = list.indexOf(row.value);
      if (i >= 0) list.splice(i, 1);
    }
  }

  function tagOf(row: Row, c: Config) {
    if (row.kind === "client") return CLIENT;
    if (row.kind === "term") return c.terms[row.index].replacement ?? DEFAULT;
    return null;
  }

  // Engagement management.
  let creating = $state(false);
  let confirmDelete = $state(false);

  async function deleteActive() {
    if (!active) return;
    if (!confirmDelete) {
      confirmDelete = true;
      setTimeout(() => (confirmDelete = false), 4000);
      return;
    }
    const id = active.id;
    confirmDelete = false;
    await onactivate(null);
    await api.deleteEngagement(id);
    onchanged();
  }
</script>

<header class="page-head">
  <h2>Terms</h2>
  <div class="profile">
    <label class="select">
      <Icon name={active ? "folder" : "globe"} />
      <select
        value={settings.active_engagement ?? ""}
        onchange={(e) => {
          scope = "engagement";
          onactivate(e.currentTarget.value || null);
        }}
        aria-label="Active engagement"
      >
        <option value="">No engagement</option>
        {#each engagements as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
      </select>
    </label>
    <button class="btn" onclick={() => (creating = !creating)}>
      <Icon name={creating ? "x" : "plus"} />
      {creating ? "Cancel" : "New engagement"}
    </button>
  </div>
</header>

{#if creating}
  <div class="card creating">
    <NewEngagement
      oncreated={(id, activate) => {
        creating = false;
        scope = "engagement";
        oncreated(id, activate);
      }}
    />
  </div>
{/if}

<div class="scope" role="tablist" aria-label="Where new words are saved">
  {#if active}
    <button role="tab" aria-selected={scope === "engagement"} onclick={() => (scope = "engagement")}>
      <Icon name="folder" />
      {active.name}
    </button>
  {/if}
  <button role="tab" aria-selected={!onEngagement} onclick={() => (scope = "global")}>
    <Icon name="globe" />
    All engagements
  </button>
  <p class="muted scope-hint">
    {#if onEngagement}
      Hidden only while {active?.name} is active
    {:else}
      Hidden in every engagement
    {/if}
  </p>
</div>

<form class="add" onsubmit={add}>
  <input
    bind:this={wordInput}
    bind:value={word}
    oninput={() => (error = "")}
    placeholder="Word to hide"
    aria-label="Word to hide"
    autocomplete="off"
    spellcheck="false"
  />
  <span class="arrow"><Icon name="arrow" /></span>
  <input
    class="mono"
    bind:value={replacement}
    placeholder={DEFAULT}
    aria-label="Replace it with"
    autocomplete="off"
    spellcheck="false"
  />
  <button class="btn primary" type="submit" disabled={!word.trim() || !target}>
    <Icon name="plus" /> Add
  </button>
  <div class="presets" role="group" aria-label="Common replacements">
    {#each PRESETS as preset (preset)}
      <button
        type="button"
        class="preset"
        class:on={replacement === preset}
        aria-pressed={replacement === preset}
        onclick={() => {
          replacement = preset;
          wordInput?.focus();
        }}>{preset}</button
      >
    {/each}
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if replacement === CLIENT}
    <p class="muted hint">Client names also match their other spellings: Globex Bank, globex-bank, GlobexBank</p>
  {/if}
</form>

{#if target}
  {@const all = rows(target)}
  <section class="list" aria-label="Hidden words">
    <div class="list-head">
      <span>{all.length} {all.length === 1 ? "word" : "words"}</span>
      {#if all.length > 8}
        <label class="filter">
          <Icon name="search" size={14} />
          <input bind:value={filter} placeholder="Filter" aria-label="Filter words" />
        </label>
      {/if}
    </div>
    {#each visible as row (row.kind + row.value + (row.kind === "term" ? row.index : ""))}
      {@const tag = tagOf(row, target)}
      <div class="row">
        <span class="word">{row.value}</span>
        <span class="arrow"><Icon name="arrow" size={14} /></span>
        {#if row.kind === "term"}
          <input
            class="tag-input"
            aria-label="Replacement for {row.value}"
            value={target.terms[row.index].replacement ?? ""}
            size={Math.max(DEFAULT.length, (target.terms[row.index].replacement ?? "").length) + 1}
            placeholder={DEFAULT}
            oninput={(e) => target && (target.terms[row.index].replacement = e.currentTarget.value || null)}
          />
        {:else if tag}
          <span class="tag">{tag}</span>
        {:else}
          <span class="partial" title="Listed as a {row.kind}: part of each word stays visible">partly hidden</span>
        {/if}
        <button class="icon-btn" aria-label="Stop hiding {row.value}" onclick={() => remove(row)}>
          <Icon name="x" size={14} />
        </button>
      </div>
    {:else}
      <p class="empty muted">
        {filter ? "No word matches the filter" : "Nothing here yet. Type a word above and press Enter"}
      </p>
    {/each}
  </section>

  {#if inherited.length}
    <details class="inherited">
      <summary>
        <Icon name="chevron" size={14} />
        {inherited.length} more from all engagements
      </summary>
      <div class="inherited-list">
        {#each inherited as row (row.kind + row.value)}
          {@const tag = tagOf(row, global)}
          <span class="pill">{row.value}<span class="muted">{tag ?? "partly hidden"}</span></span>
        {/each}
      </div>
    </details>
  {/if}

  <details class="advanced">
    <summary>
      <Icon name="chevron" size={14} />
      Advanced rules {onEngagement ? `for ${active?.name}` : "for all engagements"}
    </summary>
    <div class="advanced-body">
      {#if onEngagement && engagement}
        <RulesEditor bind:config={engagement} scope="engagement" />
      {:else}
        <RulesEditor bind:config={global} scope="global" />
      {/if}
    </div>
  </details>

  {#if onEngagement && active}
    <div class="danger-zone">
      <button class="btn danger" onclick={deleteActive}>
        <Icon name="trash" />
        {confirmDelete ? `Click again to delete ${active.name}` : "Delete engagement"}
      </button>
    </div>
  {/if}
{:else}
  <p class="muted">Loading…</p>
{/if}

<style>
  .page-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 20px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
  }
  .profile {
    display: flex;
    gap: 8px;
  }
  .select {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 4px 0 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--muted);
  }
  .select select {
    height: 32px;
    min-width: 180px;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
  }
  .creating {
    margin-bottom: 20px;
    padding: 16px;
  }
  .scope {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 12px;
  }
  .scope button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .scope button:hover {
    color: var(--text);
  }
  .scope button[aria-selected="true"] {
    border-color: var(--border);
    background: var(--surface);
    color: var(--text);
    font-weight: 500;
  }
  .scope-hint {
    margin: 0 0 0 8px;
    font-size: 12px;
  }

  .add {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 220px) auto;
    gap: 8px;
    align-items: center;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
  }
  .add input {
    height: 36px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    font-size: 14px;
  }
  .add input:focus {
    border-color: var(--accent);
    outline: none;
  }
  .add .btn {
    height: 36px;
  }
  .mono,
  .tag-input {
    font-family: var(--mono);
  }
  .arrow {
    display: flex;
    color: var(--muted);
  }
  .presets {
    grid-column: 3 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .preset {
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: none;
    color: var(--muted);
    font: 11px var(--mono);
    cursor: pointer;
  }
  .preset:hover {
    color: var(--text);
    border-color: var(--muted);
  }
  .preset.on {
    border-color: var(--redact);
    background: var(--redact);
    color: var(--redact-fg);
  }
  .error,
  .hint {
    grid-column: 1 / -1;
    margin: 0;
    font-size: 12px;
  }
  .error {
    color: var(--danger);
  }

  .list {
    margin-top: 24px;
  }
  .list-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 32px;
    margin-bottom: 4px;
    color: var(--muted);
    font-size: 12px;
  }
  .filter {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
  }
  .filter input {
    width: 160px;
    height: 28px;
    border: 0;
    outline: 0;
    background: transparent;
    font-size: 12px;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 16px minmax(0, 220px) 28px;
    gap: 12px;
    align-items: center;
    min-height: 40px;
    padding: 0 4px 0 12px;
    border-bottom: 1px solid var(--border);
  }
  .row:hover {
    background: var(--surface);
  }
  .word {
    overflow: hidden;
    font: 13px var(--mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag,
  .tag-input {
    justify-self: start;
    max-width: 100%;
    padding: 2px 8px;
    border: 0;
    border-radius: 4px;
    background: var(--redact);
    color: var(--redact-fg);
    font-size: 12px;
  }
  .tag-input {
    width: auto;
    height: 24px;
  }
  .tag-input:focus {
    outline: 2px solid var(--focus);
  }
  .tag-input::placeholder {
    color: var(--redact-fg);
    opacity: 0.6;
  }
  .partial {
    color: var(--muted);
    font-size: 12px;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }
  .icon-btn:hover {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .empty {
    padding: 16px 12px;
    font-size: 13px;
  }

  details {
    margin-top: 20px;
  }
  summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 13px;
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary:hover {
    color: var(--text);
  }
  details[open] > summary :global(.icon) {
    transform: rotate(90deg);
  }
  .inherited-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 12px;
  }
  .pill {
    display: inline-flex;
    gap: 8px;
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    font: 12px var(--mono);
  }
  .advanced-body {
    max-width: 720px;
    margin-top: 16px;
  }
  .danger-zone {
    margin-top: 32px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }
  .btn :global(.icon) {
    margin-right: 2px;
  }

  @media (max-width: 720px) {
    .add {
      grid-template-columns: 1fr;
    }
    .add .arrow {
      display: none;
    }
    .presets {
      grid-column: 1;
    }
    .page-head {
      flex-wrap: wrap;
    }
  }
</style>
