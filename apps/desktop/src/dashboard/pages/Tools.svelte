<script lang="ts">
  import type { AppSettings } from "../../lib/api";
  import Icon, { type IconName } from "../../lib/Icon.svelte";
  import Playground from "../components/Playground.svelte";
  import Compare from "./Compare.svelte";
  import Restore from "./Restore.svelte";

  type Tool = "restore" | "playground" | "compare";

  let {
    settings,
    profileName,
    tool = $bindable("restore"),
  }: { settings: AppSettings; profileName: string; tool?: Tool } = $props();

  const TOOLS: { id: Tool; label: string; icon: IconName }[] = [
    { id: "restore", label: "Restore", icon: "restore" },
    { id: "playground", label: "Playground", icon: "playground" },
    { id: "compare", label: "Compare", icon: "compare" },
  ];
</script>

<header>
  <h2>Tools</h2>
  <div class="tabs" role="tablist" aria-label="Tools">
    {#each TOOLS as t (t.id)}
      <button role="tab" aria-selected={tool === t.id} onclick={() => (tool = t.id)}>
        <Icon name={t.icon} />
        {t.label}
      </button>
    {/each}
  </div>
</header>

<div role="tabpanel">
  {#if tool === "restore"}
    <Restore {settings} {profileName} />
  {:else if tool === "playground"}
    <Playground />
  {:else}
    <Compare {profileName} />
  {/if}
</div>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 24px;
    margin-bottom: 20px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
  }
  .tabs {
    display: flex;
    gap: 4px;
  }
  .tabs button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .tabs button:hover {
    color: var(--text);
  }
  .tabs button[aria-selected="true"] {
    border-color: var(--border);
    background: var(--surface);
    color: var(--text);
    font-weight: 500;
  }
</style>
