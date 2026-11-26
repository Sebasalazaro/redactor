<script lang="ts">
  // Files and folders to redact: dropped on the window or picked in the
  // native dialogs. Paths never pass through this page; Rust reads and
  // writes the files and this page only sees names and results. Anything
  // added opens the review dialog; nothing is written until Create.
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { api, type RedactedFile } from "../../lib/api";
  import Icon from "../../lib/Icon.svelte";
  import FilesModal from "./FilesModal.svelte";

  let files = $state<RedactedFile[]>([]);
  let hovering = $state(false);
  let busy = $state(false);
  let last = $state<{ created: number; left: number; where: string } | null>(null);

  async function run(action: () => Promise<RedactedFile[]>) {
    busy = true;
    try {
      files = await action();
    } finally {
      busy = false;
    }
  }

  const add = (what: "files" | "folder") => run(what === "files" ? api.pickFiles : api.pickFolder);

  async function close() {
    const created = files.filter((f) => f.status === "saved");
    if (created.length) {
      const roots = [...new Set(created.map((f) => (f.segments.length > 1 ? f.segments[0] : f.base)))];
      last = {
        created: created.length,
        left: files.length - created.length,
        where: roots.length === 1 ? roots[0] : `${roots.length} places`,
      };
    }
    await run(api.clearFiles);
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
  <div class="drop">
    <Icon name="upload" size={18} />
    <div class="text">
      <strong>{hovering ? "Drop to review" : "Redact files and folders"}</strong>
      <span class="muted">
        {#if last}
          Last: {last.created} {last.created === 1 ? "file" : "files"} saved in {last.where}{last.left
            ? `, ${last.left} left out`
            : ""}
        {:else}
          Excel, CSV, JSON, Postman collections, logs. A folder becomes a redacted copy next to it
        {/if}
      </span>
    </div>
    <button class="btn small" onclick={() => add("files")} disabled={busy}>
      <Icon name="file" size={14} /> Choose files
    </button>
    <button class="btn small" onclick={() => add("folder")} disabled={busy}>
      <Icon name="folder" size={14} /> Choose folder
    </button>
  </div>
</section>

{#if files.length}
  <FilesModal
    {files}
    {busy}
    oncreate={() => run(api.saveFiles)}
    oncancel={() => run(api.clearFiles)}
    onclose={close}
    onadd={add}
    onremove={(id) => run(() => api.removeFile(id))}
  />
{/if}

<style>
  .files {
    flex: none;
    margin-top: 16px;
    padding: 12px;
    transition: border-color 120ms ease;
  }
  .files.hovering {
    border-color: var(--accent);
  }
  .drop {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border: 1px dashed var(--border);
    border-radius: 8px;
    color: var(--muted);
  }
  .hovering .drop {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--text);
  }
  .text {
    flex: 1;
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .text strong {
    color: var(--text);
    font-weight: 500;
  }
  .text span {
    overflow: hidden;
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
