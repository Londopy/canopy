<script lang="ts">
  import { onDestroy } from "svelte";
  import { serverLabel } from "../accounts";
  import { api, errText } from "../api";
  import Icon from "../components/Icon.svelte";
  import { app } from "../state.svelte";
  import type { Rect, WebAction } from "../types";

  // the real Overleaf page is a native webview laid over `host`; this
  // component only keeps its position in step with the layout
  let host: HTMLDivElement | undefined = $state();

  const account = $derived(app.account);
  const page = $derived(account ? app.pages[account.id] : undefined);

  function rect(): Rect | null {
    if (!host) return null;
    const r = host.getBoundingClientRect();
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }

  // show the page, or step it aside while one of our overlays is up
  $effect(() => {
    const id = account?.id;
    const hidden = app.overlayOpen;
    const r = rect();
    if (!id || !r) return;
    const call = hidden ? api.webHide() : api.webShow(id, r);
    call.catch((e) => app.toast("error", `Couldn't open the page: ${errText(e)}`));
  });

  // keep it glued to the box through window resizes and layout changes
  $effect(() => {
    if (!host) return;
    const observer = new ResizeObserver(() => {
      const r = rect();
      if (r && !app.overlayOpen) void api.webBounds(r);
    });
    observer.observe(host);
    return () => observer.disconnect();
  });

  onDestroy(() => {
    void api.webHide();
  });

  function act(action: WebAction) {
    if (!account) return;
    api.webAction(account.id, action).catch((e) => app.toast("error", errText(e)));
  }

  const status = $derived(
    page?.loading
      ? "Loading…"
      : page?.signedIn === false
        ? "Signed out"
        : page?.signedIn
          ? "Signed in"
          : page
            ? "Signing in…"
            : "Opening…",
  );
</script>

{#if account}
  <section class="web">
    <header class="bar">
      <button class="icon" title="Back" aria-label="Back" onclick={() => act("back")}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 6l-6 6 6 6" /></svg>
      </button>
      <button class="icon" title="Forward" aria-label="Forward" onclick={() => act("forward")}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg>
      </button>
      <button class="icon" title="Reload" aria-label="Reload" onclick={() => act("reload")}>
        <Icon name="sync" size={16} />
      </button>
      <button class="icon" title="All projects" aria-label="All projects" onclick={() => act("home")}>
        <Icon name="folder" size={16} />
      </button>

      <div class="where" title={page?.url ?? ""}>
        <span class="dot" class:on={page?.signedIn === true} class:off={page?.signedIn === false}></span>
        <strong>{account.label}</strong>
        <span class="muted">{page?.title || serverLabel(account.server)}</span>
      </div>

      <span class="status">{status}</span>
      <button class="icon" title="Open this page in your browser" aria-label="Open in browser" onclick={() => act("open-in-browser")}>
        <Icon name="external" size={16} />
      </button>
    </header>
    <div class="host" bind:this={host}>
      <!-- shown only for the moment before the native page paints over it -->
      <div class="placeholder"><Icon name="leaf" size={28} /></div>
    </div>
  </section>
{/if}

<style>
  .web {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .bar {
    flex: none;
    height: 40px;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-raised);
  }
  .icon {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .icon:hover {
    background: var(--hover);
    color: var(--text);
  }
  .where {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: 8px;
    white-space: nowrap;
    overflow: hidden;
  }
  .where .muted {
    color: var(--text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
    background: var(--text-faint);
  }
  .dot.on {
    background: var(--ok);
  }
  .dot.off {
    background: var(--danger);
  }
  .status {
    color: var(--text-faint);
    font-size: 12px;
    margin: 0 6px;
  }
  .host {
    flex: 1;
    position: relative;
    min-height: 0;
  }
  .placeholder {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--text-faint);
  }
</style>
