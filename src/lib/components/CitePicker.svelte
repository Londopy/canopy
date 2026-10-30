<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { errText } from "../api";
  import { app } from "../state.svelte";

  interface Item {
    key: string;
    citekey: string | null;
    title: string;
    creators: string;
    year: string;
  }

  // searches the Zotero library on this computer and adds the picked entries
  // to one of the project's .bib files; `oninsert` gets the citation keys
  let {
    linkId,
    bibFiles,
    oninsert,
    onclose,
  }: {
    linkId: string;
    bibFiles: string[];
    oninsert: (keys: string[]) => void;
    onclose: () => void;
  } = $props();

  let query = $state("");
  let items = $state<Item[]>([]);
  let picked = $state<Record<string, boolean>>({});
  let bib = $state("");
  // the first .bib file of the project, until the user picks one
  $effect(() => {
    if (!bib) bib = bibFiles[0] ?? "references.bib";
  });
  let error = $state<string | null>(null);
  let busy = $state(false);
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    input?.focus();
  });

  let token = 0;
  $effect(() => {
    const q = query.trim();
    const mine = ++token;
    if (q.length < 2) {
      items = [];
      return;
    }
    const t = setTimeout(async () => {
      try {
        const r = await invoke<Item[]>("zotero_search", { query: q });
        if (mine === token) {
          items = r;
          error = null;
        }
      } catch (e) {
        if (mine === token) error = errText(e);
      }
    }, 250);
    return () => clearTimeout(t);
  });

  const chosen = $derived(items.filter((i) => picked[i.key]));

  async function insert() {
    if (!chosen.length || busy) return;
    busy = true;
    try {
      const r = await invoke<{ keys: string[]; new: string[] }>("zotero_add", {
        linkId,
        bib,
        items: chosen.map((i) => ({ key: i.key, citekey: i.citekey })),
      });
      if (r.new.length) app.toast("ok", `Added ${r.new.length} entr${r.new.length === 1 ? "y" : "ies"} to ${bib}.`);
      oninsert(r.keys);
      onclose();
    } catch (e) {
      error = errText(e);
    } finally {
      busy = false;
    }
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void insert();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={onclose} onkeydown={onkey}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Insert a citation from Zotero" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <h2>Cite from Zotero</h2>
    <input bind:this={input} bind:value={query} placeholder="Author, title or year" spellcheck="false" />
    {#if error}<p class="error">{error}</p>{/if}
    <ul>
      {#each items as it (it.key)}
        <li>
          <label>
            <input type="checkbox" bind:checked={picked[it.key]} />
            <span class="t">{it.title || "(untitled)"}</span>
            <span class="m">{it.creators}{it.year ? ` · ${it.year}` : ""}{it.citekey ? ` · ${it.citekey}` : ""}</span>
          </label>
        </li>
      {:else}
        {#if query.trim().length >= 2 && !error}<li class="none">Nothing matches.</li>{/if}
      {/each}
    </ul>
    <div class="foot">
      <label class="bib">
        Add to
        <select bind:value={bib}>
          {#each bibFiles as f (f)}<option value={f}>{f}</option>{/each}
          {#if !bibFiles.includes("references.bib")}<option value="references.bib">references.bib (new)</option>{/if}
        </select>
      </label>
      <span class="spacer"></span>
      <button class="ghost" onclick={onclose}>Cancel</button>
      <button class="primary" disabled={!chosen.length || busy} onclick={insert}>
        {busy ? "Adding…" : chosen.length > 1 ? `Cite ${chosen.length}` : "Cite"}
      </button>
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: var(--scrim);
    display: grid;
    place-items: start center;
    padding-top: 12vh;
    z-index: 60;
  }
  .dialog {
    width: min(620px, 92vw);
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 18px 20px;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  input:not([type]) {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: auto;
    min-height: 60px;
  }
  li label {
    display: grid;
    grid-template-columns: 22px 1fr;
    gap: 2px 8px;
    padding: 6px 4px;
    border-radius: 6px;
    cursor: pointer;
  }
  li label:hover {
    background: var(--hover);
  }
  li input {
    grid-row: span 2;
    margin-top: 3px;
  }
  .t {
    font-size: 13px;
  }
  .m {
    font-size: 12px;
    color: var(--text-faint);
  }
  .none {
    color: var(--text-faint);
    font-size: 13px;
    padding: 6px;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .bib {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  select {
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
  }
  .spacer {
    flex: 1;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: 12.5px;
  }
</style>
