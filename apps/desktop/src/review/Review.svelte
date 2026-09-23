<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { api, CATEGORY_LABELS, compose, type Review, type Segment } from "../lib/api";

  /** A segment as shown: can be revealed, or added by hand or by the AI. */
  type Item = Segment & { revealed?: boolean; manual?: boolean; ai?: string };

  let review = $state<Review | null>(null);
  let items = $state<Item[]>([]);
  let error = $state<string | null>(null);
  let doc = $state<HTMLElement>();
  let scanning = $state(false);
  let scanResult = $state("");

  const redacted = $derived(items.filter((s) => s.kind === "redacted"));
  const revealed = $derived(redacted.filter((s) => s.revealed).length);
  const counts = $derived.by(() => {
    const map = new Map<string, number>();
    for (const s of redacted) {
      const label = s.ai ? "AI" : s.manual ? "Manual" : CATEGORY_LABELS[s.category];
      map.set(label, (map.get(label) ?? 0) + 1);
    }
    return [...map.entries()].sort((a, b) => b[1] - a[1]);
  });

  async function load() {
    error = null;
    scanResult = "";
    review = await api.getReview();
    items = review ? review.segments.map((s) => ({ ...s })) : [];
    doc?.scrollTo(0, 0);
  }

  function toggle(i: number) {
    const s = items[i];
    if (s.kind === "redacted") s.revealed = !s.revealed;
  }

  /** Masks the text selected inside a plain segment. */
  async function maskSelection() {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed || !sel.anchorNode || !sel.focusNode) return;
    const a = sel.anchorNode.parentElement?.closest<HTMLElement>("[data-i]");
    const b = sel.focusNode.parentElement?.closest<HTMLElement>("[data-i]");
    if (!a || a !== b) {
      error = "Select text inside a single unredacted span.";
      return;
    }
    const i = Number(a.dataset.i);
    const seg = items[i];
    if (seg.kind !== "plain") return;
    let start = Math.min(sel.anchorOffset, sel.focusOffset);
    let end = Math.max(sel.anchorOffset, sel.focusOffset);
    // Ignore surrounding whitespace picked up by a double click.
    while (start < end && /\s/.test(seg.text[start])) start++;
    while (end > start && /\s/.test(seg.text[end - 1])) end--;
    if (start === end) return;

    const original = seg.text.slice(start, end);
    const replacement = await api.maskText(original);
    const parts: Item[] = [];
    if (start > 0) parts.push({ kind: "plain", text: seg.text.slice(0, start) });
    parts.push({ kind: "redacted", category: "custom", original, replacement, manual: true });
    if (end < seg.text.length) parts.push({ kind: "plain", text: seg.text.slice(end) });
    items.splice(i, 1, ...parts);
    sel.removeAllRanges();
    error = null;
  }

  /**
   * Runs the local AI model over the text as it would be copied, and masks
   * what it finds inside unredacted text. Values it finds inside already
   * redacted spans are ignored.
   */
  async function deepScan() {
    if (scanning) return;
    scanning = true;
    error = null;
    try {
      const starts: number[] = [];
      let text = "";
      for (const s of items) {
        starts.push(text.length);
        text += s.kind === "plain" ? s.text : s.revealed ? s.original : s.replacement;
      }
      const entities = await api.deepScan(text);
      let added = 0;
      // Walk backwards so splitting an item does not shift earlier indices.
      for (const e of [...entities].sort((a, b) => b.start - a.start)) {
        const i = starts.findLastIndex((start) => start <= e.start);
        const item = items[i];
        if (i < 0 || item.kind !== "plain" || e.end > starts[i] + item.text.length) continue;
        const from = e.start - starts[i];
        const to = e.end - starts[i];
        const parts: Item[] = [];
        if (from > 0) parts.push({ kind: "plain", text: item.text.slice(0, from) });
        parts.push({
          kind: "redacted",
          category: e.category,
          original: item.text.slice(from, to),
          replacement: e.replacement,
          ai: `${e.label} (${Math.round(e.score * 100)}%)`,
        });
        if (to < item.text.length) parts.push({ kind: "plain", text: item.text.slice(to) });
        items.splice(i, 1, ...parts);
        added++;
      }
      scanResult = added ? `AI masked ${added} more` : "AI found nothing new";
    } catch (e) {
      error = String(e);
    } finally {
      scanning = false;
    }
  }

  /** Drops the texts from this page's memory before the window goes away. */
  function forget() {
    review = null;
    items = [];
  }

  async function confirm() {
    try {
      const text = compose(items);
      // What stayed masked goes to the vault; revealed values are not needed.
      const pairs = items.flatMap((s) =>
        s.kind === "redacted" && !s.revealed
          ? [{ replacement: s.replacement, original: s.original }]
          : [],
      );
      forget();
      await api.finishReview(text, pairs);
    } catch (e) {
      error = String(e);
    }
  }

  function cancel() {
    forget();
    api.cancelReview();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    } else if (e.key === "Enter" && !e.shiftKey && review) {
      e.preventDefault();
      confirm();
    } else if (e.key.toLowerCase() === "m" && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      maskSelection();
    }
  }

  onMount(() => {
    load();
    const unlisten = listen("review-ready", load);
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<main>
  <header>
    <div class="title">
      <h1>Review before copying</h1>
      {#if review}
        <span class="badge">{review.format.replaceAll("_", " ")}</span>
        <span class="badge">{review.engagement ?? "global profile"}</span>
      {/if}
    </div>
    {#if review}
      <div class="counts">
        <strong>{redacted.length}</strong> redacted
        {#each counts as [label, n] (label)}
          <span class="badge">{label} {n}</span>
        {/each}
        {#if revealed > 0}
          <span class="warn">{revealed} revealed</span>
        {/if}
      </div>
    {/if}
  </header>

  <section class="doc" bind:this={doc}>
    {#if !review}
      <p class="empty">Nothing to review. Copy some traffic and press the hotkey.</p>
    {:else if items.length === 0}
      <p class="empty">The clipboard is empty.</p>
    {:else}
      <pre>{#each items as s, i (i)}{#if s.kind === "plain"}<span data-i={i}>{s.text}</span>{:else}<span
            class="redacted cat-{s.category}"
            class:revealed={s.revealed}
            role="button"
            tabindex="0"
            class:ai={!!s.ai}
            title="{s.ai ? `AI: ${s.ai}` : s.manual ? 'Manual' : CATEGORY_LABELS[s.category]} · click to {s.revealed ? 'mask' : 'reveal'}"
            onclick={() => toggle(i)}
            onkeydown={(e) => (e.key === " " ? (e.preventDefault(), toggle(i)) : undefined)}
            >{s.revealed ? s.original : s.replacement}</span
          >{/if}{/each}</pre>
    {/if}
  </section>

  <footer>
    <div class="hints">
      <span><kbd>Click</kbd> reveal / mask</span>
      <span><kbd>Select</kbd> + <kbd>M</kbd> mask by hand</span>
      {#if scanResult}<span class="ok">{scanResult}</span>{/if}
      {#if error}<span class="error">{error}</span>{/if}
    </div>
    <div class="actions">
      {#if review}
        <button
          class="btn"
          onclick={deepScan}
          disabled={!review.ai_installed || scanning}
          title={review.ai_installed
            ? "Find names, organizations and other data with the local AI model"
            : "Install the model first: scripts/fetch-model.sh"}
        >
          {scanning ? "Scanning…" : "Deep scan (AI)"}
        </button>
      {/if}
      <button class="btn" onclick={cancel}>Cancel <kbd>Esc</kbd></button>
      <button class="btn primary" onclick={confirm} disabled={!review}>Copy <kbd>↵</kbd></button>
    </div>
  </footer>
</main>

<style>
  main {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: 100%;
  }
  header {
    padding: 14px 18px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h1 {
    margin: 0 6px 0 0;
    font-size: 15px;
    font-weight: 600;
  }
  .counts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    color: var(--muted);
  }
  .counts strong {
    color: var(--text);
  }
  .warn {
    color: var(--danger);
    font-weight: 600;
  }
  .doc {
    overflow: auto;
    padding: 14px 18px;
  }
  pre {
    margin: 0;
    font: 12.5px/1.65 var(--mono);
    white-space: pre-wrap;
    word-break: break-all;
  }
  .redacted {
    cursor: pointer;
  }
  .redacted:hover {
    outline: 1px solid var(--c);
  }
  .redacted.revealed {
    color: var(--danger);
    background: var(--danger-soft);
    text-decoration: underline wavy;
  }
  .empty {
    color: var(--muted);
    text-align: center;
    margin-top: 80px;
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 18px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
  .hints {
    display: flex;
    gap: 14px;
    color: var(--muted);
    font-size: 12px;
  }
  .error {
    color: var(--danger);
  }
  .ok {
    color: var(--ok);
  }
  .redacted.ai {
    outline: 1px dashed var(--c);
    outline-offset: 1px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
</style>
