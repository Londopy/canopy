<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { api, errText } from "./lib/api";
  import { commands, VIEWS } from "./lib/commands";
  import AccountDialog from "./lib/components/AccountDialog.svelte";
  import ActivityBar from "./lib/components/ActivityBar.svelte";
  import CommandPalette from "./lib/components/CommandPalette.svelte";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import Toasts from "./lib/components/Toasts.svelte";
  import UpdateBanner from "./lib/components/UpdateBanner.svelte";
  import { openLondopy } from "./lib/settingsio";
  import { app } from "./lib/state.svelte";
  import type { LinkStatus, PageState, TexReport } from "./lib/types";
  import { startUpdateSchedule } from "./lib/updates";
  import Projects from "./lib/views/Projects.svelte";
  import Settings from "./lib/views/Settings.svelte";
  import History from "./lib/views/History.svelte";
  import Sync from "./lib/views/Sync.svelte";
  import Upcoming from "./lib/views/Upcoming.svelte";
  import WebView from "./lib/views/WebView.svelte";

  let tex = $state<TexReport | null>(null);
  let loadError = $state<string | null>(null);

  const engineLabel = $derived(
    app.config?.compile.engine === "remote" ? "Overleaf compile" : (tex?.engine ?? (tex ? "no local TeX" : "…")),
  );

  async function rescan() {
    try {
      tex = await api.detectTex();
    } catch (e) {
      app.toast("error", `Couldn't scan for TeX: ${errText(e)}`);
    }
  }

  // the native title bar follows the app theme, not just the page
  $effect(() => {
    const theme = app.config?.general.theme ?? "system";
    document.documentElement.dataset.theme = app.dark ? "dark" : "light";
    getCurrentWindow()
      .setTheme(theme === "system" ? null : theme)
      .catch(() => {});
  });

  // lite mode hides some views; don't strand the user on one
  $effect(() => {
    const v = VIEWS.find((x) => x.view === app.view);
    if (v?.full && app.lite) app.view = "projects";
  });

  function onkeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) return;
    const key = e.key.toLowerCase();
    if ((e.shiftKey && key === "p") || (!e.shiftKey && key === "k")) {
      e.preventDefault();
      app.paletteOpen = !app.paletteOpen;
      return;
    }
    // every other shortcut lives on its command, so the palette shows the same keys
    const combo = `Ctrl+${e.shiftKey ? "Shift+" : ""}${e.key.length === 1 ? e.key.toUpperCase() : e.key}`;
    const cmd = commands().find((c) => c.keys === combo);
    if (cmd) {
      e.preventDefault();
      void cmd.run();
    }
  }

  onMount(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    app.systemDark = media.matches;
    const onMedia = (e: MediaQueryListEvent) => (app.systemDark = e.matches);
    media.addEventListener("change", onMedia);

    const unlisten = [
      listen("tray-settings", () => (app.view = "settings")),
      listen("tray-update", () => (app.view = "settings")),
      listen<string>("londopy-path", (e) => openLondopy(e.payload)),
      listen<PageState>("web-state", (e) => {
        const before = app.pages[e.payload.id];
        app.pages[e.payload.id] = {
          ...e.payload,
          // an SSO provider's page doesn't say; keep what we knew
          signedIn: e.payload.signedIn ?? before?.signedIn ?? null,
          title: before?.title,
        };
      }),
      listen<LinkStatus>("sync-status", (e) => {
        app.sync[e.payload.id] = e.payload;
      }),
      listen<[string, string]>("web-title", (e) => {
        const [id, title] = e.payload;
        const page = app.pages[id];
        if (page) page.title = title;
      }),
    ];
    let stopUpdates = () => {};

    app
      .load()
      .then(async () => {
        stopUpdates = startUpdateSchedule();
        void rescan();
        // statuses from syncs that ran before the UI loaded (full build only)
        if (app.info?.full_build) {
          for (const s of await api.syncStatuses().catch(() => [])) app.sync[s.id] = s;
        }
        const file = await api.takeStartupFile();
        if (file) await openLondopy(file);
      })
      .catch((e) => (loadError = errText(e)));

    return () => {
      media.removeEventListener("change", onMedia);
      unlisten.forEach((p) => p.then((f) => f()));
      stopUpdates();
    };
  });
</script>

<svelte:window {onkeydown} />

{#if loadError}
  <div class="fatal">
    <h1>Canopy couldn't start</h1>
    <p>{loadError}</p>
  </div>
{:else if app.config}
  <div class="shell">
    <ActivityBar />
    <div class="main">
      <UpdateBanner />
      <main class="content">
        {#if app.view === "projects"}
          <Projects />
        {:else if app.view === "web"}
          <WebView />
        {:else if app.view === "settings"}
          <Settings {tex} onrescan={rescan} />
        {:else if app.view === "search"}
          <Upcoming icon="search" title="Project search" text="Search and replace across every file in a synced project. Arrives with local mode." />
        {:else if app.view === "sync"}
          <Sync />
        {:else if app.view === "history"}
          <History />
        {:else if app.view === "dashboard"}
          <Upcoming icon="chart" title="Writing dashboard" text="Word counts per section, daily goals, deadlines and compile errors at a glance." />
        {/if}
      </main>
    </div>
  </div>
  <StatusBar engine={engineLabel} />
  {#if app.paletteOpen}<CommandPalette />{/if}
  <AccountDialog />
{/if}
<Toasts />

<style>
  .shell {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .content {
    flex: 1;
    overflow: auto;
  }
  .fatal {
    margin: auto;
    max-width: 520px;
    padding: 32px;
    text-align: center;
  }
</style>
