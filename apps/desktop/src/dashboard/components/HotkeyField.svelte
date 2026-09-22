<script lang="ts">
  // Records a key combination and writes it in Tauri accelerator syntax.
  let { hotkey = $bindable() }: { hotkey: string } = $props();

  import { hotkey as pretty } from "../../lib/format";

  let recording = $state(false);
  const isMac = navigator.platform.toLowerCase().includes("mac");

  function onKeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      recording = false;
      return;
    }
    if (["Meta", "Control", "Alt", "Shift"].includes(e.key)) return;
    const mods = [
      (isMac ? e.metaKey : e.ctrlKey) && "CommandOrControl",
      isMac && e.ctrlKey && "Control",
      e.altKey && "Alt",
      e.shiftKey && "Shift",
    ].filter(Boolean);
    if (mods.length === 0) return;
    const key = e.code.replace(/^Key/, "").replace(/^Digit/, "");
    hotkey = [...mods, key].join("+");
    recording = false;
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="hotkey">
  <span class="value" class:recording>{recording ? "Press a combination…" : pretty(hotkey)}</span>
  <button class="btn" onclick={() => (recording = !recording)}>
    {recording ? "Cancel" : "Record"}
  </button>
</div>

<style>
  .hotkey {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .value {
    min-width: 180px;
    padding: 5px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    font: 13px var(--mono);
  }
  .recording {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
