<script lang="ts">
  import { initials } from "../accounts";
  import { VIEWS } from "../commands";
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";

  const shown = $derived(VIEWS.filter((v) => !v.full || !app.lite));
</script>

<nav class="bar" aria-label="Views">
  <div class="logo" title="Canopy"><Icon name="leaf" size={22} /></div>
  {#each shown as v (v.view)}
    <button
      class:active={app.view === v.view}
      title={`${v.title} (${v.keys})`}
      aria-label={v.title}
      aria-current={app.view === v.view ? "page" : undefined}
      onclick={() => (app.view = v.view)}
    >
      <Icon name={v.icon} />
    </button>
  {/each}
  {#if app.accounts.length > 0}
    <div class="sep"></div>
    {#each app.accounts as a (a.id)}
      {@const page = app.pages[a.id]}
      <button
        class="acct"
        class:active={app.view === "web" && app.activeAccount === a.id}
        title={a.label}
        aria-label={`Open ${a.label}`}
        onclick={() => app.openAccount(a.id)}
      >
        {initials(a.label)}
        {#if page?.signedIn === true}<span class="badge on"></span>{:else if page?.signedIn === false}<span class="badge off"></span>{/if}
      </button>
    {/each}
  {/if}
  <div class="spacer"></div>
  <button title="Command palette (Ctrl+Shift+P)" aria-label="Command palette" onclick={() => (app.paletteOpen = true)}>
    <Icon name="command" />
  </button>
  <button
    class:active={app.view === "settings"}
    title="Settings (Ctrl+,)"
    aria-label="Settings"
    onclick={() => (app.view = "settings")}
  >
    <Icon name="settings" />
  </button>
</nav>

<style>
  .bar {
    width: 52px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 10px 0;
    background: var(--surface-sunken);
    border-right: 1px solid var(--border);
  }
  .logo {
    color: var(--accent);
    padding: 6px 0 12px;
  }
  .spacer {
    flex: 1;
  }
  .sep {
    width: 24px;
    height: 1px;
    margin: 6px 0;
    background: var(--border);
  }
  .acct {
    font-size: 11.5px;
    font-weight: 650;
    letter-spacing: 0.02em;
  }
  .badge {
    position: absolute;
    right: 5px;
    bottom: 5px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1.5px solid var(--surface-sunken);
  }
  .badge.on {
    background: var(--ok);
  }
  .badge.off {
    background: var(--danger);
  }
  button {
    position: relative;
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    transition:
      background 0.12s,
      color 0.12s;
  }
  button:hover {
    color: var(--text);
    background: var(--hover);
  }
  button.active {
    color: var(--accent);
    background: var(--accent-soft);
  }
  button.active::before {
    content: "";
    position: absolute;
    left: -7px;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
</style>
