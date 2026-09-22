<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { api, type AppSettings, type Config, type Engagement, type Status } from "../lib/api";
  import { Autosave } from "../lib/autosave.svelte";
  import NewEngagement from "./components/NewEngagement.svelte";
  import Playground from "./components/Playground.svelte";
  import Engagements from "./pages/Engagements.svelte";
  import GlobalRules from "./pages/GlobalRules.svelte";
  import Overview from "./pages/Overview.svelte";
  import Settings from "./pages/Settings.svelte";

  type Section = "overview" | "engagements" | "global" | "playground" | "settings";

  let section = $state<Section>("overview");
  let status = $state<Status | null>(null);
  let settings = $state<AppSettings | null>(null);
  let global = $state<Config | null>(null);
  let engagements = $state<Engagement[]>([]);
  let editing = $state<string | null>(null);
  let quickCreate = $state(false);

  const autosave = new Autosave();
  const active = $derived(engagements.find((e) => e.id === settings?.active_engagement) ?? null);

  $effect(() => {
    if (!settings) return;
    const value = $state.snapshot(settings);
    autosave.track("settings", value, () => api.saveSettings(value));
  });

  $effect(() => {
    if (!global) return;
    const value = $state.snapshot(global);
    autosave.track("global", value, async () => {
      await api.saveGlobal(value);
      status = await api.getStatus();
    });
  });

  async function loadAll() {
    const [s, st, g, e] = await Promise.all([
      api.getStatus(),
      api.getSettings(),
      api.getGlobal(),
      api.listEngagements(),
    ]);
    autosave.mark("settings", st);
    autosave.mark("global", g);
    [status, settings, global, engagements] = [s, st, g, e];
  }

  async function refreshEngagements() {
    engagements = await api.listEngagements();
  }

  /** Switching profile is saved right away, not debounced. */
  async function activate(id: string | null) {
    if (!settings) return;
    settings.active_engagement = id;
    const value = $state.snapshot(settings);
    await autosave.now("settings", value, () => api.saveSettings(value));
  }

  async function created(id: string, makeActive: boolean) {
    quickCreate = false;
    await refreshEngagements();
    if (makeActive) await activate(id);
    editing = id;
    section = "engagements";
  }

  function navigate(to: Section, engagement?: string) {
    section = to;
    if (to === "engagements") editing = engagement ?? null;
  }

  onMount(() => {
    loadAll();
    const unlisten = listen("settings-changed", async () => {
      const fresh = await api.getSettings();
      autosave.mark("settings", fresh);
      settings = fresh;
    });
    return () => {
      unlisten.then((f) => f());
    };
  });

  const NAV: { id: Section; label: string }[] = [
    { id: "overview", label: "Overview" },
    { id: "engagements", label: "Engagements" },
    { id: "global", label: "Global rules" },
    { id: "playground", label: "Playground" },
    { id: "settings", label: "Settings" },
  ];
</script>

<div class="layout">
  <nav>
    <div class="brand">
      <span class="logo" aria-hidden="true"><i></i><i></i><i></i></span>
      redactor
    </div>

    {#if settings}
      <div class="profile">
        <label for="profile">Active profile</label>
        <select
          id="profile"
          class="text"
          value={settings.active_engagement ?? ""}
          onchange={(e) => activate(e.currentTarget.value || null)}
        >
          <option value="">Global only</option>
          {#each engagements as e (e.id)}<option value={e.id}>{e.name}</option>{/each}
        </select>
        <button class="link" onclick={() => (quickCreate = !quickCreate)}>
          {quickCreate ? "Cancel" : "+ New engagement"}
        </button>
        {#if quickCreate}
          <NewEngagement compact oncreated={(id) => created(id, true)} />
        {/if}
      </div>
    {/if}

    {#each NAV as item (item.id)}
      <button class="nav" class:active={section === item.id} onclick={() => navigate(item.id)}>
        {item.label}
        {#if item.id === "engagements" && engagements.length}
          <span class="count">{engagements.length}</span>
        {/if}
      </button>
    {/each}

    <div class="save-state" class:bad={autosave.state === "error"} aria-live="polite">
      {#if autosave.state === "pending" || autosave.state === "saving"}
        Saving…
      {:else if autosave.state === "saved"}
        ✓ All changes saved
      {:else if autosave.state === "error"}
        Couldn't save: {autosave.error}
      {:else}
        Changes save automatically
      {/if}
    </div>
  </nav>

  <main>
    {#if status?.error}
      <div class="banner">Configuration error: {status.error}</div>
    {/if}

    {#if settings && global}
      {#if section === "overview"}
        <Overview
          {settings}
          {global}
          {engagements}
          onnavigate={navigate}
          onactivate={activate}
          oncreated={created}
        />
      {:else if section === "engagements"}
        <Engagements
          {settings}
          {global}
          {engagements}
          bind:editing
          {autosave}
          onactivate={activate}
          oncreated={created}
          onchanged={refreshEngagements}
        />
      {:else if section === "global"}
        <GlobalRules bind:global {engagements} />
      {:else if section === "playground"}
        <h2>Playground</h2>
        <p class="muted">Profile: {active?.name ?? "global only"}</p>
        <Playground />
      {:else if section === "settings"}
        <Settings bind:settings {status} />
      {/if}
    {/if}
  </main>
</div>

<style>
  .layout {
    display: grid;
    grid-template-columns: 220px 1fr;
    height: 100%;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 10px;
    border-right: 1px solid var(--border);
    background: var(--surface);
    overflow: auto;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 8px 14px;
    font-weight: 700;
    font-size: 15px;
  }
  .logo {
    display: grid;
    gap: 3px;
  }
  .logo i {
    display: block;
    height: 4px;
    border-radius: 2px;
    background: var(--accent);
  }
  .logo i:nth-child(1) {
    width: 18px;
  }
  .logo i:nth-child(2) {
    width: 12px;
  }
  .logo i:nth-child(3) {
    width: 15px;
  }
  .profile {
    display: grid;
    gap: 6px;
    margin: 0 4px 14px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
  }
  .profile label {
    color: var(--muted);
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .profile .link {
    justify-self: start;
    font-size: 12px;
  }
  .nav {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 7px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    cursor: pointer;
  }
  .nav:hover {
    background: var(--surface-2);
  }
  .nav.active {
    background: var(--accent-soft);
    color: var(--accent-strong);
    font-weight: 600;
  }
  .count {
    padding: 0 7px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--muted);
    font-size: 11px;
    font-weight: 500;
  }
  .save-state {
    margin-top: auto;
    padding: 10px;
    color: var(--muted);
    font-size: 12px;
  }
  .save-state.bad {
    color: var(--danger);
  }
  main {
    overflow: auto;
    padding: 24px 32px 40px;
  }
  main > h2 {
    margin: 0 0 4px;
    font-size: 20px;
  }
  .banner {
    margin-bottom: 16px;
    padding: 10px 14px;
    border-radius: var(--radius);
    background: var(--danger-soft);
    color: var(--danger);
  }
</style>
