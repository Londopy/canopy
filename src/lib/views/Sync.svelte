<script lang="ts">
  import { api, errText } from "../api";
  import Icon from "../components/Icon.svelte";
  import { app } from "../state.svelte";
  import { ago, stateLabel, syncAnyway, syncNow, unlink } from "../syncing";
  import type { SyncLink } from "../types";

  const links = $derived(app.config?.links ?? []);

  function accountLabel(link: SyncLink) {
    return app.accounts.find((a) => a.id === link.account_id)?.label ?? "Unknown account";
  }

  async function setAuto(link: SyncLink, on: boolean) {
    try {
      app.config = await api.setLinkAuto(link.id, on);
    } catch (e) {
      app.toast("error", errText(e));
    }
  }

  function history(link: SyncLink) {
    app.historyLink = link.id;
    app.view = "history";
  }
</script>

<section class="sync">
  <header>
    <h1>Sync</h1>
  </header>

  {#if links.length === 0}
    <div class="empty">
      <Icon name="sync" size={30} />
      <h2>Keep a project in a folder</h2>
      <p>
        Open <strong>Projects</strong>, open an account's project list, and choose <strong>Sync to a folder</strong> next
        to a project.
        Canopy keeps both sides in step, merges edits made on both, and keeps a snapshot of every sync.
      </p>
      <button class="primary" onclick={() => (app.view = "projects")}>Go to projects</button>
    </div>
  {:else}
    <ul>
      {#each links as link (link.id)}
        {@const s = app.sync[link.id]}
        {@const label = stateLabel(s)}
        <li>
          <div class="top">
            <div class="name">
              <strong>{link.project_name}</strong>
              <small>{accountLabel(link)} · <span class="path" title={link.folder}>{link.folder}</span></small>
            </div>
            <span class="badge {label.cls}">{label.text}</span>
          </div>

          <div class="detail">
            {#if s?.state === "error" && s.error}
              <p class="error">{s.error}</p>
              {#if s.massDelete}
                <button class="ghost small" onclick={() => syncAnyway(link)}>Review and sync anyway…</button>
              {/if}
            {:else if s?.summary}
              <p class="muted">{s.summary} · {ago(s.lastSync)}</p>
            {:else}
              <p class="muted">Not synced yet this session.</p>
            {/if}

            {#if s?.failed?.length}
              <details class="list">
                <summary>{s.failed.length} file{s.failed.length === 1 ? "" : "s"} Overleaf refused (tried again next sync)</summary>
                <ul>{#each s.failed as f (f)}<li><code>{f}</code></li>{/each}</ul>
              </details>
            {/if}
            {#if s?.unsyncable?.length}
              <details class="list">
                <summary>{s.unsyncable.length} file{s.unsyncable.length === 1 ? "" : "s"} can't sync and are left alone</summary>
                <ul>{#each s.unsyncable as f (f)}<li><code>{f}</code></li>{/each}</ul>
              </details>
            {/if}
            {#if s?.keptCopies?.length}
              <p class="muted">
                Kept your version of a changed file as
                {#each s.keptCopies as c, i (c)}{i ? ", " : ""}<code>{c}</code>{/each}.
              </p>
            {/if}

            {#if s?.conflicts?.length}
              <div class="conflicts">
                <p>
                  These files changed on both sides. Text files now have <code>&lt;&lt;&lt;&lt;&lt;&lt;&lt; your folder</code>
                  … <code>&gt;&gt;&gt;&gt;&gt;&gt;&gt; overleaf</code> markers: edit them to keep what you want, and the
                  next sync uploads the result. For other files, Overleaf's version goes in place and yours is kept next to it as <code>name.local-conflict.ext</code>.
                </p>
                <ul>
                  {#each s.conflicts as c (c.path)}
                    <li><code>{c.path}</code> <span class="muted">{c.kind === "binary" ? "kept both" : "has markers"}</span></li>
                  {/each}
                </ul>
              </div>
            {/if}
          </div>

          <div class="actions">
            <button class="primary small" disabled={s?.state === "syncing"} onclick={() => syncNow(link)}>
              <Icon name="sync" size={14} /> Sync now
            </button>
            <button class="ghost small" onclick={() => api.openLinkFolder(link.id).catch((e) => app.toast("error", errText(e)))}>
              <Icon name="folder" size={14} /> Open folder
            </button>
            <button class="ghost small" onclick={() => history(link)}>
              <Icon name="history" size={14} /> Snapshots
            </button>
            <label class="auto">
              <input type="checkbox" class="switch" checked={link.auto_sync} onchange={(e) => setAuto(link, e.currentTarget.checked)} />
              Sync on its own
            </label>
            <button class="icon danger" title="Stop syncing" aria-label={`Stop syncing ${link.project_name}`} onclick={() => unlink(link)}>
              <Icon name="x" size={16} />
            </button>
          </div>
        </li>
      {/each}
    </ul>
    <p class="hint">
      With <strong>Sync on its own</strong>, Canopy syncs a few seconds after you save a file in the folder, and checks
      Overleaf for changes every few minutes. Build files like <code>.aux</code> and <code>.log</code> stay local.
    </p>
  {/if}
</section>

<style>
  .sync {
    max-width: 820px;
    margin: 0 auto;
    padding: 32px 32px 56px;
  }
  h1 {
    font-size: 22px;
    margin: 0 0 18px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  li {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
    padding: 14px 16px;
  }
  .top {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .name small {
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path {
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .badge {
    font-size: 12px;
    padding: 2px 9px;
    border-radius: 999px;
    background: var(--surface-sunken);
    color: var(--text-muted);
    white-space: nowrap;
  }
  .badge.ok {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .badge.busy {
    color: var(--accent);
  }
  .badge.warn {
    background: rgba(214, 150, 40, 0.15);
    color: #b9791b;
  }
  .badge.bad {
    background: rgba(194, 69, 61, 0.12);
    color: var(--danger);
  }
  .detail p {
    margin: 8px 0 0;
    font-size: 13px;
  }
  .muted {
    color: var(--text-muted);
  }
  .error {
    color: var(--danger);
  }
  .conflicts {
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: rgba(214, 150, 40, 0.08);
    font-size: 12.5px;
  }
  .conflicts p {
    margin: 0 0 6px;
    line-height: 1.5;
  }
  .conflicts li {
    border: 0;
    padding: 2px 0;
    background: transparent;
  }
  .list {
    margin-top: 8px;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .list summary {
    cursor: pointer;
  }
  .list ul {
    margin: 6px 0 0;
    padding-left: 18px;
    gap: 2px;
  }
  .list li {
    border: 0;
    padding: 0;
    background: transparent;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 12px;
  }
  button.small {
    padding: 5px 11px;
    font-size: 12.5px;
  }
  .auto {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    margin-left: auto;
    font-size: 12.5px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .icon {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }
  .icon.danger:hover {
    color: var(--danger);
    background: var(--hover);
  }
  .empty {
    text-align: center;
    padding: 8vh 24px;
    color: var(--accent);
  }
  .empty h2 {
    color: var(--text);
    font-size: 17px;
    margin: 12px 0 6px;
  }
  .empty p {
    color: var(--text-muted);
    max-width: 460px;
    margin: 0 auto 16px;
    line-height: 1.55;
  }
  .hint {
    color: var(--text-muted);
    font-size: 12.5px;
    margin-top: 16px;
    line-height: 1.5;
  }
</style>
