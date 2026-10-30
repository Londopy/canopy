<script lang="ts">
  import { FitAddon } from "@xterm/addon-fit";
  import { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";
  import { errText } from "../api";

  // a shell in the project's folder, in a pseudo-terminal on the Rust side
  let { linkId, dark }: { linkId: string; dark: boolean } = $props();

  let host: HTMLDivElement | undefined = $state();
  let error = $state<string | null>(null);
  let term: Terminal | null = null;
  let fit: FitAddon | null = null;
  let id: number | null = null;
  const stops: (() => void)[] = [];
  let ro: ResizeObserver | null = null;

  const themes = {
    dark: { background: "#121513", foreground: "#e4e9e6", cursor: "#4fbf82", selectionBackground: "#2f8f5b55" },
    light: { background: "#f1f3f1", foreground: "#1d2420", cursor: "#2f8f5b", selectionBackground: "#2f8f5b33" },
  };

  $effect(() => {
    if (term) term.options.theme = dark ? themes.dark : themes.light;
  });

  onMount(async () => {
    term = new Terminal({
      fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-mono") || "monospace",
      fontSize: 13,
      cursorBlink: true,
      scrollback: 5000,
      theme: dark ? themes.dark : themes.light,
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host!);
    fit.fit();
    // listen before the shell starts: its first output arrives within
    // milliseconds (on Windows, a cursor-position question the shell waits
    // on), before term_open has even returned our id. until then, keep it
    const early: { id: number; data: string }[] = [];
    let exitedEarly: number | null = null;
    stops.push(
      await listen<{ id: number; data: string }>("term-data", (e) => {
        if (id === null) early.push(e.payload);
        else if (e.payload.id === id) term?.write(e.payload.data);
      }),
      await listen<{ id: number }>("term-exit", (e) => {
        if (id === null) exitedEarly = e.payload.id;
        else if (e.payload.id === id) {
          term?.write("\r\n[the shell has exited]\r\n");
          id = null;
        }
      }),
    );
    let opened: number;
    try {
      opened = await invoke<number>("term_open", { linkId, cols: term.cols, rows: term.rows });
    } catch (e) {
      error = errText(e);
      return;
    }
    // the panel closed while the shell was starting: don't leave it running
    if (destroyed) {
      void invoke("term_close", { id: opened }).catch(() => {});
      return;
    }
    id = opened;
    // handlers before the replay: xterm answers the shell's questions
    // (cursor position) through onData
    term.onData((data) => {
      if (id !== null) void invoke("term_write", { id, data }).catch(() => {});
    });
    term.onResize(({ cols, rows }) => {
      if (id !== null) void invoke("term_resize", { id, cols, rows }).catch(() => {});
    });
    for (const d of early.splice(0)) if (d.id === opened) term.write(d.data);
    if (exitedEarly === opened) {
      term.write("\r\n[the shell has exited]\r\n");
      id = null;
    }
    ro = new ResizeObserver(() => fit?.fit());
    ro.observe(host!);
    term.focus();
  });

  let destroyed = false;
  onDestroy(() => {
    destroyed = true;
    ro?.disconnect();
    for (const s of stops) s();
    if (id !== null) void invoke("term_close", { id }).catch(() => {});
    term?.dispose();
  });
</script>

<div class="term">
  {#if error}<p class="error">{error}</p>{/if}
  <div class="host" bind:this={host}></div>
</div>

<style>
  .term {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 4px 0 0 8px;
    background: var(--surface-sunken);
  }
  .host {
    flex: 1;
    min-height: 0;
  }
  .error {
    margin: 6px 4px;
    color: var(--danger);
    font-size: 12.5px;
  }
</style>
