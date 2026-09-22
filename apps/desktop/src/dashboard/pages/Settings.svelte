<script lang="ts">
  import type { AppSettings, Status } from "../../lib/api";
  import HotkeyField from "../components/HotkeyField.svelte";

  let { settings = $bindable(), status }: { settings: AppSettings; status: Status | null } = $props();

  const FORGET_OPTIONS = [
    [5, "5 minutes"],
    [15, "15 minutes"],
    [60, "1 hour"],
    [0, "Never (until quit)"],
  ] as const;
</script>

<h2>Settings</h2>

<h4>Hotkey</h4>
<div class="rows">
  <div class="row">
    <div>
      <strong>Shortcut</strong>
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
</div>

<h4>Privacy</h4>
<div class="rows">
  <div class="row">
    <div>
      <strong>Keep the last redaction</strong>
      <p>Lets you copy it again from the overview. Only the redacted text, only in memory.</p>
    </div>
    <input type="checkbox" class="toggle" bind:checked={settings.keep_last} />
  </div>
  <div class="row" class:disabled={!settings.keep_last}>
    <div>
      <strong>Forget it after</strong>
      <p>Dropped automatically, even if the app stays open.</p>
    </div>
    <select class="text" bind:value={settings.forget_last_after_minutes} disabled={!settings.keep_last}>
      {#each FORGET_OPTIONS as [value, label] (value)}<option {value}>{label}</option>{/each}
    </select>
  </div>
  <div class="row">
    <div>
      <strong>Clear the clipboard when a review is cancelled</strong>
      <p>Otherwise the original, unredacted text stays on the clipboard.</p>
    </div>
    <input type="checkbox" class="toggle" bind:checked={settings.clear_clipboard_on_cancel} />
  </div>
</div>

<h4>Memory</h4>
<div class="rows">
  <div class="row">
    <div>
      <strong>Unload windows when closed</strong>
      <p>
        Frees each window's web view process (~30–40 MB). Reopening takes a fraction of a second longer.
      </p>
    </div>
    <input type="checkbox" class="toggle" bind:checked={settings.unload_windows} />
  </div>
</div>

<h4>AI deep scan</h4>
<div class="rows">
  <div class="row">
    <div>
      <strong>Local model</strong>
      <p>
        GLiNER PII (knowledgator/gliner-pii-edge-v1.0), runs on this Mac only when you press
        <em>Deep scan</em> in the review popup, in a separate process that ends after two idle
        minutes (~190 MB while it runs). English-focused.
      </p>
    </div>
    {#if status?.ai_installed}
      <span class="installed">Installed</span>
    {:else}
      <code class="path">scripts/fetch-model.sh</code>
    {/if}
  </div>
</div>

<h4>Storage</h4>
<div class="rows">
  <div class="row">
    <div>
      <strong>Config folder</strong>
      <p>Shared with the <code>redactor</code> CLI. Files are readable only by you.</p>
    </div>
    <code class="path">{status?.config_dir}</code>
  </div>
</div>

<style>
  h2 {
    margin: 0 0 8px;
    font-size: 20px;
  }
  h4 {
    margin: 22px 0 8px;
    color: var(--muted);
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .rows {
    max-width: 820px;
    border: 1px solid var(--border);
    border-radius: 10px;
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
    color: var(--muted);
    font-size: 12px;
  }
  .row.disabled {
    opacity: 0.5;
  }
  .toggle {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }
  .installed {
    color: var(--ok);
    font-weight: 600;
  }
  .path {
    font-size: 12px;
    color: var(--muted);
  }
</style>
