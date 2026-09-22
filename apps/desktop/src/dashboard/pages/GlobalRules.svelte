<script lang="ts">
  import type { Config, Engagement } from "../../lib/api";
  import RulesEditor from "../components/RulesEditor.svelte";

  let { global = $bindable(), engagements }: { global: Config; engagements: Engagement[] } = $props();
</script>

<h2>Global rules</h2>
<p class="muted lead">
  Apply to every engagement{engagements.length ? ` (${engagements.length} saved)` : ""}. Put the client's
  name and anything that never changes here; per-test details belong in an engagement.
</p>
<div class="narrow">
  <RulesEditor bind:config={global} scope="global" />
</div>

<style>
  h2 {
    margin: 0 0 4px;
    font-size: 20px;
  }
  .lead {
    margin: 0 0 18px;
    max-width: 680px;
  }
  .narrow {
    max-width: 760px;
  }
</style>
