<script lang="ts">
  // Rules beyond plain words, for the global config or an engagement.
  import { DEFAULT_MASKING, type Config } from "../../lib/api";
  import ListField from "./ListField.svelte";
  import MaskingField from "./MaskingField.svelte";
  
  let { config = $bindable(), scope }: { config: Config; scope: "global" | "engagement" } = $props();

  function toggleMasking(on: boolean) {
    config.masking = on ? { ...DEFAULT_MASKING } : null;
  }
</script>

<ListField
  label="Sensitive headers"
  help="Extra headers whose values are secrets (Authorization, Cookie and ~30 others are built in)."
  placeholder="X-Device-Token"
  bind:values={config.sensitive_headers}
/>
<ListField
  label="Sensitive keys"
  help="Extra JSON, query and form keys whose values are secrets."
  placeholder="deviceFingerprint"
  bind:values={config.sensitive_keys}
/>
<ListField
  label="Allowed domains"
  help="Public domains left untouched (unless they contain the client's name)."
  placeholder="cdn.jsdelivr.net"
  bind:values={config.allow_domains}
/>

<div class="masking">
  <label class="switch">
    <input
      type="checkbox"
      checked={config.masking !== null}
      onchange={(e) => toggleMasking(e.currentTarget.checked)}
    />
    <strong>{scope === "global" ? "Custom masking ratios" : "Override masking ratios"}</strong>
  </label>
  {#if config.masking}
    <MaskingField bind:masking={config.masking} />
  {/if}
</div>

<style>
  .masking {
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  .switch {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 8px 0 14px;
  }
</style>
