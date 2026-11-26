<script lang="ts" module>
  import type { RedactedFile } from "../../lib/api";

  /** A folder or file of what will be created, below one base folder. */
  export interface TreeNode {
    key: string;
    name: string;
    file: RedactedFile | null;
    children: TreeNode[];
    ready: number;
    skipped: number;
    values: number;
  }

  /** Groups outputs by the folder they land in, then nests their paths. */
  export function buildTree(files: RedactedFile[]): TreeNode[] {
    const roots = new Map<string, TreeNode>();
    for (const f of files) {
      let node = roots.get(f.base);
      if (!node) {
        node = { key: f.base, name: f.base, file: null, children: [], ready: 0, skipped: 0, values: 0 };
        roots.set(f.base, node);
      }
      f.segments.forEach((segment, i) => {
        const key = `${node!.key}/${segment}`;
        let child = node!.children.find((c) => c.key === key);
        if (!child) {
          child = { key, name: segment, file: null, children: [], ready: 0, skipped: 0, values: 0 };
          node!.children.push(child);
        }
        if (i === f.segments.length - 1) child.file = f;
        node = child;
      });
    }
    const tally = (n: TreeNode) => {
      if (n.file && !n.children.length) {
        const ok = n.file.status === "ready" || n.file.status === "saved";
        n.ready = ok ? 1 : 0;
        n.skipped = ok ? 0 : 1;
        n.values = n.file.values;
        return;
      }
      // Folders first, then files, each by name.
      n.children.sort((a, b) => Number(!!a.file && !a.children.length) - Number(!!b.file && !b.children.length) || a.name.localeCompare(b.name));
      for (const c of n.children) {
        tally(c);
        n.ready += c.ready;
        n.skipped += c.skipped;
        n.values += c.values;
      }
    };
    const list = [...roots.values()];
    list.forEach(tally);
    return list;
  }
</script>

<script lang="ts">
  import { api, CATEGORY_LABELS } from "../../lib/api";
  import { format } from "../../lib/format";
  import Icon from "../../lib/Icon.svelte";

  let {
    files,
    busy,
    oncreate,
    oncancel,
    onclose,
    onadd,
    onremove,
  }: {
    files: RedactedFile[];
    busy: boolean;
    oncreate: () => void;
    oncancel: () => void;
    onclose: () => void;
    onadd: (what: "files" | "folder") => void;
    onremove: (id: number) => void;
  } = $props();

  const tree = $derived(buildTree(files));
  const ready = $derived(files.filter((f) => f.status === "ready"));
  const skipped = $derived(files.filter((f) => f.status === "skipped" || f.status === "failed"));
  const saved = $derived(files.filter((f) => f.status === "saved"));
  const done = $derived(saved.length > 0 && ready.length === 0);
  const hidden = $derived(files.reduce((n, f) => n + f.values, 0));

  let collapsed = $state<Set<string>>(new Set());
  let selectedId = $state<number | null>(null);
  const selected = $derived(
    files.find((f) => f.id === selectedId) ?? files.find((f) => f.status === "ready" || f.status === "saved") ?? null,
  );
  let copied = $state(false);

  function toggle(key: string) {
    const next = new Set(collapsed);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    collapsed = next;
  }

  async function copyText(id: number) {
    await api.copyFileText(id);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  /** The last two folders of a base path: enough to recognise it. */
  function shortBase(base: string) {
    const parts = base.split(/[\\/]/).filter(Boolean);
    return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : base;
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !busy) (done ? onclose : oncancel)();
  }
</script>

<svelte:window {onkeydown} />

