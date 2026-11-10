<script lang="ts">
  // Compares two Burp site map exports. Files are picked in a native dialog
  // and read in Rust: this page only sees file names and the redacted report.
  import { onMount } from "svelte";
  import { api, type Comparison, type CompareRow, type CompareSection } from "../../lib/api";

  let { profileName }: { profileName: string } = $props();

  let state_ = $state<Comparison>({ high_file: null, low_file: null, report: null });
  let highLabel = $state("admin");
  let lowLabel = $state("user");
  let includeBodies = $state("");
  let query = $state("");
  let busy = $state(false);
  let error = $state("");
  let notice = $state("");
  // Long sections are only rendered once opened.
  let open = $state<Record<string, boolean>>({ same_content: true, different_content: true, inverted: true });

  const report = $derived(state_.report);
  const ready = $derived(!!state_.high_file && !!state_.low_file && !!highLabel.trim() && !!lowLabel.trim());
  const needle = $derived(query.trim().toLowerCase());

  function matches(row: CompareRow): boolean {
    return !needle || row.endpoint.toLowerCase().includes(needle) || row.notes.toLowerCase().includes(needle);
  }

  function visible(section: CompareSection): CompareRow[] {
    return section.rows.filter(matches);
  }

  function flash(text: string) {
    notice = text;
    setTimeout(() => (notice = ""), 2500);
  }

  async function pick(role: "high" | "low") {
    error = "";
    try {
      state_ = await api.pickCompareExport(role);
    } catch (e) {
      error = String(e);
    }
  }

  async function run() {
    busy = true;
    error = "";
    try {
      state_ = await api.runCompare(highLabel.trim(), lowLabel.trim(), includeBodies.trim() || null);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function copy() {
    try {
      await api.copyCompareReport();
      flash("Report copied");
    } catch (e) {
      error = String(e);
    }
  }

  async function save() {
    try {
      if (await api.saveCompareReport()) flash("Report saved");
    } catch (e) {
      error = String(e);
    }
  }

  function badge(statuses: string | undefined): string {
    return statuses?.split(", ").some((s) => s.startsWith("2")) ? "allowed" : "denied";
  }

  onMount(async () => {
    state_ = await api.getCompare();
    if (report) [highLabel, lowLabel] = [report.high_label, report.low_label];
  });
</script>

<header>
  <div>
    <p class="muted">
      Compare two Burp site map exports of different roles, using the rules of
      <strong>{profileName}</strong>. Nothing is sent anywhere: this only looks at what each role
      already requested.
    </p>
  </div>
</header>

<div class="inputs">
  {#each [{ role: "high", title: "More privileges", file: state_.high_file }, { role: "low", title: "Fewer privileges", file: state_.low_file }] as side (side.role)}
    <div class="card export">
      <label class="role">
        <span class="muted">{side.title}</span>
        {#if side.role === "high"}
          <input class="text" bind:value={highLabel} aria-label="Name of the role with more privileges" />
        {:else}
          <input class="text" bind:value={lowLabel} aria-label="Name of the role with fewer privileges" />
        {/if}
      </label>
      <div class="file">
        <span class="name" class:muted={!side.file} title={side.file ?? ""}>
          {side.file ?? "No export chosen"}
        </span>
        <button class="btn" onclick={() => pick(side.role as "high" | "low")} disabled={busy}>
          Choose XML…
        </button>
      </div>
    </div>
  {/each}
</div>

<details class="options">
  <summary>Options</summary>
  <label>
    <span>Add redacted responses of endpoints containing</span>
    <input class="text mono" bind:value={includeBodies} placeholder="GET /api/users/{'{id}'}" spellcheck="false" />
  </label>
  <p class="warn">
    Up to 3 per export. Free text the engine does not recognize stays as it is, so read them before
    you share the report.
  </p>
</details>

<div class="actions">
  <button class="btn primary" onclick={run} disabled={!ready || busy}>
    {busy ? "Comparing…" : "Compare"}
  </button>
  {#if busy}<span class="muted" role="status">Large exports can take a minute.</span>{/if}
  {#if notice}<span class="notice" role="status">{notice}</span>{/if}
</div>

{#if error}<p class="error" role="alert">{error}</p>{/if}

{#if report}
  <section class="results">
    <div class="toolbar">
      <input
        class="text"
        type="search"
        bind:value={query}
        placeholder="Filter endpoints and fields"
        aria-label="Filter endpoints and fields"
      />
      <button class="btn" onclick={copy}>Copy report</button>
      <button class="btn" onclick={save}>Save .md…</button>
    </div>
    <p class="muted small">
      Read it before you share it: it reveals the application's structure, and names the engine does not
      recognize stay readable. Copy and save include up to 150 rows per section.
      Left out: {report.left_out.high_unrequested} never requested and {report.left_out.high_static} static
      files from {report.high_label}; {report.left_out.low_unrequested} and {report.left_out.low_static}
      from {report.low_label}.
    </p>

    {#each report.sections.filter((s) => s.rows.length) as section (section.class)}
      {@const rows = visible(section)}
      {@const paired = section.class !== "high_only" && section.class !== "low_only"}
      <details
        class="section"
        open={open[section.class] || (!!needle && rows.length > 0)}
        ontoggle={(e) => (open[section.class] = e.currentTarget.open)}
      >
        <summary>
          <span class="title">{section.title}</span>
          <span class="count">{needle ? `${rows.length} of ${section.rows.length}` : section.rows.length}</span>
        </summary>
        {#if open[section.class] || needle}
          <p class="muted hint">{section.hint}</p>
          {#if section.prefixes.length}
            <p class="prefixes">
              {#each section.prefixes as [prefix, count] (prefix)}
                <button class="prefix" onclick={() => (query = prefix)}>{prefix} <b>{count}</b></button>
              {/each}
            </p>
          {/if}
          <div class="table">
            <table>
              <thead>
                <tr>
                  <th>Endpoint</th>
                  {#if paired}
                    <th>{report.high_label}</th>
                    <th>{report.low_label}</th>
                    <th class="num">{report.high_label} size</th>
                    <th class="num">{report.low_label} size</th>
                    <th>Notes</th>
                  {:else}
                    <th>Status</th>
                    <th class="num">Size</th>
                    <th class="num">Requests</th>
                  {/if}
                </tr>
              </thead>
              <tbody>
                {#each rows as row (row.origin + row.endpoint)}
                  {@const single = row.high ?? row.low}
                  <tr>
                    <td class="endpoint" title={row.origin}>{row.endpoint}</td>
                    {#if paired}
                      <td><span class="status {badge(row.high?.statuses)}">{row.high?.statuses}</span></td>
                      <td><span class="status {badge(row.low?.statuses)}">{row.low?.statuses}</span></td>
                      <td class="num">{row.high?.size}</td>
                      <td class="num">{row.low?.size}</td>
                      <td class="notes">{row.notes}</td>
                    {:else}
                      <td><span class="status {badge(single?.statuses)}">{single?.statuses}</span></td>
                      <td class="num">{single?.size}</td>
                      <td class="num">{single?.requests}</td>
                    {/if}
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </details>
    {/each}

    {#if report.samples.length}
      <h4>Selected responses</h4>
      {#each report.samples as sample, i (i)}
        <div class="sample">
          <div class="muted small"><strong>{sample.role}</strong> · <code>{sample.endpoint}</code></div>
          <pre>{sample.text}</pre>
        </div>
      {/each}
    {/if}
  </section>
{/if}

<style>
  header p {
    margin: 0 0 14px;
    max-width: 640px;
  }
  .inputs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .export {
    display: grid;
    gap: 10px;
    min-width: 0;
  }
  .role {
    display: grid;
    gap: 4px;
    font-size: 12px;
  }
  .file {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12px;
  }
  .options {
    margin-top: 10px;
    font-size: 13px;
  }
  .options summary {
    color: var(--muted);
    cursor: pointer;
  }
  .options label {
    display: grid;
    gap: 4px;
    margin-top: 8px;
    max-width: 520px;
    font-size: 12px;
    color: var(--muted);
  }
  .mono {
    font-family: var(--mono);
  }
  .warn {
    margin: 6px 0 0;
    color: var(--cat-secret);
    font-size: 12px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 12px 0;
  }
  .notice {
    color: var(--ok);
    font-size: 13px;
  }
  .error {
    color: var(--danger);
  }
  .toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .toolbar input {
    flex: 1;
    min-width: 0;
  }
  .small {
    font-size: 12px;
  }
  .results > .small {
    margin: 8px 0 12px;
  }
  .section {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    margin-bottom: 8px;
  }
  .section summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    cursor: pointer;
    font-weight: 600;
    font-size: 13px;
  }
  .count {
    color: var(--muted);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .hint {
    margin: 0;
    padding: 0 12px 8px;
    font-size: 12px;
  }
  .prefixes {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    padding: 0 12px 8px;
  }
  .prefix {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text);
    padding: 2px 6px;
    font-family: var(--mono);
    font-size: 11px;
    cursor: pointer;
  }
  .prefix b {
    color: var(--muted);
    font-weight: 500;
  }
  .table {
    overflow-x: auto;
    border-top: 1px solid var(--border);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th {
    text-align: left;
    color: var(--muted);
    font-weight: 500;
    white-space: nowrap;
    background: var(--surface-2);
  }
  th,
  td {
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }
  tr:last-child td {
    border-bottom: 0;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .endpoint {
    font-family: var(--mono);
    overflow-wrap: anywhere;
  }
  .notes {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 11px;
    overflow-wrap: anywhere;
    min-width: 180px;
  }
  .status {
    font-family: var(--mono);
    white-space: nowrap;
  }
  .status.allowed {
    color: var(--ok);
  }
  .status.denied {
    color: var(--muted);
  }
  h4 {
    margin: 16px 0 8px;
  }
  .sample {
    margin-bottom: 10px;
  }
  .sample pre {
    margin: 4px 0 0;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    font-family: var(--mono);
    font-size: 12px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 280px;
    overflow-y: auto;
  }
</style>
