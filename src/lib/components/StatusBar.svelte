<script lang="ts">
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { engine }: { engine: string } = $props();

  const accounts = $derived(app.config?.accounts.length ?? 0);
  const latest = $derived(app.toasts[app.toasts.length - 1]);
  const statuses = $derived(Object.values(app.sync));
  const syncing = $derived(statuses.filter((s) => s.state === "syncing").length);
  const trouble = $derived(statuses.filter((s) => s.state === "error" || s.state === "conflict").length);
</script>

<footer class="status">
  <span class="item" title="Signed-in Overleaf accounts">
    <Icon name="cloud" size={14} />
    {accounts === 0 ? "No accounts" : `${accounts} account${accounts === 1 ? "" : "s"}`}
  </span>
  {#if !app.lite}
    <span class="item" title="Local compiler">
      <Icon name="feather" size={14} />
      {engine}
    </span>
  {/if}
  {#if syncing > 0}
    <button class="item link" onclick={() => (app.view = "sync")}><Icon name="sync" size={14} /> Syncing {syncing}…</button>
  {:else if trouble > 0}
    <button class="item link warn" onclick={() => (app.view = "sync")}><Icon name="alert" size={14} /> {trouble} sync issue{trouble === 1 ? "" : "s"}</button>
  {/if}
  {#if app.lite}<span class="item pill">Lite</span>{/if}
  {#if app.view === "web" && latest}
    <span class="item toast {latest.kind}" role="status">{latest.text}</span>
  {/if}
  <span class="spacer"></span>
  {#if app.update}
    <button class="item link" onclick={() => (app.view = "settings")}>
      <Icon name="download" size={14} /> v{app.update.version} available
    </button>
  {/if}
  <button class="item link" onclick={() => (app.paletteOpen = true)}>Ctrl+Shift+P</button>
  <span class="item">v{app.info?.version}</span>
</footer>

<style>
  .status {
    height: 26px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px;
    font-size: 12px;
    color: var(--text-faint);
    background: var(--surface-sunken);
    border-top: 1px solid var(--border);
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 8px;
    height: 100%;
  }
  .spacer {
    flex: 1;
  }
  .pill {
    color: var(--accent);
    font-weight: 600;
  }
  .toast {
    color: var(--text);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toast.ok {
    color: var(--ok);
  }
  .toast.error {
    color: var(--danger);
  }
  .link {
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .link.warn {
    color: var(--danger);
  }
  .link:hover {
    color: var(--text);
    background: var(--hover);
  }
</style>
