<script lang="ts">
  // Creates an engagement from any display name; the file id is derived.
  import { api } from "../../lib/api";
  import { slugify } from "../../lib/format";

  let {
    oncreated,
    compact = false,
  }: { oncreated: (id: string, activate: boolean) => void; compact?: boolean } = $props();

  let name = $state("");
  let activate = $state(true);
  let error = $state("");
  let busy = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!name.trim() || busy) return;
    busy = true;
    try {
      const id = await api.createEngagement(name);
      name = "";
      error = "";
      oncreated(id, activate);
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<form class:compact onsubmit={submit}>
  <div class="row">
    <input
      class="text"
      bind:value={name}
      placeholder="Globex Q3 — Web app"
      aria-label="Engagement name"
      maxlength="120"
    />
    <button class="btn primary" type="submit" disabled={!name.trim() || busy}>Create</button>
  </div>
  {#if name.trim()}
    <p class="hint">
      Saved as <code>engagements/{slugify(name)}.toml</code>
    </p>
  {/if}
  {#if !compact}
    <label class="activate">
      <input type="checkbox" bind:checked={activate} /> Make it the active profile
    </label>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
</form>

<style>
  .row {
    display: flex;
    gap: 8px;
  }
  input.text {
    flex: 1;
    min-width: 0;
  }
  .hint {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 12px;
  }
  .activate {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-top: 8px;
    font-size: 12px;
  }
  .error {
    margin: 6px 0 0;
    color: var(--danger);
    font-size: 12px;
  }
  .compact .row {
    flex-direction: column;
  }
</style>
