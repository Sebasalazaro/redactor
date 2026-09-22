<script lang="ts">
  import { api, type AppSettings, type Config, type Engagement } from "../../lib/api";
  import type { Autosave } from "../../lib/autosave.svelte";
  import EffectiveRules from "../components/EffectiveRules.svelte";
  import NewEngagement from "../components/NewEngagement.svelte";
  import RulesEditor from "../components/RulesEditor.svelte";

  let {
    settings,
    global,
    engagements,
    editing = $bindable(),
    autosave,
    onactivate,
    oncreated,
    onchanged,
  }: {
    settings: AppSettings;
    global: Config;
    engagements: Engagement[];
    editing: string | null;
    autosave: Autosave;
    onactivate: (id: string | null) => void;
    oncreated: (id: string, activate: boolean) => void;
    onchanged: () => void;
  } = $props();

  let config = $state<Config | null>(null);
  let loadedId = $state<string | null>(null);
  let confirmDelete = $state(false);
  let creating = $state(false);

  // Load the engagement being edited whenever the selection changes.
  $effect(() => {
    const id = editing;
    if (!id) {
      config = null;
      loadedId = null;
      return;
    }
    if (id === loadedId) return;
    confirmDelete = false;
    api.getEngagement(id).then((loaded) => {
      autosave.mark(`engagement:${id}`, loaded);
      loadedId = id;
      config = loaded;
    });
  });

  $effect(() => {
    if (!config || !loadedId) return;
    const id = loadedId;
    const value = $state.snapshot(config);
    autosave.track(`engagement:${id}`, value, async () => {
      await api.saveEngagement(id, value);
      onchanged();
    });
  });

  function counts(c: Config) {
    return [
      [c.hosts.length, "hosts"],
      [c.users.length, "users"],
      [c.terms.length, "terms"],
      [c.sensitive_headers.length + c.sensitive_keys.length, "headers/keys"],
    ] as const;
  }

  async function remove() {
    if (!editing) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    await api.deleteEngagement(editing);
    editing = null;
    onchanged();
  }
</script>

{#if editing && config}
  <div class="head">
    <div>
      <button class="link" onclick={() => (editing = null)}>← All engagements</button>
      <input
        class="title"
        bind:value={
          () => config?.name ?? "",
          (v) => config && (config.name = v || null)
        }
        placeholder="Engagement name"
        aria-label="Engagement name"
      />
      <p class="muted id">
        <code>engagements/{editing}.toml</code>
        {#if settings.active_engagement === editing}
          · <span class="chip accent">active</span>
        {:else}
          · <button class="link" onclick={() => onactivate(editing)}>Make active</button>
        {/if}
      </p>
    </div>
    <button class="btn danger" onclick={remove}>{confirmDelete ? "Click again to delete" : "Delete"}</button>
  </div>
  <div class="editor">
    <div>
      <p class="muted lead">
        Rules added here apply only while this engagement is active, on top of the global rules.
      </p>
      <RulesEditor bind:config scope="engagement" />
    </div>
    <EffectiveRules {global} engagement={config} />
  </div>
{:else}
  <div class="head">
    <div>
      <h2>Engagements</h2>
      <p class="muted">
        One profile per test: hosts, users and terms that change between engagements. The active one is
        layered on top of the global rules.
      </p>
    </div>
    <button class="btn primary" onclick={() => (creating = !creating)}>+ New engagement</button>
  </div>

  {#if creating || engagements.length === 0}
    <div class="card create">
      <h3>New engagement</h3>
      <NewEngagement
        oncreated={(id, activate) => {
          creating = false;
          oncreated(id, activate);
          editing = id;
        }}
      />
    </div>
  {/if}

  <div class="cards">
    <article class="card" class:active={settings.active_engagement === null}>
      <h3>Global only <span class="muted small">no engagement</span></h3>
      <p class="muted small">Only the global rules apply.</p>
      <div class="foot">
        {#if settings.active_engagement === null}
          <span class="chip accent">active</span>
        {:else}
          <button class="btn small" onclick={() => onactivate(null)}>Activate</button>
        {/if}
      </div>
    </article>
    {#each engagements as e (e.id)}
      <article class="card" class:active={settings.active_engagement === e.id}>
        <h3>{e.name}</h3>
        <p class="muted small"><code>{e.id}</code></p>
        <div class="chips">
          {#each counts(e.config) as [n, label] (label)}
            <span class="chip">{n} {label}</span>
          {/each}
        </div>
        <div class="foot">
          {#if settings.active_engagement === e.id}
            <span class="chip accent">active</span>
          {:else}
            <button class="btn small" onclick={() => onactivate(e.id)}>Activate</button>
          {/if}
          <button class="btn small" onclick={() => (editing = e.id)}>Edit</button>
        </div>
      </article>
    {/each}
  </div>
{/if}

<style>
  h2 {
    margin: 0;
    font-size: 20px;
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }
  .head p {
    margin: 4px 0 0;
    max-width: 620px;
  }
  .title {
    display: block;
    width: 100%;
    margin-top: 8px;
    padding: 2px 0;
    border: 0;
    border-bottom: 1px dashed transparent;
    background: none;
    font-size: 20px;
    font-weight: 650;
  }
  .title:hover,
  .title:focus {
    border-bottom-color: var(--border);
    outline: none;
  }
  .id {
    font-size: 12px;
  }
  .editor {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 24px;
    align-items: start;
  }
  .lead {
    margin: 0 0 16px;
  }
  .create {
    margin-bottom: 16px;
    max-width: 560px;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 12px;
  }
  .cards .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .cards .card h3 {
    margin: 0;
  }
  .cards .card.active {
    border-color: var(--accent);
  }
  .foot {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-top: auto;
    padding-top: 6px;
  }
  .small {
    font-size: 12px;
    margin: 0;
  }
</style>
