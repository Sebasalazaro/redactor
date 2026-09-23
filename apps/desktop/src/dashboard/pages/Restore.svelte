<script lang="ts">
  // Paste an LLM answer; redacted values the vault knows come back as the
  // real ones. Ambiguous values are left as is until you pick a candidate.
  import { onMount } from "svelte";
  import { api, type AppSettings, type RestorePart } from "../../lib/api";

  let { settings, profileName }: { settings: AppSettings; profileName: string } = $props();

  let input = $state("");
  let parts = $state<RestorePart[]>([]);
  let choices = $state<Record<number, string>>({});
  let entries = $state<number | null>(null);
  let error = $state("");
  let notice = $state("");
  let timer: ReturnType<typeof setTimeout>;

  const restored = $derived(parts.filter((p) => p.kind === "restored").length);
  const ambiguous = $derived(parts.filter((p) => p.kind === "ambiguous").length);
  const output = $derived(
    parts
      .map((p, i) =>
        p.kind === "plain" ? p.text : p.kind === "restored" ? p.original : (choices[i] ?? p.replacement),
      )
      .join(""),
  );

  $effect(() => {
    const text = input;
    clearTimeout(timer);
    timer = setTimeout(async () => {
      try {
        parts = text ? (await api.restoreText(text)).parts : [];
        choices = {};
        error = "";
      } catch (e) {
        error = String(e);
      }
    }, 200);
  });

  function flash(text: string) {
    notice = text;
    setTimeout(() => (notice = ""), 2500);
  }

  async function copy() {
    await api.copyText(output);
    flash("Copied");
  }

  async function restoreClipboard() {
    try {
      const [done, unsure] = await api.restoreClipboard();
      flash(`Clipboard restored: ${done} values${unsure ? `, ${unsure} ambiguous left as is` : ""}`);
    } catch (e) {
      error = String(e);
    }
  }

  async function forgetAll() {
    await api.forgetVault();
    entries = 0;
    parts = input ? (await api.restoreText(input)).parts : [];
    flash("Vault cleared for this profile");
  }

  onMount(async () => {
    try {
      entries = (await api.vaultInfo()).entries;
    } catch (e) {
      error = String(e);
    }
  });
</script>

<header>
  <div>
    <h2>Restore</h2>
    <p class="muted">
      Paste an answer from your LLM: redacted values become the real ones again, from the encrypted
      vault of <strong>{profileName}</strong>{entries !== null ? ` (${entries} values remembered)` : ""}.
    </p>
  </div>
  <button class="btn primary" onclick={restoreClipboard} disabled={!settings.remember}>
    Restore clipboard
  </button>
</header>

{#if !settings.remember}
  <p class="warn">Remembering is off in Settings, so nothing new is added to the vault.</p>
{/if}
{#if error}<p class="error">{error}</p>{/if}

<div class="panes">
  <label>
    <span>LLM answer</span>
    <textarea bind:value={input} spellcheck="false" placeholder="curl -b 'sid=a7f3e9c1d…[len=32]' https://api.[CLIENT].example/…"></textarea>
  </label>
  <div>
    <span>
      Restored · {restored} values{ambiguous ? ` · ${ambiguous} ambiguous (click to choose)` : ""}
    </span>
    <pre>{#each parts as p, i (i)}{#if p.kind === "plain"}{p.text}{:else if p.kind === "restored"}<mark
            class="restored"
            title="was {p.replacement}">{p.original}</mark
          >{:else}<span class="ambiguous" title="{p.replacement} could be: {p.candidates.join(', ')}"
            >{choices[i] ?? p.replacement}<span class="options"
              >{#each p.candidates as c (c)}<button
                  class:chosen={choices[i] === c}
                  onclick={() => (choices[i] = c)}>{c}</button
                >{/each}</span
            ></span
          >{/if}{/each}</pre>
  </div>
</div>

<footer>
  <button class="btn danger small" onclick={forgetAll}>Forget this profile's vault</button>
  {#if notice}<span class="notice">{notice}</span>{/if}
  <button class="btn primary" onclick={copy} disabled={!output}>Copy restored</button>
</footer>

<style>
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 20px;
  }
  header p {
    margin: 0 0 14px;
    max-width: 640px;
  }
  .warn {
    color: var(--cat-secret);
  }
  .error {
    color: var(--danger);
  }
  .panes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    height: calc(100vh - 230px);
  }
  label,
  .panes > div {
    display: grid;
    grid-template-rows: auto 1fr;
    gap: 6px;
    min-height: 0;
  }
  span {
    color: var(--muted);
    font-size: 12px;
  }
  textarea,
  pre {
    margin: 0;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    font: 12px/1.6 var(--mono);
    overflow: auto;
    resize: none;
    white-space: pre-wrap;
    word-break: break-all;
  }
  mark.restored {
    background: var(--accent-soft);
    color: var(--accent-strong);
    border-radius: 3px;
  }
  .ambiguous {
    color: var(--danger);
    font: inherit;
    border-bottom: 1px dashed var(--danger);
  }
  .options {
    display: inline-flex;
    gap: 2px;
    margin-left: 4px;
    vertical-align: middle;
  }
  .options button {
    padding: 0 5px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--surface-2);
    font: 11px var(--mono);
    cursor: pointer;
  }
  .options button.chosen {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 12px;
  }
  .notice {
    color: var(--ok);
  }
</style>
