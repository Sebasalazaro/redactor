<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { api, type AppSettings, type Config, type Engagement, type Status } from "../lib/api";
  import { Autosave, COMMIT_DRAFTS } from "../lib/autosave.svelte";
  import Icon, { type IconName } from "../lib/Icon.svelte";
  import Home from "./pages/Home.svelte";
  import Settings from "./pages/Settings.svelte";
  import Terms from "./pages/Terms.svelte";
  import Tools from "./pages/Tools.svelte";

  type Section = "home" | "terms" | "tools" | "settings";

  let section = $state<Section>("home");
  let status = $state<Status | null>(null);
  let settings = $state<AppSettings | null>(null);
  let global = $state<Config | null>(null);
  let engagements = $state<Engagement[]>([]);

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
    await refreshEngagements();
    if (makeActive) await activate(id);
  }

  /** Saves everything still pending. */
  async function flush() {
    // Commit half-typed values, let effects schedule their saves, run them.
    window.dispatchEvent(new Event(COMMIT_DRAFTS));
    await tick();
    await autosave.flush();
  }

  /** Saves everything still pending, then lets the window close. */
  async function closeSafely() {
    await flush();
    await api.closeWindow();
  }

  async function quit() {
    await flush();
    await api.quit();
  }

  // The first time the window is closed, say that the app keeps running.
  let closeNotice = $state(false);
  const isMac = navigator.platform.toLowerCase().includes("mac");

  async function dismissNotice(andQuit: boolean) {
    closeNotice = false;
    if (settings) {
      settings.close_notice_shown = true;
      const value = $state.snapshot(settings);
      await autosave.now("settings", value, () => api.saveSettings(value));
    }
    await (andQuit ? quit() : closeSafely());
  }

  onMount(() => {
    loadAll();
    const unlisteners = [
      listen("settings-changed", async () => {
        const fresh = await api.getSettings();
        autosave.mark("settings", fresh);
        settings = fresh;
      }),
      listen("close-requested", closeSafely),
      listen("close-notice", () => (closeNotice = true)),
    ];
    return () => {
      unlisteners.forEach((u) => u.then((f) => f()));
    };
  });

  const NAV: { id: Section; label: string; icon: IconName }[] = [
    { id: "home", label: "Home", icon: "home" },
    { id: "terms", label: "Terms", icon: "terms" },
    { id: "tools", label: "Tools", icon: "tools" },
    { id: "settings", label: "Settings", icon: "settings" },
  ];
</script>

<div class="layout">
  <nav>
    <div class="brand">
      <span class="logo" aria-hidden="true"><i></i><i></i><i></i></span>
      redactor
    </div>

    {#each NAV as item (item.id)}
      <button
        class="nav"
        class:active={section === item.id}
        aria-current={section === item.id ? "page" : undefined}
        onclick={() => (section = item.id)}
      >
        <Icon name={item.icon} />
        {item.label}
      </button>
    {/each}

    <div class="footer">
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
      {#if status}<div class="version">redactor {status.version}</div>{/if}
    </div>
  </nav>

  <main class:fit={section === "home"}>
    {#if status?.error}
      <div class="banner">Configuration error: {status.error}</div>
    {/if}

    {#if settings && global}
      {#if section === "home"}
        <Home
          {settings}
          {global}
          {engagements}
          onterms={() => (section = "terms")}
          onsettings={() => (section = "settings")}
        />
      {:else if section === "terms"}
        <Terms
          {settings}
          bind:global
          {engagements}
          {autosave}
          onactivate={activate}
          oncreated={created}
          onchanged={refreshEngagements}
        />
      {:else if section === "tools"}
        <Tools {settings} profileName={active?.name ?? "all engagements"} />
      {:else if section === "settings"}
        <Settings onquit={quit} bind:settings {status} />
      {/if}
    {/if}
  </main>
</div>

{#if closeNotice}
  <div class="overlay">
    <div class="dialog card" role="alertdialog" aria-modal="true" aria-labelledby="notice-title">
      <h3 id="notice-title">redactor keeps running</h3>
      <p>
        Closing this window leaves redactor in the {isMac ? "menu bar" : "system tray"}, so the hotkey
        keeps working. To stop it, use <strong>Quit redactor</strong> there or in Settings.
      </p>
      <div class="dialog-actions">
        <button class="btn" onclick={() => dismissNotice(true)}>Quit redactor</button>
        <button class="btn primary" onclick={() => dismissNotice(false)}>Keep running</button>
      </div>
    </div>
  </div>
{/if}

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
    padding: 16px 8px;
    border-right: 1px solid var(--border);
    background: var(--surface);
    overflow: auto;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 10px 16px;
    font-weight: 600;
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
  .nav {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--muted);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .nav:hover {
    color: var(--text);
  }
  .nav:hover {
    background: var(--surface-2);
  }
  .nav.active {
    background: var(--surface-2);
    color: var(--text);
    font-weight: 500;
  }
  .nav.active :global(.icon) {
    color: var(--accent);
  }
  .footer {
    margin-top: auto;
    padding: 10px;
    display: grid;
    gap: 4px;
    font-size: 12px;
  }
  .save-state {
    color: var(--muted);
  }
  .version {
    color: var(--muted);
    font-size: 11px;
  }
  .save-state.bad {
    color: var(--danger);
  }
  main {
    overflow: auto;
    padding: 24px 32px 32px;
  }
  main.fit {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .overlay {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: rgb(0 0 0 / 0.45);
    z-index: 10;
  }
  .dialog {
    max-width: 420px;
    padding: 20px;
  }
  .dialog h3 {
    margin: 0 0 8px;
    font-size: 16px;
  }
  .dialog p {
    margin: 0 0 16px;
    color: var(--muted);
    font-size: 13px;
    line-height: 1.5;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .banner {
    margin-bottom: 16px;
    padding: 10px 14px;
    border-radius: var(--radius);
    background: var(--danger-soft);
    color: var(--danger);
  }
</style>