<div class="overlay" role="presentation">
  <div class="modal" role="dialog" aria-modal="true" aria-labelledby="files-title">
    <header>
      <div>
        <h3 id="files-title">{done ? "Redacted copies created" : "Review what will be created"}</h3>
        <p class="muted">
          {done
            ? "Originals were not changed."
            : "Originals are not changed. Files that cannot be redacted are left out."}
        </p>
      </div>
      <dl class="stats">
        <div>
          <dt>{done ? "Created" : "To create"}</dt>
          <dd>{done ? saved.length : ready.length}</dd>
        </div>
        <div>
          <dt>Left out</dt>
          <dd class:warn={skipped.length > 0}>{skipped.length}</dd>
        </div>
        <div>
          <dt>Values hidden</dt>
          <dd>{hidden}</dd>
        </div>
      </dl>
    </header>

    <div class="body">
      <div class="tree" role="tree" aria-label="Files to create">
        {#each tree as root (root.key)}
          <div class="base" title={root.name}>
            <Icon name="folder" size={14} />
            <span>{shortBase(root.name)}</span>
          </div>
          {@render branch(root.children, 1)}
        {/each}
      </div>

      <aside class="preview" aria-label="Preview">
        {#if selected}
          <div class="preview-head">
            <div class="names">
              <span class="from" title="Original">{selected.source}</span>
              <span class="to">
                <Icon name="arrow" size={12} />
                {selected.segments.join("/")}
              </span>
            </div>
            {#if selected.has_text}
              <button class="btn small" onclick={() => copyText(selected.id)}>
                <Icon name={copied ? "check" : "copy"} size={14} />
                {copied ? "Copied" : "Copy as text"}
              </button>
            {/if}
          </div>
          {#if selected.status === "skipped" || selected.status === "failed"}
            <p class="reason">{selected.message}</p>
          {:else}
            <div class="chips">
              {#if selected.format}<span class="chip">{format(selected.format)}</span>{/if}
              {#each selected.categories as [cat, n] (cat)}
                <span class="chip">{CATEGORY_LABELS[cat]} <span class="muted">{n}</span></span>
              {/each}
            </div>
            {#if selected.preview !== null}
              <pre>{selected.preview}</pre>
            {:else}
              <p class="muted">Saved.</p>
            {/if}
          {/if}
        {:else}
          <p class="muted empty">Nothing to show yet</p>
        {/if}
      </aside>
    </div>

    <footer>
      {#if done}
        <span class="muted">Saved next to the originals</span>
        <button class="btn primary" onclick={onclose}>Done</button>
      {:else}
        <button class="btn" onclick={() => onadd("files")} disabled={busy}><Icon name="plus" /> Files</button>
        <button class="btn" onclick={() => onadd("folder")} disabled={busy}><Icon name="folder" /> Folder</button>
        <span class="spacer"></span>
        <button class="btn" onclick={oncancel} disabled={busy}>Cancel</button>
        <button class="btn primary" onclick={oncreate} disabled={busy || !ready.length}>
          <Icon name="save" />
          {busy ? "Creating…" : ready.length === 1 ? "Create 1 file" : `Create ${ready.length} files`}
        </button>
      {/if}
    </footer>
  </div>
</div>

{#snippet branch(nodes: TreeNode[], depth: number)}
  {#each nodes as node (node.key)}
    {@const isFile = !!node.file && !node.children.length}
    {#if isFile && node.file}
      {@const f = node.file}
      <div
        class="row file {f.status}"
        class:selected={selected?.id === f.id}
        style:--depth={depth}
        role="treeitem"
        aria-selected={selected?.id === f.id}
        tabindex="0"
        onclick={() => (selectedId = f.id)}
        onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (selectedId = f.id)}
      >
        <span class="dot" aria-hidden="true"></span>
        <Icon name={f.folder ? "folder" : "file"} size={14} />
        <span class="label">{node.name}</span>
        <span class="meta">
          {#if f.status === "ready" || f.status === "saved"}
            {f.values} hidden
          {:else}
            {f.message}
          {/if}
        </span>
        {#if f.status === "ready" && !busy}
          <button
            class="leave"
            aria-label="Leave {node.name} out"
            title="Leave out"
            onclick={(e) => {
              e.stopPropagation();
              onremove(f.id);
            }}><Icon name="x" size={12} /></button
          >
        {/if}
      </div>
    {:else}
      {@const open = !collapsed.has(node.key)}
      <div
        class="row folder"
        style:--depth={depth}
        role="treeitem"
        aria-expanded={open}
        aria-selected="false"
        tabindex="0"
        onclick={() => toggle(node.key)}
        onkeydown={(e) => (e.key === "Enter" || e.key === " ") && toggle(node.key)}
      >
        <span class="chev" class:open><Icon name="chevron" size={12} /></span>
        <Icon name="folder" size={14} />
        <span class="label">{node.name}</span>
        <span class="meta">
          {node.ready} {node.ready === 1 ? "file" : "files"}{node.skipped ? `, ${node.skipped} left out` : ""}
        </span>
      </div>
      {#if open}
        <div role="group">{@render branch(node.children, depth + 1)}</div>
      {/if}
    {/if}
  {/each}
{/snippet}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(0 0 0 / 0.5);
  }
  .modal {
    display: flex;
    flex-direction: column;
    width: min(1040px, 100%);
    height: min(680px, 100%);
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--surface);
    box-shadow: 0 16px 48px rgb(0 0 0 / 0.3);
    animation: rise 160ms ease-out;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.99);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .modal {
      animation: none;
    }
  }
  header {
    display: flex;
    justify-content: space-between;
    gap: 24px;
    padding: 20px 24px 16px;
    border-bottom: 1px solid var(--border);
  }
  h3 {
    margin: 0 0 4px;
    font-size: 16px;
    font-weight: 600;
  }
  header p {
    margin: 0;
    font-size: 12px;
  }
  .stats {
    display: flex;
    gap: 24px;
    margin: 0;
  }
  .stats div {
    text-align: right;
  }
  dt {
    color: var(--muted);
    font-size: 12px;
  }
  dd {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  dd.warn {
    color: var(--cat-secret);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 5fr) minmax(0, 6fr);
  }
  .tree {
    overflow: auto;
    padding: 8px 8px 16px;
    border-right: 1px solid var(--border);
  }
  .base {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 12px 8px 4px;
    color: var(--muted);
    font-size: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 8px 0 calc(8px + (var(--depth) - 1) * 18px);
    border-radius: 6px;
    color: var(--text);
    cursor: pointer;
  }
  .row:hover {
    background: var(--surface-2);
  }
  .row:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
  .row.selected {
    background: var(--surface-2);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .row :global(.icon) {
    color: var(--muted);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font: 12px var(--mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    flex: none;
    max-width: 45%;
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    display: flex;
    transition: transform 120ms ease;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .file.saved .dot {
    background: var(--ok);
  }
  .file.skipped .dot,
  .file.failed .dot {
    background: var(--muted);
  }
  .file.skipped .label,
  .file.failed .label {
    color: var(--muted);
    text-decoration: line-through;
  }
  .file.failed .meta {
    color: var(--danger);
  }
  .leave {
    display: none;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }
  .row:hover .leave {
    display: grid;
  }
  .leave:hover {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .preview {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 16px;
  }
  .preview-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }
  .names {
    display: grid;
    gap: 2px;
    min-width: 0;
    font: 12px var(--mono);
  }
  .from {
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .to {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow-wrap: anywhere;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 8px;
  }
  pre {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 12px;
    overflow: auto;
    border-radius: 8px;
    background: var(--surface-2);
    font: 12px/1.6 var(--mono);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .reason {
    margin: 0;
    color: var(--muted);
  }
  .empty {
    margin: auto;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
  footer > .muted {
    flex: 1;
    font-size: 12px;
  }
  .spacer {
    flex: 1;
  }
</style>
