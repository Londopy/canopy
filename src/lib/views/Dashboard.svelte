<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { errText } from "../api";
  import Icon from "../components/Icon.svelte";
  import { app } from "../state.svelte";

  interface Section {
    level: string;
    title: string;
    line: number;
    words: number;
    formulas: number;
  }
  interface Goals {
    target: number;
    deadline: string | null;
    daily: number;
  }
  interface Data {
    total: number;
    formulas: number;
    files: { path: string; words: number; sections: Section[] }[];
    days: [string, number][];
    today: number;
    goals: Goals;
    neededPerDay: number | null;
    daysLeft: number | null;
    main: string | null;
  }

  const links = $derived(app.config?.links ?? []);
  let picked = $state<string | null>(null);
  const linkId = $derived(picked && links.some((l) => l.id === picked) ? picked : (app.editorLink ?? links[0]?.id ?? null));

  let data = $state<Data | null>(null);
  let error = $state<string | null>(null);
  let editing = $state(false);
  let target = $state(0);
  let deadline = $state("");
  let daily = $state(0);

  async function load(id: string) {
    try {
      data = await invoke<Data>("dashboard", { linkId: id });
      error = null;
      target = data.goals.target;
      deadline = data.goals.deadline ?? "";
      daily = data.goals.daily;
    } catch (e) {
      error = errText(e);
    }
  }

  // refresh when the project changes or a sync lands
  $effect(() => {
    const id = linkId;
    void (id ? app.sync[id]?.lastSync : null);
    if (id) void load(id);
  });

  async function saveGoals() {
    if (!linkId) return;
    try {
      await invoke("set_goals", { linkId, goals: { target: Math.max(0, Math.round(target)), deadline: deadline || null, daily: Math.max(0, Math.round(daily)) } });
      editing = false;
      await load(linkId);
    } catch (e) {
      app.toast("error", errText(e));
    }
  }

  const fmt = (n: number) => n.toLocaleString();
  // the day's goal: set by hand, or what the deadline asks for
  const dayGoal = $derived(data ? data.goals.daily || data.neededPerDay || 0 : 0);
  const maxDay = $derived(Math.max(1, dayGoal, ...(data?.days.map(([, w]) => Math.abs(w)) ?? [1])));
  const progress = $derived(data && data.goals.target ? Math.min(1, data.total / data.goals.target) : null);

  function openAt(path: string, line: number) {
    if (!linkId) return;
    app.editorLink = linkId;
    app.editorOpen = { path, line };
    app.view = "editor";
  }
</script>

