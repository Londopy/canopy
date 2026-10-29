<script lang="ts">
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";

  const ICON = { info: "leaf", ok: "check", error: "alert" } as const;
</script>

<div class="stack" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}">
      <Icon name={ICON[t.kind]} size={16} />
      <span>{t.text}</span>
      <button aria-label="Dismiss" onclick={() => app.dismiss(t.id)}><Icon name="x" size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    right: 16px;
    bottom: 40px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 60;
    max-width: min(420px, 90vw);
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent);
    border-radius: var(--radius);
    box-shadow: var(--shadow-lg);
    animation: slide 0.16s ease-out;
  }
  .toast.ok {
    border-left-color: var(--ok);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
  span {
    flex: 1;
    line-height: 1.4;
  }
  button {
    border: 0;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    padding: 2px;
  }
  @keyframes slide {
    from {
      transform: translateX(12px);
      opacity: 0;
    }
  }
</style>
