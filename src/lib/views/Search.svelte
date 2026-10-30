<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { errText } from "../api";
  import Icon from "../components/Icon.svelte";
  import { app } from "../state.svelte";

  interface Hit {
    path: string;
    line: number;
    text: string;
    start: number;
    end: number;
  }
  interface Results {
    hits: Hit[];
    files: number;
    truncated: boolean;
  }

  const links = $derived(app.config?.links ?? []);
  let linkId = $state<string | null>(null);
  const current = $derived(linkId && links.some((l) => l.id === linkId) ? linkId : (app.editorLink ?? links[0]?.id ?? null));

  let text = $state("");
  let regex = $state(false);
  let caseSensitive = $state(false);
  let wholeWord = $state(false);
  let results = $state<Results | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let input: HTMLInputElement | undefined = $state();

  // searches as you type, after a short pause; a newer search wins
  let token = 0;
  $effect(() => {
    const q = { text, regex, caseSensitive, wholeWord };
    const id = current;
    if (!id) return;
    const mine = ++token;
    const t = setTimeout(async () => {
      if (!q.text) {
        results = null;
        error = null;
        return;
      }
      busy = true;
      try {
        const r = await invoke<Results>("search_project", { linkId: id, query: q });
        if (mine === token) {
          results = r;
          error = null;
        }
      } catch (e) {
        if (mine === token) error = errText(e);
      } finally {
        if (mine === token) busy = false;
      }
    }, 220);
    return () => clearTimeout(t);
  });

  $effect(() => {
    input?.focus();
  });

  const groups = $derived.by(() => {
    const m = new Map<string, Hit[]>();
    for (const h of results?.hits ?? []) {
      const list = m.get(h.path) ?? [];
      list.push(h);
      m.set(h.path, list);
    }
    return [...m.entries()];
  });

  function open(h: Hit) {
    if (!current) return;
    app.editorLink = current;
    app.editorOpen = { path: h.path, line: h.line };
    app.view = "editor";
  }

  // the line split around the match, for highlighting
  function parts(h: Hit) {
    const chars = [...h.text];
    return [chars.slice(0, h.start).join(""), chars.slice(h.start, h.end).join(""), chars.slice(h.end).join("")];
  }
</script>

<section class="search">
  <header>
    <h1>Search</h1>
    {#if links.length > 1}
      <select value={current} onchange={(e) => (linkId = e.currentTarget.value)} aria-label="Project">
        {#each links as l (l.id)}<option value={l.id}>{l.project_name}</option>{/each}
      </select>
    {/if}
  </header>

  {#if !current}
    <div class="empty">
      <Icon name="search" size={30} />
      <p>Search works on projects synced to a folder. Sync one from <strong>Projects</strong> first.</p>
    </div>
  {:else}
    <div class="box">
      <Icon name="search" size={16} />
      <input bind:this={input} bind:value={text} placeholder={"\\cite{knuth"} spellcheck="false" aria-label="Search the project" />
      <button class="opt" class:on={caseSensitive} title="Match case" aria-pressed={caseSensitive} onclick={() => (caseSensitive = !caseSensitive)}>Aa</button>
      <button class="opt" class:on={wholeWord} title="Whole word" aria-pressed={wholeWord} onclick={() => (wholeWord = !wholeWord)}>ab</button>
      <button class="opt" class:on={regex} title="Regular expression" aria-pressed={regex} onclick={() => (regex = !regex)}>.*</button>
    </div>

    {#if error}
      <p class="error">{error}</p>
    {:else if results}
      <p class="muted">
        {results.hits.length}{results.truncated ? "+" : ""} result{results.hits.length === 1 ? "" : "s"} in {results.files} file{results.files === 1 ? "" : "s"}
        {#if busy}· searching…{/if}
      </p>
      <div class="results">
        {#each groups as [path, hits] (path)}
          <div class="file">
            <div class="fname"><code>{path}</code> <span class="muted">{hits.length}</span></div>
            {#each hits as h (h.line)}
              {@const [a, m, b] = parts(h)}
              <button class="hit" onclick={() => open(h)}>
                <span class="ln">{h.line}</span>
                <span class="tx">{a.trimStart()}<mark>{m}</mark>{b}</span>
              </button>
            {/each}
          </div>
        {/each}
      </div>
    {:else}
      <p class="muted">Searches every text file in the project's folder, the way sync sees it.</p>
    {/if}
  {/if}
</section>

<style>
  .search {
    max-width: 900px;
    margin: 0 auto;
    padding: 28px 32px 48px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 14px;
  }
  h1 {
    font-size: 22px;
    margin: 0;
  }
  select {
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
  }
  .box {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 6px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
    color: var(--text-faint);
  }
  .box:focus-within {
    border-color: var(--accent);
  }
  .box input {
    flex: 1;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 13.5px;
  }
  .opt {
    min-width: 30px;
    height: 26px;
    border: 1px solid transparent;
    border-radius: 5px;
    background: transparent;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 12px;
    cursor: pointer;
  }
  .opt.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .muted {
    color: var(--text-faint);
    font-size: 12.5px;
  }
  .error {
    color: var(--danger);
    font-size: 13px;
  }
  .results {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .fname {
    padding: 4px 0;
    font-size: 12.5px;
  }
  .hit {
    width: 100%;
    display: flex;
    gap: 12px;
    padding: 3px 8px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text);
    text-align: left;
    font-family: var(--font-mono);
    font-size: 12.5px;
    cursor: pointer;
  }
  .hit:hover {
    background: var(--hover);
  }
  .ln {
    flex: none;
    width: 40px;
    text-align: right;
    color: var(--text-faint);
  }
  .tx {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  mark {
    background: rgba(214, 150, 40, 0.3);
    color: inherit;
    border-radius: 2px;
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
