<script lang="ts">
  import logo from "../../assets/canopy.svg";
  import { initials, removeAccount, serverLabel } from "../accounts";
  import { api, errText } from "../api";
  import Icon from "../components/Icon.svelte";
  import { app } from "../state.svelte";
  import { linkProject, stateLabel } from "../syncing";
  import type { Account, ProjectSummary } from "../types";

  // each account's project list, fetched when its row is opened
  let open = $state<Record<string, { loading: boolean; error: string | null; items: ProjectSummary[] }>>({});

  async function toggle(a: Account) {
    if (open[a.id]) {
      delete open[a.id];
      return;
    }
    open[a.id] = { loading: true, error: null, items: [] };
    try {
      const items = await api.listProjects(a.id);
      open[a.id] = { loading: false, error: null, items };
    } catch (e) {
      open[a.id] = { loading: false, error: errText(e), items: [] };
    }
  }

  const steps = [
    { icon: "cloud", title: "Sign in to Overleaf", text: "overleaf.com or your own self-hosted server. You sign in on Overleaf's own page." },
    { icon: "sync", title: "Link a folder", text: "Mirror any project to disk and edit it with whatever you like." },
    { icon: "feather", title: "Compile locally", text: "TeX Live, MiKTeX or Tectonic. No timeouts, works offline." },
  ];

  function stateOf(id: string) {
    const p = app.pages[id];
    if (!p) return { cls: "", text: "Not opened yet" };
    if (p.signedIn === false) return { cls: "off", text: "Signed out" };
    if (p.signedIn) return { cls: "on", text: "Signed in" };
    return { cls: "", text: "Signing in…" };
  }
</script>

