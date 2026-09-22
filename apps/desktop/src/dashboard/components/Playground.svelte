<script lang="ts">
  import { api, CATEGORY_LABELS, type Review } from "../../lib/api";

  const SAMPLE = `POST /api/v2/accounts/88127/transfers HTTP/1.1
Host: api.globexbank.example
Cookie: GBX_SESSION=a7f3e9c1d5b2084f6e1a9c3d7b5f2e80
X-Forwarded-For: 203.0.113.45

{"toAccountId": "99301", "email": "maria.gomez@globexbank.example", "otp": "482913"}`;

  let input = $state(SAMPLE);
  let result = $state<Review | null>(null);
  let timer: ReturnType<typeof setTimeout>;

  $effect(() => {
    const text = input;
    clearTimeout(timer);
    timer = setTimeout(async () => (result = await api.preview(text)), 120);
  });

  const count = $derived(result?.segments.filter((s) => s.kind === "redacted").length ?? 0);
</script>

<p class="intro">
  Try the active profile on any text. Nothing here is copied or saved. Changes to the rules apply
  after saving.
</p>
<div class="panes">
  <label>
    <span>Input</span>
    <textarea bind:value={input} spellcheck="false"></textarea>
  </label>
  <div>
    <span>Output · {count} redacted{result ? ` · ${result.format.replaceAll("_", " ")}` : ""}</span>
    <pre>{#if result}{#each result.segments as s, i (i)}{#if s.kind === "plain"}{s.text}{:else}<mark
              class="redacted cat-{s.category}"
              title={CATEGORY_LABELS[s.category]}>{s.replacement}</mark
            >{/if}{/each}{/if}</pre>
  </div>
</div>

<style>
  .intro {
    color: var(--muted);
    margin-top: 0;
  }
  .panes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    height: calc(100vh - 190px);
  }
  label,
  .panes > div {
    display: grid;
    grid-template-rows: auto 1fr;
    gap: 6px;
    min-height: 0;
  }
  span {
    color: var(--muted);
    font-size: 12px;
  }
  textarea,
  pre {
    margin: 0;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    font: 12px/1.6 var(--mono);
    overflow: auto;
    resize: none;
    white-space: pre-wrap;
    word-break: break-all;
  }
</style>
