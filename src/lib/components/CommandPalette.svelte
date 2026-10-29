<script lang="ts">
  import { commands, fuzzyScore, type Command } from "../commands";
  import { app } from "../state.svelte";

  let query = $state("");
  let active = $state(0);
  let input: HTMLInputElement | undefined = $state();

  const results = $derived.by(() => {
    const all = commands();
    if (!query.trim()) return all;
    return all
      .map((c) => ({ c, s: fuzzyScore(query, c.title) }))
      .filter((r) => r.s >= 0)
      .sort((a, b) => b.s - a.s)
      .map((r) => r.c);
  });

  $effect(() => {
    // reset the highlight whenever the list changes under it
    void results;
    active = 0;
  });

  $effect(() => {
    input?.focus();
  });

  function close() {
    app.paletteOpen = false;
  }

  async function run(c: Command | undefined) {
    if (!c) return;
    close();
    await c.run();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      active = Math.min(active + 1, results.length - 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      active = Math.max(active - 1, 0);
    } else if (e.key === "Enter") {
      e.preventDefault();
      void run(results[active]);
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={close}>
  <div class="palette" role="dialog" aria-label="Command palette" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <input
      bind:this={input}
      bind:value={query}
      {onkeydown}
      placeholder="Type a command"
      spellcheck="false"
      aria-controls="palette-list"
    />
    <ul id="palette-list" role="listbox">
      {#each results as c, i (c.id)}
        <li role="option" aria-selected={i === active}>
          <button class:active={i === active} onmouseenter={() => (active = i)} onclick={() => run(c)}>
            <span class="group">{c.group}</span>
            <span class="title">{c.title}</span>
            {#if c.keys}<kbd>{c.keys}</kbd>{/if}
          </button>
        </li>
      {:else}
        <li class="empty">No matching commands</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: var(--scrim);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    z-index: 50;
    animation: fade 0.12s ease-out;
  }
  .palette {
    width: min(620px, 92vw);
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    animation: rise 0.14s ease-out;
  }
  input {
    width: 100%;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: transparent;
    padding: 14px 16px;
    font-size: 15px;
    color: var(--text);
    outline: none;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 6px;
    max-height: 50vh;
    overflow-y: auto;
  }
  button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  button.active {
    background: var(--accent-soft);
  }
  .group {
    color: var(--text-faint);
    font-size: 12px;
    min-width: 78px;
  }
  .title {
    flex: 1;
  }
  .empty {
    padding: 12px;
    color: var(--text-faint);
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      transform: translateY(-6px);
      opacity: 0;
    }
  }
</style>
