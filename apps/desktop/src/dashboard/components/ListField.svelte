<script lang="ts">
  // Editable list of strings shown as chips. Enter or comma adds a value.
  let {
    label,
    help = "",
    placeholder = "",
    values = $bindable(),
  }: { label: string; help?: string; placeholder?: string; values: string[] } = $props();

  let draft = $state("");
  const id = `list-${Math.random().toString(36).slice(2)}`;

  function add() {
    const items = draft
      .split(",")
      .map((v) => v.trim())
      .filter((v) => v && !values.includes(v));
    values = [...values, ...items];
    draft = "";
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      add();
    } else if (e.key === "Backspace" && !draft && values.length) {
      values = values.slice(0, -1);
    }
  }
</script>

<div class="field">
  <label for={id}>{label}</label>
  {#if help}<p class="help">{help}</p>{/if}
  <div class="box">
    {#each values as value, i (value)}
      <span class="chip">
        {value}
        <button aria-label="Remove {value}" onclick={() => (values = values.filter((_, j) => j !== i))}
          >×</button
        >
      </span>
    {/each}
    <input {id} bind:value={draft} {placeholder} onkeydown={onKeydown} onblur={add} />
  </div>
</div>

<style>
  .field {
    margin-bottom: 18px;
  }
  label {
    font-weight: 600;
  }
  .help {
    margin: 2px 0 6px;
    color: var(--muted);
    font-size: 12px;
  }
  .box {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .box:focus-within {
    border-color: var(--focus);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 4px 1px 8px;
    border-radius: 999px;
    background: var(--surface-2);
    font: 12px var(--mono);
  }
  .chip button {
    border: 0;
    background: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0 4px;
  }
  input {
    flex: 1;
    min-width: 160px;
    border: 0;
    outline: 0;
    background: transparent;
    padding: 2px 4px;
  }
</style>