<section class="dash">
  <header>
    <h1>Writing dashboard</h1>
    {#if links.length > 1}
      <select value={linkId} onchange={(e) => (picked = e.currentTarget.value)} aria-label="Project">
        {#each links as l (l.id)}<option value={l.id}>{l.project_name}</option>{/each}
      </select>
    {/if}
  </header>

  {#if !linkId}
    <div class="empty">
      <Icon name="chart" size={30} />
      <p>The dashboard counts words in projects synced to a folder. Sync one from <strong>Projects</strong> first.</p>
    </div>
  {:else if error}
    <p class="error">{error}</p>
  {:else if data}
    <div class="cards">
      <div class="card">
        <span class="label">Words</span>
        <span class="big">{fmt(data.total)}</span>
        {#if progress !== null}
          <div class="bar"><div style={`width: ${progress * 100}%`}></div></div>
          <span class="muted">{Math.round(progress * 100)}% of {fmt(data.goals.target)}</span>
        {:else}
          <span class="muted">{fmt(data.formulas)} formulas</span>
        {/if}
      </div>
      <div class="card">
        <span class="label">Today</span>
        <span class="big" class:neg={data.today < 0}>{data.today > 0 ? "+" : ""}{fmt(data.today)}</span>
        {#if dayGoal}
          <span class="muted">{data.today >= dayGoal ? "Daily goal met" : `${fmt(dayGoal - data.today)} to go today`}</span>
        {:else}
          <span class="muted">words since yesterday</span>
        {/if}
      </div>
      <div class="card">
        <span class="label">Deadline</span>
        {#if data.goals.deadline}
          <span class="big">{data.daysLeft !== null && data.daysLeft > 0 ? `${data.daysLeft} day${data.daysLeft === 1 ? "" : "s"}` : "Past"}</span>
          <span class="muted">
            {new Date(data.goals.deadline + "T00:00").toLocaleDateString(undefined, { dateStyle: "medium" })}
            {#if data.neededPerDay}· {fmt(data.neededPerDay)} words a day{/if}
          </span>
        {:else}
          <span class="big dim">None</span>
          <span class="muted">set a goal to track it</span>
        {/if}
      </div>
    </div>

    <div class="row">
      <h2>Last 30 days</h2>
      <button class="ghost small" onclick={() => (editing = !editing)}>{editing ? "Cancel" : "Set goals"}</button>
    </div>
    {#if editing}
      <form class="goals" onsubmit={(e) => (e.preventDefault(), saveGoals())}>
        <label>Total words <input type="number" min="0" step="100" bind:value={target} /></label>
        <label>Deadline <input type="date" bind:value={deadline} /></label>
        <label>Words a day <input type="number" min="0" step="50" bind:value={daily} placeholder="from the deadline" /></label>
        <button class="primary small" type="submit">Save</button>
      </form>
    {/if}
    <div class="chart" role="img" aria-label="Words written per day over the last 30 days">
      {#each data.days as [day, w] (day)}
        <div class="col" title={`${day}: ${w > 0 ? "+" : ""}${w} words`}>
          <div class="b" class:neg={w < 0} class:met={dayGoal > 0 && w >= dayGoal} style={`height: ${(Math.abs(w) / maxDay) * 100}%`}></div>
        </div>
      {/each}
      {#if dayGoal}
        <div class="goal" style={`bottom: ${(dayGoal / maxDay) * 100}%`}><span>{fmt(dayGoal)}/day</span></div>
      {/if}
    </div>

    <h2>By section</h2>
    {#if data.files.length === 0}
      <p class="muted">No main file found: pick one in the editor.</p>
    {:else}
      <table>
        <tbody>
          {#each data.files as f (f.path)}
            <tr class="file"><td colspan="2"><code>{f.path}</code></td><td class="n">{fmt(f.words)}</td></tr>
            {#each f.sections.filter((s) => s.level !== "preamble" || s.words > 0) as s (s.line)}
              <tr>
                <td class="lvl lvl-{s.level}">
                  <button class="link" onclick={() => openAt(f.path, s.line)}>{s.title || "(before the first heading)"}</button>
                </td>
                <td class="muted">{s.formulas ? `${s.formulas} formula${s.formulas === 1 ? "" : "s"}` : ""}</td>
                <td class="n">{fmt(s.words)}</td>
              </tr>
            {/each}
          {/each}
        </tbody>
      </table>
    {/if}
    <p class="muted foot">Counts body text the way texcount does, roughly: commands, comments and citations don't count; math counts as formulas.</p>
  {/if}
</section>

<style>
  .dash {
    max-width: 920px;
    margin: 0 auto;
    padding: 28px 32px 56px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 18px;
  }
  h1 {
    font-size: 22px;
    margin: 0;
  }
  h2 {
    font-size: 14px;
    margin: 26px 0 10px;
  }
  select {
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 12px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
  }
  .label {
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .big {
    font-size: 26px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .big.neg {
    color: var(--danger);
  }
  .big.dim {
    color: var(--text-faint);
  }
  .muted {
    color: var(--text-faint);
    font-size: 12.5px;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-sunken);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
  }
  .row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  button.small {
    padding: 4px 10px;
    font-size: 12.5px;
  }
  .goals {
    display: flex;
    gap: 12px;
    align-items: flex-end;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }
  .goals label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .goals input {
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    width: 150px;
  }
  .chart {
    position: relative;
    height: 140px;
    display: flex;
    align-items: flex-end;
    gap: 3px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
  }
  .col {
    flex: 1;
    height: 100%;
    display: flex;
    align-items: flex-end;
  }
  .b {
    width: 100%;
    min-height: 1px;
    border-radius: 3px 3px 0 0;
    background: var(--accent-soft);
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .b.met {
    background: var(--accent);
  }
  .b.neg {
    outline-color: var(--danger);
    background: rgba(194, 69, 61, 0.15);
  }
  .goal {
    position: absolute;
    left: 8px;
    right: 8px;
    border-top: 1px dashed var(--text-faint);
  }
  .goal span {
    position: absolute;
    right: 0;
    top: -18px;
    font-size: 11px;
    color: var(--text-faint);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  td {
    padding: 4px 6px;
    border-bottom: 1px solid var(--border-soft);
  }
  tr.file td {
    padding-top: 12px;
    font-weight: 600;
  }
  .n {
    text-align: right;
    font-variant-numeric: tabular-nums;
    width: 90px;
  }
  .lvl-subsection {
    padding-left: 22px;
  }
  .lvl-subsubsection,
  .lvl-paragraph {
    padding-left: 38px;
  }
  .link {
    border: 0;
    background: transparent;
    color: var(--text);
    padding: 0;
    cursor: pointer;
    text-align: left;
  }
  .link:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .foot {
    margin-top: 18px;
  }
  .error {
    color: var(--danger);
  }
  .empty {
    text-align: center;
    color: var(--accent);
    padding: 12vh 24px;
  }
  .empty p {
    color: var(--text-muted);
  }
</style>
