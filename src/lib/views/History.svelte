<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api, errText } from "../api";
  import Icon from "../components/Icon.svelte";
  import { app } from "../state.svelte";
  import { when } from "../syncing";
  import type { Change, Snapshot } from "../types";

  const links = $derived(app.config?.links ?? []);
  const linkId = $derived(app.historyLink ?? links[0]?.id ?? null);
  const link = $derived(links.find((l) => l.id === linkId) ?? null);

  let snapshots = $state<Snapshot[]>([]);
  let selected = $state<Snapshot | null>(null);
  let changes = $state<Change[]>([]);
  let file = $state<string | null>(null);
  let diff = $state("");
  let loading = $state(false);

  // reload whenever the project changes, or a sync lands for it
  $effect(() => {
    const id = linkId;
    void app.sync[id ?? ""]?.lastSync;
    if (!id) return;
    api
      .linkHistory(id)
      .then((list) => {
        snapshots = list;
        if (!selected || !list.some((s) => s.id === selected?.id)) void pick(list[0] ?? null);
      })
      .catch((e) => app.toast("error", errText(e)));
  });

  async function pick(s: Snapshot | null) {
    selected = s;
    changes = [];
    file = null;
    diff = "";
    if (!s || !linkId) return;
    loading = true;
    try {
      changes = await api.snapshotChanges(linkId, s.id);
      if (changes[0]) await show(changes[0].path);
    } catch (e) {
      app.toast("error", errText(e));
    } finally {
      loading = false;
    }
  }

  async function show(path: string) {
    if (!selected || !linkId) return;
    file = path;
    try {
      diff = await api.snapshotDiff(linkId, selected.id, path);
    } catch (e) {
      diff = errText(e);
    }
  }

  async function restore() {
    if (!selected || !link) return;
    const yes = await ask(
      `Put "${link.project_name}" back to how it was on ${when(selected.time)}? Canopy first saves your folder as it is now (unsynced edits included) as a "Before restore" snapshot, then rewrites the folder and syncs it to Overleaf. Files added since then are deleted on both sides, but stay in that snapshot.`,
      { title: "Restore snapshot", kind: "warning", okLabel: "Restore", cancelLabel: "Cancel" },
    );
    if (!yes) return;
    try {
      const done = await api.restoreSnapshot(link.id, selected.id);
      app.toast(
        "ok",
        `Restored ${link.project_name}: ${done.changed} file${done.changed === 1 ? "" : "s"} changed. The version before is the "Before restore" snapshot.`,
      );
    } catch (e) {
      app.toast("error", `Restore failed: ${errText(e)}`);
    }
  }

  function lineClass(line: string) {
    if (line.startsWith("+++") || line.startsWith("---")) return "meta";
    if (line.startsWith("@@")) return "hunk";
    if (line.startsWith("+")) return "add";
    if (line.startsWith("-")) return "del";
    return "";
  }

  const KIND = { added: "A", modified: "M", deleted: "D" } as const;
</script>

<section class="history">
  <header>
    <h1>Snapshots</h1>
    {#if links.length > 1}
      <select value={linkId} onchange={(e) => (app.historyLink = e.currentTarget.value)} aria-label="Project">
        {#each links as l (l.id)}<option value={l.id}>{l.project_name}</option>{/each}
      </select>
    {/if}
  </header>

  {#if !link}
    <div class="empty">
      <Icon name="history" size={30} />
      <p>Snapshots appear here once a project syncs to a folder. Every sync keeps one.</p>
    </div>
  {:else if snapshots.length === 0}
    <p class="muted">No snapshots of {link.project_name} yet. The first sync makes one.</p>
  {:else}
    <div class="panes">
      <ol class="list">
        {#each snapshots as s (s.id)}
          <li>
            <button class:on={selected?.id === s.id} onclick={() => pick(s)}>
              <span class="when">{when(s.time)}</span>
              <span class="msg">{s.message}</span>
            </button>
          </li>
        {/each}
      </ol>

      <div class="detail">
        {#if selected}
          <div class="head">
            <div>
              <strong>{when(selected.time)}</strong>
              <span class="muted">{selected.message}</span>
            </div>
            <button class="ghost small" onclick={restore}>Restore this snapshot</button>
          </div>
          {#if loading}
            <p class="muted">Loading…</p>
          {:else if changes.length === 0}
            <p class="muted">The first snapshot: the project as it was first synced.</p>
          {:else}
            <div class="files">
              {#each changes as c (c.path)}
                <button class="file" class:on={file === c.path} onclick={() => show(c.path)}>
                  <span class="kind {c.kind}">{KIND[c.kind]}</span>{c.path}
                </button>
              {/each}
            </div>
            {#if file}
              <pre class="diff">{#each diff.split("\n") as line, i (i)}<span class={lineClass(line)}>{line}
</span>{/each}</pre>
            {/if}
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .history {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 24px 28px;
    min-height: 0;
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
  .panes {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(220px, 300px) 1fr;
    gap: 14px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 4px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
  }
  .list button {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .list button:hover {
    background: var(--hover);
  }
  .list button.on {
    background: var(--accent-soft);
  }
  .when {
    font-size: 12.5px;
    font-weight: 550;
  }
  .msg {
    font-size: 12px;
    color: var(--text-muted);
  }
  .detail {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .head div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .files {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .file {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-raised);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: 12px;
    cursor: pointer;
  }
  .file.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .kind {
    font-weight: 700;
    font-size: 10.5px;
  }
  .kind.added {
    color: var(--ok);
  }
  .kind.modified {
    color: #b9791b;
  }
  .kind.deleted {
    color: var(--danger);
  }
  .diff {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin: 0;
    padding: 10px 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .diff span {
    display: block;
    padding: 0 12px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .diff .add {
    background: rgba(47, 143, 91, 0.12);
  }
  .diff .del {
    background: rgba(194, 69, 61, 0.12);
  }
  .diff .hunk {
    color: var(--accent);
  }
  .diff .meta {
    color: var(--text-faint);
  }
  .muted {
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .empty {
    text-align: center;
    color: var(--accent);
    padding: 10vh 24px;
  }
  .empty p {
    color: var(--text-muted);
  }
  button.small {
    padding: 5px 11px;
    font-size: 12.5px;
  }
</style>
