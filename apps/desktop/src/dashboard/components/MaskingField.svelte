<script lang="ts">
  import type { MaskingConfig } from "../../lib/api";

  let { masking = $bindable() }: { masking: MaskingConfig } = $props();

  const sliders: { key: keyof MaskingConfig; label: string; help: string }[] = [
    { key: "id_cut", label: "Identifiers hidden", help: "834512 → 8345** at 20 %" },
    { key: "secret_keep", label: "Secret prefix shown", help: "Cookies, tokens, keys (max 12 chars)" },
    { key: "pii_keep", label: "Personal data shown", help: "Juan Pérez → Ju** Pé*** at 40 %" },
  ];
</script>

<div class="grid">
  {#each sliders as s (s.key)}
    <label>
      <span class="top"><strong>{s.label}</strong><span>{Math.round(masking[s.key] * 100)} %</span></span>
      <input type="range" min="0" max="1" step="0.05" bind:value={masking[s.key]} />
      <span class="help">{s.help}</span>
    </label>
  {/each}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
    margin-bottom: 18px;
  }
  label {
    display: grid;
    gap: 4px;
  }
  .top {
    display: flex;
    justify-content: space-between;
  }
  input {
    accent-color: var(--accent);
  }
  .help {
    color: var(--muted);
    font-size: 12px;
  }
</style>
