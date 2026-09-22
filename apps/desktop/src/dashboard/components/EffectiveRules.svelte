<script lang="ts">
  // What actually applies when this engagement is active: global rules plus
  // the engagement's own, with the origin of each value.
  import { DEFAULT_MASKING, type Config } from "../../lib/api";

  let { global, engagement }: { global: Config; engagement: Config } = $props();

  type Key = "client" | "hosts" | "users" | "sensitive_headers" | "sensitive_keys" | "allow_domains";
  const ROWS: { key: Key; label: string }[] = [
    { key: "client", label: "Client names" },
    { key: "hosts", label: "Hosts" },
    { key: "users", label: "Users" },
    { key: "sensitive_headers", label: "Sensitive headers" },
    { key: "sensitive_keys", label: "Sensitive keys" },
    { key: "allow_domains", label: "Allowed domains" },
  ];

  const masking = $derived(engagement.masking ?? global.masking ?? DEFAULT_MASKING);
  const maskingFrom = $derived(
    engagement.masking ? "engagement" : global.masking ? "global" : "default",
  );
  const terms = $derived([
    ...global.terms.filter((t) => t.value).map((t) => ({ ...t, from: "global" })),
    ...engagement.terms.filter((t) => t.value).map((t) => ({ ...t, from: "engagement" })),
  ]);
</script>

<div class="effective">
  <h3>Effective rules</h3>
  <p class="legend">
    <span class="chip">global</span> applies to every engagement ·
    <span class="chip accent">engagement</span> only here
  </p>
  {#each ROWS as row (row.key)}
    {@const g = global[row.key]}
    {@const e = engagement[row.key]}
    <div class="group">
      <div class="label">{row.label} <span class="muted">{g.length + e.length}</span></div>
      {#if g.length + e.length === 0}
        <span class="muted none">none</span>
      {:else}
        <div class="chips">
          {#each g as v (v)}<span class="chip" title="global">{v}</span>{/each}
          {#each e as v (v)}<span class="chip accent" title="engagement">{v}</span>{/each}
        </div>
      {/if}
    </div>
  {/each}
  <div class="group">
    <div class="label">Custom terms <span class="muted">{terms.length}</span></div>
    {#if terms.length === 0}
      <span class="muted none">none</span>
    {:else}
      <div class="chips">
        {#each terms as t, i (i)}
          <span class="chip" class:accent={t.from === "engagement"} title={t.from}
            >{t.value} → {t.replacement ?? "[REDACTED]"}</span
          >
        {/each}
      </div>
    {/if}
  </div>
  <div class="group">
    <div class="label">Masking <span class="muted">from {maskingFrom}</span></div>
    <div class="chips">
      <span class="chip" class:accent={maskingFrom === "engagement"}
        >ids −{Math.round(masking.id_cut * 100)}%</span
      >
      <span class="chip" class:accent={maskingFrom === "engagement"}
        >secrets {Math.round(masking.secret_keep * 100)}%</span
      >
      <span class="chip" class:accent={maskingFrom === "engagement"}
        >personal {Math.round(masking.pii_keep * 100)}%</span
      >
    </div>
  </div>
</div>

<style>
  .effective {
    position: sticky;
    top: 0;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }
  h3 {
    margin: 0 0 4px;
    font-size: 13px;
  }
  .legend {
    margin: 0 0 14px;
    color: var(--muted);
    font-size: 12px;
  }
  .group {
    margin-bottom: 12px;
  }
  .label {
    margin-bottom: 4px;
    font-size: 12px;
    font-weight: 600;
  }
  .none {
    font-size: 12px;
  }
</style>
