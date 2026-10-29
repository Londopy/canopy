<script lang="ts">
  import { api, errText } from "../api";
  import { app } from "../state.svelte";
  import Icon from "./Icon.svelte";

  // add: pick overleaf.com or a self-hosted server; rename: just the name
  const dialog = $derived(app.accountDialog);
  let kind = $state<"cloud" | "self">("cloud");
  let server = $state("");
  let label = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);
  let first: HTMLInputElement | undefined = $state();

  // a fresh form every time the dialog opens
  $effect(() => {
    if (!dialog) return;
    label = dialog.mode === "rename" ? dialog.account.label : "";
    server = "";
    kind = "cloud";
    error = null;
  });

  $effect(() => {
    // focus the first field once it's drawn
    void kind;
    first?.focus();
  });

  function close() {
    app.accountDialog = null;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!dialog || busy) return;
    busy = true;
    error = null;
    try {
      if (dialog.mode === "add") {
        const before = new Set(app.accounts.map((a) => a.id));
        app.config = await api.addAccount(label, kind === "cloud" ? "overleaf.com" : server);
        const added = app.accounts.find((a) => !before.has(a.id));
        close();
        // straight to its page, where the sign-in happens
        if (added) app.openAccount(added.id);
      } else {
        app.config = await api.renameAccount(dialog.account.id, label);
        close();
      }
    } catch (err) {
      const text = errText(err);
      error = text.charAt(0).toUpperCase() + text.slice(1);
    } finally {
      busy = false;
    }
  }
</script>

{#if dialog}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onclick={close} onkeydown={(e) => e.key === "Escape" && close()}>
    <div role="dialog" aria-modal="true" aria-labelledby="account-dialog-title" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <form class="dialog" onsubmit={submit}>
      <h2 id="account-dialog-title">{dialog.mode === "add" ? "Add an Overleaf account" : "Rename account"}</h2>

      {#if dialog.mode === "add"}
        <div class="choices" role="radiogroup" aria-label="Server">
          <button type="button" role="radio" aria-checked={kind === "cloud"} class:on={kind === "cloud"} onclick={() => (kind = "cloud")}>
            <Icon name="cloud" size={18} />
            <span><strong>overleaf.com</strong><small>The hosted service</small></span>
          </button>
          <button type="button" role="radio" aria-checked={kind === "self"} class:on={kind === "self"} onclick={() => (kind = "self")}>
            <Icon name="folder" size={18} />
            <span><strong>Self-hosted</strong><small>Community Edition or Server Pro</small></span>
          </button>
        </div>

        {#if kind === "self"}
          <label>
            Server address
            <input bind:this={first} bind:value={server} placeholder="overleaf.example.org" spellcheck="false" autocomplete="off" required />
            <small>https is added for you. Plain http only works for this computer (localhost).</small>
          </label>
        {/if}
      {/if}

      <label>
        <span>Name {#if dialog.mode === "add"}<span class="optional">(optional)</span>{/if}</span>
        {#if dialog.mode === "rename" || kind === "cloud"}
          <input bind:this={first} bind:value={label} maxlength="60" placeholder={kind === "cloud" ? "Overleaf" : ""} />
        {:else}
          <input bind:value={label} maxlength="60" placeholder="The server's address" />
        {/if}
      </label>

      {#if dialog.mode === "add"}
        <p class="hint">
          Next you sign in on {kind === "cloud" ? "Overleaf's" : "your server's"} own page. Canopy never sees your
          password, and each account keeps its own sign-in.
        </p>
      {/if}

      {#if error}<p class="error" role="alert">{error}</p>{/if}

      <div class="buttons">
        <button type="button" class="ghost" onclick={close}>Cancel</button>
        <button type="submit" class="primary" disabled={busy}>
          {dialog.mode === "add" ? (busy ? "Adding…" : "Add and sign in") : "Save"}
        </button>
      </div>
    </form>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: var(--scrim);
    display: grid;
    place-items: center;
    z-index: 55;
  }
  .dialog {
    width: min(480px, 92vw);
    padding: 20px 22px;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
  }
  .choices {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .choices button {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    text-align: left;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text-muted);
    cursor: pointer;
  }
  .choices button.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .choices span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .choices strong {
    color: var(--text);
  }
  small,
  .hint,
  .optional {
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 400;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-weight: 550;
  }
  input {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-weight: 400;
  }
  .hint {
    margin: 0;
    line-height: 1.5;
  }
  .error {
    margin: 0;
    color: var(--danger);
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
