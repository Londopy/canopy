<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errText } from "../api";
  import { exportSettings, importSettings } from "../settingsio";
  import { app } from "../state.svelte";
  import type { EnginePref, TexReport, Theme } from "../types";
  import { checkForUpdates } from "../updates";

  let { tex, onrescan }: { tex: TexReport | null; onrescan: () => void } = $props();

  const themes: { value: Theme; label: string }[] = [
    { value: "system", label: "System" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ];
  const engines: { value: EnginePref; label: string }[] = [
    { value: "auto", label: "Automatic (latexmk, then Tectonic)" },
    { value: "latexmk", label: "latexmk (TeX Live / MiKTeX)" },
    { value: "tectonic", label: "Tectonic" },
    { value: "remote", label: "Always compile on Overleaf" },
  ];
  const toolNames = ["latexmk", "pdflatex", "tectonic", "texlab", "synctex"] as const;

  const cfg = $derived(app.config);
</script>

{#if cfg}
  <section class="settings">
    <h1>Settings</h1>

    <div class="card">
      <h2>Appearance</h2>
      <div class="row">
        <div>
          <div class="label">Theme</div>
          <div class="hint">System follows your OS light/dark setting.</div>
        </div>
        <div class="seg" role="radiogroup" aria-label="Theme">
          {#each themes as t (t.value)}
            <button
              role="radio"
              aria-checked={cfg.general.theme === t.value}
              class:on={cfg.general.theme === t.value}
              onclick={() => app.setTheme(t.value)}>{t.label}</button
            >
          {/each}
        </div>
      </div>
    </div>

    <div class="card">
      <h2>General</h2>
      <label class="row">
        <div>
          <div class="label">Lite mode</div>
          <div class="hint">
            Web mode only: no local editor, file watcher or language server. Uses the least memory.
            {#if !app.info?.full_build}<strong>This is the Lite build, so it's always on.</strong>{/if}
          </div>
        </div>
        <input
          type="checkbox"
          class="switch"
          checked={cfg.general.lite_mode || !app.info?.full_build}
          disabled={!app.info?.full_build}
          onchange={(e) => app.save((c) => (c.general.lite_mode = e.currentTarget.checked))}
        />
      </label>
      <label class="row">
        <div>
          <div class="label">Keep running in the tray when the window closes</div>
          <div class="hint">Quit from the tray menu.</div>
        </div>
        <input
          type="checkbox"
          class="switch"
          checked={cfg.general.close_to_tray}
          onchange={(e) => app.save((c) => (c.general.close_to_tray = e.currentTarget.checked))}
        />
      </label>
      <label class="row">
        <div>
          <div class="label">Start Canopy when you sign in</div>
          <div class="hint">Starts hidden in the tray.</div>
        </div>
        <input
          type="checkbox"
          class="switch"
          checked={cfg.general.start_with_system}
          onchange={(e) => app.save((c) => (c.general.start_with_system = e.currentTarget.checked))}
        />
      </label>
    </div>

    <div class="card">
      <h2>Compiler</h2>
      <div class="row">
        <div>
          <div class="label">Local compiler</div>
          <div class="hint">
            {#if tex?.engine}
              Compiles will use <strong>{tex.engine}</strong>.
            {:else if cfg.compile.engine === "remote"}
              Compiles run on the Overleaf server.
            {:else}
              Nothing usable found yet. Tectonic installs with one click once local mode lands.
            {/if}
          </div>
        </div>
        <select
          value={cfg.compile.engine}
          onchange={async (e) => {
            const v = e.currentTarget.value as EnginePref;
            await app.save((c) => (c.compile.engine = v));
            onrescan();
          }}
        >
          {#each engines as en (en.value)}<option value={en.value}>{en.label}</option>{/each}
        </select>
      </div>
      <table class="tools">
        <tbody>
          {#each toolNames as n (n)}
            <tr>
              <td class="tool">{n}</td>
              <td class:missing={!tex?.tools[n]}>{tex ? (tex.tools[n] ?? "not found") : "checking…"}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="buttons"><button class="ghost" onclick={onrescan}>Scan again</button></div>
    </div>

    <div class="card">
      <h2>Updates</h2>
      <label class="row">
        <div>
          <div class="label">Check for updates automatically</div>
          <div class="hint">Nothing installs until you say so. Updates are signature-checked first.</div>
        </div>
        <input
          type="checkbox"
          class="switch"
          checked={cfg.updater.auto_update}
          onchange={(e) => app.save((c) => (c.updater.auto_update = e.currentTarget.checked))}
        />
      </label>
      <div class="buttons"><button class="ghost" onclick={() => checkForUpdates(true)}>Check now</button></div>
    </div>

    <div class="card">
      <h2>Your data</h2>
      <p class="hint">
        Settings save as a <code>.londopy</code> file you can move to another computer. Accounts aren't included: a
        sign-in stays on the computer it was made on.
      </p>
      <div class="buttons">
        <button class="ghost" onclick={exportSettings}>Export settings…</button>
        <button class="ghost" onclick={importSettings}>Import settings…</button>
        <button class="ghost" onclick={() => api.openDataDir().catch((e) => app.toast("error", errText(e)))}>
          Open data folder
        </button>
      </div>
    </div>

    {#if app.info}
      <div class="card about">
        <h2>About</h2>
        <p>
          <strong>Canopy</strong> v{app.info.version}{app.info.full_build ? "" : " Lite"} · built {app.info.build_date}
          · {app.info.os}/{app.info.arch}{app.info.portable ? " · portable" : ""}
        </p>
        <p class="hint">Data folder: <code>{app.info.data_dir}</code></p>
        <p class="hint">Canopy is an independent project and isn't affiliated with Overleaf.</p>
        <div class="buttons">
          <button class="ghost" onclick={() => openUrl(app.info!.homepage)}>Project page</button>
          <button class="ghost" onclick={() => openUrl(`${app.info!.homepage}/issues`)}>Report a problem</button>
        </div>
      </div>
    {/if}
  </section>
{/if}

<style>
  .settings {
    max-width: 760px;
    margin: 0 auto;
    padding: 32px 32px 56px;
  }
  h1 {
    font-size: 22px;
    margin: 0 0 20px;
  }
  .card {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface-raised);
    padding: 6px 18px 14px;
    margin-bottom: 14px;
  }
  h2 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    margin: 12px 0 4px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border-soft);
  }
  .row:last-of-type {
    border-bottom: 0;
  }
  label.row {
    cursor: pointer;
  }
  .label {
    font-weight: 550;
  }
  .hint {
    color: var(--text-muted);
    font-size: 12.5px;
    line-height: 1.5;
    margin-top: 2px;
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: var(--radius);
    background: var(--surface-sunken);
    border: 1px solid var(--border);
  }
  .seg button {
    border: 0;
    background: transparent;
    color: var(--text-muted);
    padding: 5px 12px;
    border-radius: 6px;
    cursor: pointer;
  }
  .seg button.on {
    background: var(--surface-raised);
    color: var(--text);
    box-shadow: var(--shadow-sm);
  }
  .tools {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
    margin-top: 4px;
  }
  .tools td {
    padding: 4px 0;
    font-family: var(--font-mono);
    word-break: break-all;
  }
  .tool {
    width: 90px;
    color: var(--text-muted);
  }
  .missing {
    color: var(--text-faint);
  }
  .buttons {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 10px;
  }
  .about p {
    margin: 8px 0;
  }
</style>