{#if app.accounts.length === 0}
  <section class="welcome">
    <div class="hero">
      <img class="mark" src={logo} alt="" width="72" height="72" />
      <h1>Welcome to Canopy</h1>
      <p class="lede">The real Overleaf editor, plus everything the browser can't do.</p>
    </div>

    <div class="steps">
      {#each steps as s, i (s.title)}
        <div class="step">
          <div class="num">{i + 1}</div>
          <Icon name={s.icon} size={22} />
          <h3>{s.title}</h3>
          <p>{s.text}</p>
        </div>
      {/each}
    </div>

    <div class="actions">
      <button class="primary" onclick={() => (app.accountDialog = { mode: "add" })}>
        <Icon name="plus" size={16} /> Add an Overleaf account
      </button>
      <button class="ghost" onclick={() => (app.paletteOpen = true)}>
        <Icon name="command" size={16} /> Command palette <kbd>Ctrl+Shift+P</kbd>
      </button>
    </div>
  </section>
{:else}
  <section class="accounts">
    <header>
      <h1>Accounts</h1>
      <button class="primary" onclick={() => (app.accountDialog = { mode: "add" })}>
        <Icon name="plus" size={16} /> Add account
      </button>
    </header>

    <ul>
      {#each app.accounts as a (a.id)}
        {@const s = stateOf(a.id)}
        <li>
          <button class="open" onclick={() => app.openAccount(a.id)}>
            <span class="avatar">{initials(a.label)}</span>
            <span class="text">
              <strong>{a.label}</strong>
              <small>{serverLabel(a.server)}</small>
            </span>
            <span class="state {s.cls}"><span class="dot"></span>{s.text}</span>
          </button>
          {#if !app.lite}
            <button
              class="icon"
              title="Projects"
              aria-label={`Projects on ${a.label}`}
              aria-expanded={!!open[a.id]}
              onclick={() => toggle(a)}
            >
              <Icon name="folder" size={16} />
            </button>
          {/if}
          <button class="icon" title="Rename" aria-label={`Rename ${a.label}`} onclick={() => (app.accountDialog = { mode: "rename", account: a })}>
            <Icon name="pencil" size={16} />
          </button>
          <button class="icon danger" title="Remove" aria-label={`Remove ${a.label}`} onclick={() => removeAccount(a)}>
            <Icon name="trash" size={16} />
          </button>
        </li>
        {#if open[a.id]}
          {@const list = open[a.id]}
          <li class="projects">
            {#if list.loading}
              <p class="muted">Loading projects…</p>
            {:else if list.error}
              <p class="error">{list.error}</p>
              <button class="ghost" onclick={() => app.openAccount(a.id)}>Open {a.label} to sign in</button>
            {:else if list.items.length === 0}
              <p class="muted">No projects on this account yet.</p>
            {:else}
              {#each list.items as p (p._id)}
                {@const link = app.config?.links.find((l) => l.account_id === a.id && l.project_id === p._id)}
                <div class="project">
                  <span class="pname">{p.name}</span>
                  {#if link}
                    <span class="linked" title={link.folder}>
                      <Icon name="sync" size={13} /> {stateLabel(app.sync[link.id]).text}
                    </span>
                    <button class="ghost small" onclick={() => (app.view = "sync")}>Sync settings</button>
                  {:else if p.accessLevel === "readOnly" || p.accessLevel === "review"}
                    <span class="muted small-text">Read-only</span>
                  {:else}
                    <button class="ghost small" onclick={() => linkProject(a, p)}>Sync to a folder…</button>
                  {/if}
                </div>
              {/each}
            {/if}
          </li>
        {/if}
      {/each}
    </ul>

    {#if app.lite}
      <p class="hint">Projects open on each account's own Overleaf dashboard. Folder sync is off in Lite mode.</p>
    {/if}
  </section>
{/if}

<style>
  .welcome {
    max-width: 860px;
    margin: 0 auto;
    padding: 9vh 32px 48px;
  }
  .hero {
    text-align: center;
  }
  .mark {
    display: block;
    margin: 0 auto 18px;
  }
  h1 {
    margin: 0 0 8px;
    font-size: 28px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .lede {
    margin: 0;
    color: var(--text-muted);
    font-size: 15px;
  }
  .steps {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 14px;
    margin: 40px 0 28px;
  }
  .step {
    position: relative;
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
    color: var(--accent);
  }
  .num {
    position: absolute;
    top: 14px;
    right: 16px;
    font-size: 12px;
    color: var(--text-faint);
  }
  h3 {
    margin: 10px 0 6px;
    font-size: 14px;
    color: var(--text);
  }
  .step p {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    gap: 10px;
    justify-content: center;
    flex-wrap: wrap;
  }

  .accounts {
    max-width: 760px;
    margin: 0 auto;
    padding: 32px 32px 56px;
  }
  .accounts header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 18px;
  }
  .accounts h1 {
    font-size: 22px;
    margin: 0;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px 4px 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
  }
  .open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .open:hover {
    background: var(--hover);
  }
  .avatar {
    width: 36px;
    height: 36px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 650;
    font-size: 13px;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .text small {
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-faint);
    font-size: 12px;
    white-space: nowrap;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
  }
  .state.on {
    color: var(--ok);
  }
  .state.off {
    color: var(--danger);
  }
  .icon {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }
  .icon:hover {
    background: var(--hover);
    color: var(--text);
  }
  .icon.danger:hover {
    color: var(--danger);
  }
  .hint {
    color: var(--text-muted);
    font-size: 12.5px;
    margin-top: 16px;
  }
  li.projects {
    display: block;
    margin: -4px 0 4px 24px;
    padding: 8px 12px;
  }
  .project {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border-soft);
  }
  .project:last-child {
    border-bottom: 0;
  }
  .pname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .linked {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--accent);
    font-size: 12px;
  }
  .muted,
  .small-text {
    color: var(--text-faint);
    font-size: 12.5px;
    margin: 4px 0;
  }
  .error {
    color: var(--danger);
    font-size: 12.5px;
    margin: 4px 0 8px;
  }
  button.small {
    padding: 4px 10px;
    font-size: 12.5px;
  }
</style>
