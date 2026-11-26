<script lang="ts">
  // Files to redact: dropped on the window or picked in the native dialog.
  // Paths never pass through this page; Rust reads and writes the files and
  // this page only sees names and results.
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { api, CATEGORY_LABELS, type RedactedFile } from "../../lib/api";
  import { format } from "../../lib/format";
  import Icon from "../../lib/Icon.svelte";

  let files = $state<RedactedFile[]>([]);
  let hovering = $state(false);
  let busy = $state(false);
  let open = $state<number | null>(null);
  let copied = $state<number | null>(null);

  async function copyText(id: number) {
    await api.copyFileText(id);
    copied = id;
    setTimeout(() => (copied = null), 1500);
  }

  const ready = $derived(files.filter((f) => f.status === "ready"));

  async function run(action: () => Promise<RedactedFile[]>) {
    busy = true;
    try {
      files = await action();
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    api.getFiles().then((f) => (files = f));
    const unlisten = [
      listen<boolean>("files-hover", (e) => (hovering = e.payload)),
      listen("files-changed", async () => (files = await api.getFiles())),
    ];
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });
</script>

<section class="card files" class:hovering aria-label="Files">
  <div class="head">
    <h3>Files</h3>
    {#if files.length}
      <button class="btn small" onclick={() => run(api.clearFiles)} disabled={busy}>Clear list</button>
    {/if}
  </div>

  {#if !files.length || hovering}
    <div class="drop">
      <Icon name="upload" size={18} />
      <p>
        {hovering ? "Drop to redact" : "Drop Excel, CSV, JSON, Postman collections or any text file here"}
      </p>
      {#if !hovering}
        <button class="btn small" onclick={() => run(api.pickFiles)} disabled={busy}>Choose files</button>
      {/if}
    </div>
  {:else}
    <ul>
      {#each files as f (f.id)}
        <li class="row {f.status}">
          <span class="icon-col"><Icon name={f.status === "saved" ? "check" : f.status === "ready" ? "file" : "alert"} /></span>
          <div class="names">
            <span class="name">{f.name}</span>
            {#if f.status === "ready" || f.status === "saved"}
              <span class="arrow"><Icon name="arrow" size={12} /></span>
              <span class="out">{f.output_name}</span>
            {/if}
          </div>
          <span class="meta">
            {#if f.status === "ready"}
              {f.values} {f.values === 1 ? "value" : "values"} hidden{f.format ? `, ${format(f.format)}` : ""}
            {:else if f.status === "saved"}
              Saved
            {:else}
              {f.message}
            {/if}
          </span>
          <span class="actions">
            {#if f.has_text}
              <button class="btn small" onclick={() => copyText(f.id)} title="Every sheet as a table, to paste into a chat">
                <Icon name={copied === f.id ? "check" : "copy"} size={14} />
                {copied === f.id ? "Copied" : "Copy as text"}
              </button>
            {/if}
            {#if f.preview !== null}
              <button
                class="icon-btn"
                aria-expanded={open === f.id}
                aria-label="Preview {f.name}"
                onclick={() => (open = open === f.id ? null : f.id)}
              >
                <Icon name="chevron" size={14} />
              </button>
            {/if}
            <button class="icon-btn" aria-label="Remove {f.name} from the list" onclick={() => run(() => api.removeFile(f.id))}>
              <Icon name="x" size={14} />
            </button>
          </span>
          {#if open === f.id && f.preview !== null}
            <div class="preview">
              <div class="chips">
                {#each f.categories as [cat, n] (cat)}
                  <span class="chip">{CATEGORY_LABELS[cat]} <span class="muted">{n}</span></span>
                {/each}
              </div>
              <pre>{f.preview}</pre>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
    <div class="foot">
      <button class="btn" onclick={() => run(api.pickFiles)} disabled={busy}><Icon name="plus" /> Add files</button>
      <span class="muted hint">Copies go next to each original; the originals are not changed</span>
      <button class="btn primary" onclick={() => run(api.saveFiles)} disabled={busy || !ready.length}>
        <Icon name="save" />
        {ready.length > 1 ? `Save ${ready.length} redacted copies` : "Save redacted copy"}
      </button>
    </div>
  {/if}
</section>

<style>
  .files {
    flex: none;
    display: flex;
    flex-direction: column;
    max-height: 45%;
    margin-top: 16px;
    padding: 16px;
    transition: border-color 120ms ease;
  }
  .files.hovering {
    border-color: var(--accent);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .drop {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px;
    border: 1px dashed var(--border);
    border-radius: 8px;
    color: var(--muted);
  }
  .hovering .drop {
    border-color: var(--accent);
    color: var(--text);
  }
  .drop p {
    flex: 1;
    margin: 0;
  }
  ul {
    min-height: 0;
    margin: 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }
  .row {
    display: grid;
    grid-template-columns: 20px minmax(0, 1fr) auto auto;
    gap: 8px 12px;
    align-items: center;
    min-height: 36px;
    border-bottom: 1px solid var(--border);
  }
  .icon-col {
    color: var(--muted);
  }
  .saved .icon-col {
    color: var(--ok);
  }
  .rejected .icon-col,
  .failed .icon-col,
  .rejected .meta,
  .failed .meta {
    color: var(--danger);
  }
  .names {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font: 12px var(--mono);
  }
  .name,
  .out {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .out {
    color: var(--muted);
  }
  .arrow {
    display: flex;
    flex: none;
    color: var(--muted);
  }
  .meta {
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    gap: 2px;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }
  .icon-btn:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .icon-btn[aria-expanded="true"] :global(.icon) {
    transform: rotate(90deg);
  }
  .preview {
    grid-column: 2 / -1;
    padding-bottom: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 8px;
  }
  pre {
    max-height: 200px;
    margin: 0;
    padding: 12px;
    overflow: auto;
    border-radius: 8px;
    background: var(--surface-2);
    font: 12px/1.6 var(--mono);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
  }
  .hint {
    flex: 1;
    font-size: 12px;
  }
</style>
