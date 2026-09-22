<script lang="ts">
  import type { CustomTerm } from "../../lib/api";

  let { terms = $bindable() }: { terms: CustomTerm[] } = $props();
</script>

<div class="field">
  <span class="label">Custom terms</span>
  <p class="help">
    Literal values replaced everywhere: device names, codenames, your own name. Empty replacement
    means <code>[REDACTED]</code>.
  </p>
  {#each terms as term, i (i)}
    <div class="row">
      <input aria-label="Term" placeholder="MacBook-de-Pentester" bind:value={term.value} />
      <span class="arrow">→</span>
      <input
        aria-label="Replacement"
        placeholder="[REDACTED]"
        value={term.replacement ?? ""}
        oninput={(e) => (term.replacement = e.currentTarget.value || null)}
      />
      <button class="btn" aria-label="Remove term" onclick={() => terms.splice(i, 1)}>×</button>
    </div>
  {/each}
  <button class="btn" onclick={() => terms.push({ value: "", replacement: null })}>+ Add term</button>
</div>

<style>
  .field {
    margin-bottom: 18px;
  }
  .label {
    font-weight: 600;
  }
  .help {
    margin: 2px 0 6px;
    color: var(--muted);
    font-size: 12px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr auto 1fr auto;
    gap: 8px;
    align-items: center;
    margin-bottom: 6px;
  }
  input {
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    font-family: var(--mono);
    font-size: 12px;
  }
  .arrow {
    color: var(--muted);
  }
</style>
