<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app } from "../state.svelte";
  import { installUpdate, skipUpdate } from "../updates";
  import Icon from "./Icon.svelte";

  let notesOpen = $state(false);
  let installing = $state(false);

  async function install() {
    installing = true;
    await installUpdate();
    installing = false;
  }
</script>

{#if app.update}
  <div class="banner" role="status">
    <Icon name="download" size={16} />
    <span class="text">
      <strong>Canopy v{app.update.version}</strong> is available. You have v{app.update.current}.
    </span>
    {#if app.update.notes}
      <button class="ghost" onclick={() => (notesOpen = !notesOpen)}>What's new</button>
    {/if}
    <button class="ghost" onclick={skipUpdate}>Later</button>
    {#if app.info?.can_self_update}
      <button class="primary" disabled={installing} onclick={install}>
        {installing ? "Installing…" : "Install and restart"}
      </button>
    {:else}
      <!-- portable copies and dev builds can't replace themselves -->
      <button class="primary" onclick={() => openUrl(`${app.info?.homepage}/releases/latest`)}>Download</button>
    {/if}
  </div>
  {#if notesOpen && app.update.notes}
    <pre class="notes">{app.update.notes}</pre>
  {/if}
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    background: var(--accent-soft);
    border-bottom: 1px solid var(--border);
    color: var(--text);
  }
  .text {
    flex: 1;
  }
  .notes {
    margin: 0;
    max-height: 40vh;
    overflow: auto;
    padding: 12px 16px;
    white-space: pre-wrap;
    font-family: var(--font-mono);
    font-size: 12.5px;
    background: var(--surface-raised);
    border-bottom: 1px solid var(--border);
  }
</style>
