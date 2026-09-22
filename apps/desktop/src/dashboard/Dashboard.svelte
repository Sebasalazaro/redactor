<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { api, normalizeConfig, type AppSettings, type Config, type Status } from "../lib/api";
  import HotkeyField from "./components/HotkeyField.svelte";
  import Playground from "./components/Playground.svelte";
  import RulesEditor from "./components/RulesEditor.svelte";

  type Section = "general" | "global" | "engagements" | "playground";
  const SECTIONS: { id: Section; label: string }[] = [
    { id: "general", label: "General" },
    { id: "global", label: "Global rules" },
    { id: "engagements", label: "Engagements" },
    { id: "playground", label: "Playground" },
  ];

  let section = $state<Section>("general");
  let status = $state<Status | null>(null);
  let settings = $state<AppSettings | null>(null);
  let global = $state<Config | null>(null);
  let engagements = $state<string[]>([]);
  let selected = $state<string | null>(null);
  let engagement = $state<Config | null>(null);
  let newName = $state("");
  let confirmDelete = $state(false);
  let toast = $state<{ text: string; ok: boolean } | null>(null);

  function notify(text: string, ok = true) {
    toast = { text, ok };
    setTimeout(() => (toast = null), 3000);
  }

  async function run(action: () => Promise<unknown>, done: string) {
    try {
      await action();
      notify(done);
      status = await api.getStatus();
    } catch (e) {
      notify(String(e), false);
    }
  }

  async function loadAll() {
    [status, settings, global, engagements] = await Promise.all([
      api.getStatus(),
      api.getSettings(),
      api.getGlobal(),
      api.listEngagements(),
    ]);
    if (selected && engagements.includes(selected)) await select(selected);
  }

  async function select(name: string) {
    selected = name;
    confirmDelete = false;
    engagement = await api.getEngagement(name);
  }

  async function create() {
    const name = newName.trim();
    if (!name) return;
    await run(async () => {
      await api.saveEngagement(name, normalizeConfig({ name }));
      engagements = await api.listEngagements();
      newName = "";
      await select(name);
    }, `Created ${name}`);
  }

  async function remove() {
    if (!selected) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    const name = selected;
    await run(async () => {
      await api.deleteEngagement(name);
      selected = null;
      engagement = null;
      await loadAll();
    }, `Deleted ${name}`);
  }

  onMount(() => {
    loadAll();
    const unlisten = listen("settings-changed", async () => {
      settings = await api.getSettings();
    });
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<div class="layout">
  <nav>
    <div class="brand">
      <span class="logo" aria-hidden="true"><i></i><i></i><i></i></span>
      redactor
    </div>
    {#each SECTIONS as s (s.id)}
      <button class:active={section === s.id} onclick={() => (section = s.id)}>{s.label}</button>
    {/each}
    {#if settings}
      <div class="profile">
        Active profile
        <strong>{settings.active_engagement ?? "global"}</strong>
      </div>
    {/if}
  </nav>

  <main>
    {#if status?.error}
      <div class="banner">Configuration error: {status.error}</div>
    {/if}

    {#if section === "general" && settings}
      <h2>General</h2>
      <div class="rows">
        <div class="row">
          <div>
            <strong>Hotkey</strong>
            <p>Redacts the clipboard from any app.</p>
          </div>
          <HotkeyField bind:hotkey={settings.hotkey} />
        </div>
        <div class="row">
          <div>
            <strong>Review before copying</strong>
            <p>Show the popup with the diff. When off, the clipboard is replaced right away.</p>
          </div>
          <input type="checkbox" class="toggle" bind:checked={settings.review} />
        </div>
        <div class="row">
          <div>
            <strong>Active engagement</strong>
            <p>Layered on top of the global rules. Also switchable from the menu bar.</p>
          </div>
          <select
            value={settings.active_engagement ?? ""}
            onchange={(e) => settings && (settings.active_engagement = e.currentTarget.value || null)}
          >
            <option value="">None (global only)</option>
            {#each engagements as name (name)}<option value={name}>{name}</option>{/each}
          </select>
        </div>
        <div class="row">
          <div>
            <strong>Config folder</strong>
            <p>Shared with the <code>redactor</code> CLI. Files are readable only by you.</p>
          </div>
          <code class="path">{status?.config_dir}</code>
        </div>
      </div>
      <div class="save">
        <button class="btn primary" onclick={() => settings && run(() => api.saveSettings(settings!), "Settings saved")}
          >Save</button
        >
      </div>
    {:else if section === "global" && global}
      <h2>Global rules</h2>
      <p class="lead">Apply to every engagement. Put the client's name here.</p>
      <RulesEditor bind:config={global} scope="global" />
      <div class="save">
        <button class="btn primary" onclick={() => global && run(() => api.saveGlobal(global!), "Global rules saved")}
          >Save</button
        >
      </div>
    {:else if section === "engagements"}
      <h2>Engagements</h2>
      <div class="split">
        <aside>
          {#each engagements as name (name)}
            <button class:active={selected === name} onclick={() => select(name)}>
              {name}
              {#if settings?.active_engagement === name}<span class="dot" title="Active"></span>{/if}
            </button>
          {:else}
            <p class="lead">No engagements yet.</p>
          {/each}
          <form
            onsubmit={(e) => {
              e.preventDefault();
              create();
            }}
          >
            <input bind:value={newName} placeholder="new-engagement" aria-label="New engagement name" />
            <button class="btn" type="submit">Add</button>
          </form>
        </aside>
        <section>
          {#if engagement && selected}
            <div class="engagement-head">
              <h3>{selected}</h3>
              <button class="btn danger" onclick={remove}>
                {confirmDelete ? "Click again to delete" : "Delete"}
              </button>
            </div>
            <RulesEditor bind:config={engagement} scope="engagement" />
            <div class="save">
              <button
                class="btn primary"
                onclick={() =>
                  engagement && selected && run(() => api.saveEngagement(selected!, engagement!), `Saved ${selected}`)}
                >Save</button
              >
            </div>
          {:else}
            <p class="lead">Pick an engagement or create one. Names become file names.</p>
          {/if}
        </section>
      </div>
    {:else if section === "playground"}
      <h2>Playground</h2>
      <Playground />
    {/if}
  </main>

  {#if toast}
    <div class="toast" class:bad={!toast.ok} role="status">{toast.text}</div>
  {/if}
</div>

<style>
  .layout {
    display: grid;
    grid-template-columns: 200px 1fr;
    height: 100%;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 10px;
    border-right: 1px solid var(--border);
    background: var(--surface);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 8px 16px;
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
  nav > button,
  aside > button {
    text-align: left;
    padding: 7px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    cursor: pointer;
  }
  nav > button:hover,
  aside > button:hover {
    background: var(--surface-2);
  }
  nav > button.active,
  aside > button.active {
    background: var(--accent-soft);
    color: var(--accent-strong);
    font-weight: 600;
  }
  .profile {
    margin-top: auto;
    padding: 10px;
    display: grid;
    color: var(--muted);
    font-size: 12px;
  }
  .profile strong {
    color: var(--text);
    font: 12px var(--mono);
  }
  main {
    overflow: auto;
    padding: 24px 32px 40px;
  }
  h2 {
    margin: 0 0 16px;
    font-size: 20px;
  }
  h3 {
    margin: 0;
  }
  .lead,
  .row p {
    color: var(--muted);
    margin: 2px 0 16px;
  }
  .rows {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 16px;
  }
  .row + .row {
    border-top: 1px solid var(--border);
  }
  .row p {
    margin: 2px 0 0;
    font-size: 12px;
  }
  .toggle {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }
  select,
  form input {
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
  }
  .path {
    font-size: 12px;
    color: var(--muted);
  }
  .save {
    display: flex;
    justify-content: flex-end;
    margin-top: 16px;
  }
  .split {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 24px;
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  aside form {
    display: flex;
    gap: 6px;
    margin-top: 12px;
  }
  aside form input {
    min-width: 0;
    flex: 1;
    font-family: var(--mono);
    font-size: 12px;
  }
  .dot {
    display: inline-block;
    width: 7px;
    height: 7px;
    margin-left: 6px;
    border-radius: 50%;
    background: var(--ok);
  }
  .engagement-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
  }
  .banner {
    margin-bottom: 16px;
    padding: 10px 14px;
    border-radius: var(--radius);
    background: var(--danger-soft);
    color: var(--danger);
  }
  .toast {
    position: fixed;
    right: 20px;
    bottom: 20px;
    padding: 10px 14px;
    border-radius: var(--radius);
    background: var(--text);
    color: var(--bg);
    box-shadow: 0 6px 24px rgb(0 0 0 / 0.2);
  }
  .toast.bad {
    background: var(--danger);
    color: #fff;
  }
</style>
