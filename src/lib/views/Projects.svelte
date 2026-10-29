<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import { app } from "../state.svelte";

  const steps = [
    { icon: "cloud", title: "Sign in to Overleaf", text: "overleaf.com or your own self-hosted server. You sign in on Overleaf's own page." },
    { icon: "sync", title: "Link a folder", text: "Mirror any project to disk and edit it with whatever you like." },
    { icon: "feather", title: "Compile locally", text: "TeX Live, MiKTeX or Tectonic. No timeouts, works offline." },
  ];
</script>

<section class="welcome">
  <div class="hero">
    <div class="mark"><Icon name="leaf" size={40} /></div>
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
    <button class="primary" disabled title="Accounts arrive in the next milestone">
      <Icon name="plus" size={16} /> Add an Overleaf account
    </button>
    <button class="ghost" onclick={() => (app.paletteOpen = true)}>
      <Icon name="command" size={16} /> Command palette <kbd>Ctrl+Shift+P</kbd>
    </button>
  </div>
</section>

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
    width: 72px;
    height: 72px;
    margin: 0 auto 18px;
    display: grid;
    place-items: center;
    border-radius: 20px;
    color: var(--accent);
    background: var(--accent-soft);
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
</style>
