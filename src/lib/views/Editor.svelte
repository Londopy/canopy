<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { onDestroy, onMount } from "svelte";
  import { api, errText } from "../api";
  import CodeEditor, { type CodeEditorApi } from "../components/CodeEditor.svelte";
  import Icon from "../components/Icon.svelte";
  import PdfView, { type PdfViewApi } from "../components/PdfView.svelte";
  import { app } from "../state.svelte";
  import type { CompileOutcome, Diagnostic, FileEntry } from "../types";

  // files the editor opens; anything else (images, PDFs) shows in the tree only
  const TEXT = /\.(tex|bib|sty|cls|bst|bbx|cbx|dtx|ins|ltx|txt|md|cfg|def|clo|latexmkrc|tikz|asy|gnuplot|csv|tsv|json|yaml|yml|lua|py)$/i;

  const links = $derived(app.config?.links ?? []);
  const linkId = $derived(app.editorLink && links.some((l) => l.id === app.editorLink) ? app.editorLink : (links[0]?.id ?? null));
  const link = $derived(links.find((l) => l.id === linkId) ?? null);

  let files = $state<FileEntry[]>([]);
  let mainFile = $state<string | null>(null);
  let openPaths = $state<string[]>([]);
  let current = $state<string | null>(null);
  let dirty = $state<Record<string, boolean>>({});
  let compiling = $state(false);
  // a compile that runs long is often MiKTeX waiting on its install dialog
  let slow = $state(false);
  let slowTimer: ReturnType<typeof setTimeout> | undefined;
  let outcome = $state<CompileOutcome | null>(null);
  let pdf = $state<ArrayBuffer | null>(null);
  let showProblems = $state(false);
  let newFile = $state<string | null>(null);
  let pdfShare = $state(readShare());
  let compileOnSave = $state(readFlag("canopy.compileOnSave", true));
  let editor: CodeEditorApi | undefined = $state();
  let pdfView: PdfViewApi | undefined = $state();

  // per-viewer layout conveniences; storage can be missing, so never required
  function readShare(): number {
    try {
      const v = Number(localStorage.getItem("canopy.pdfShare"));
      return v >= 0.2 && v <= 0.8 ? v : 0.45;
    } catch {
      return 0.45;
    }
  }
  function readFlag(key: string, fallback: boolean): boolean {
    try {
      const v = localStorage.getItem(key);
      return v === null ? fallback : v === "1";
    } catch {
      return fallback;
    }
  }
  function store(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      /* private mode: the setting just isn't remembered */
    }
  }

  const problems = $derived(outcome?.diagnostics ?? []);
  const errors = $derived(problems.filter((d) => d.severity === "error").length);
  const warnings = $derived(problems.filter((d) => d.severity !== "error").length);
  const fileProblems = $derived(problems.filter((d) => d.file === current));

  // the tree: folders first, by path
  type Row = { kind: "dir" | "file"; path: string; name: string; depth: number; size?: number };
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const seen = new Set<string>();
    for (const f of files) {
      const parts = f.path.split("/");
      for (let i = 1; i < parts.length; i++) {
        const dir = parts.slice(0, i).join("/");
        if (!seen.has(dir)) {
          seen.add(dir);
          out.push({ kind: "dir", path: dir, name: parts[i - 1], depth: i - 1 });
        }
      }
      out.push({ kind: "file", path: f.path, name: parts[parts.length - 1], depth: parts.length - 1, size: f.size });
    }
    return out;
  });

  // the project the open buffers belong to; saves always go there, even
  // while the view is already switching to another one
  let bufferProject: string | null = null;

  async function loadProject(id: string, previous: string | null) {
    // the old project's unsaved edits go to the old project's folder first;
    // if that fails, stay on it rather than drop them
    if (previous && editor?.dirtyPaths().length) {
      if (!(await saveAll(previous))) {
        app.toast("error", "Stayed on this project: its unsaved edits couldn't be saved.");
        loadedFor = previous;
        app.editorLink = previous;
        return;
      }
    }
    editor?.reset();
    bufferProject = id;
    files = [];
    openPaths = [];
    current = null;
    outcome = null;
    pdf = null;
    dirty = {};
    try {
      files = await api.localFiles(id);
      mainFile = await api.mainFile(id);
      const first = mainFile ?? files.find((f) => TEXT.test(f.path))?.path;
      if (first) await openFile(first);
    } catch (e) {
      app.toast("error", errText(e));
    }
  }

  let loadedFor: string | null = null;
  $effect(() => {
    const id = linkId;
    if (id && id !== loadedFor) {
      const previous = loadedFor;
      loadedFor = id;
      void loadProject(id, previous);
    }
  });

  // a file named by a search, a dashboard section or a citation, to open
  // once the project is loaded
  $effect(() => {
    const req = app.editorOpen;
    if (req && editor && bufferProject === linkId && files.length) {
      app.editorOpen = null;
      void openFile(req.path, req.line);
    }
  });

  async function openFile(path: string, line?: number) {
    if (!linkId || !editor) return;
    if (!TEXT.test(path)) {
      app.toast("info", `${path} isn't a text file Canopy edits.`);
      return;
    }
    if (!openPaths.includes(path)) {
      try {
        const text = await api.readText(linkId, path);
        editor.open(path, text);
        openPaths = [...openPaths, path];
      } catch (e) {
        app.toast("error", errText(e));
        return;
      }
    }
    current = path;
    if (line) queueMicrotask(() => editor?.goto(line));
  }

  function closeFile(path: string) {
    if (editor?.dirty(path) && !confirm(`${path} has unsaved changes. Close it anyway?`)) return;
    editor?.forget(path);
    openPaths = openPaths.filter((p) => p !== path);
    delete dirty[path];
    if (current === path) current = openPaths[openPaths.length - 1] ?? null;
  }

  // saves run one after another: two at once would race on the same file
  let saving: Promise<boolean> = Promise.resolve(true);
  // a save asked for during a compile waits for it: the TeX program holds
  // the files open, and Windows refuses to replace them meanwhile
  let saveAfterCompile = false;

  function saveAll(id: string | null = bufferProject): Promise<boolean> {
    saving = saving.then(() => saveNow(id));
    return saving;
  }

  async function saveNow(id: string | null): Promise<boolean> {
    if (!id || !editor) return false;
    let ok = true;
    for (const path of editor.dirtyPaths()) {
      const text = editor.text(path) ?? "";
      const base = editor.savedText(path);
      try {
        let r;
        try {
          r = await api.writeText(id, path, text, base, false);
        } catch (e) {
          const msg = errText(e);
          if (!msg.startsWith("conflict:")) throw e;
          const keep = await ask(
            `${path} changed on Overleaf on the same lines you edited here. Save yours over it? Theirs stays in Snapshots, so you can get it back.`,
            { title: "Save over a change", kind: "warning", okLabel: "Save mine", cancelLabel: "Not now" },
          );
          if (!keep) {
            ok = false;
            continue;
          }
          r = await api.writeText(id, path, text, base, true);
        }
        if (r.merged) {
          editor.replaceSaved(path, r.text);
          app.toast("info", `Merged a change from Overleaf into ${path}.`);
        } else {
          editor.markSaved(path, text);
        }
        dirty[path] = editor.dirty(path);
      } catch (e) {
        ok = false;
        app.toast("error", `Couldn't save ${path}: ${errText(e)}`);
      }
    }
    return ok;
  }

  async function save() {
    if (compiling) {
      saveAfterCompile = true;
      app.toast("info", "Saving when this compile finishes.");
      return;
    }
    if (await saveAll()) {
      if (compileOnSave) void compile();
    }
  }

  async function compile() {
    const id = bufferProject;
    if (!id || compiling) return;
    await saveAll(id);
    compiling = true;
    slow = false;
    slowTimer = setTimeout(() => (slow = true), 8000);
    try {
      const o = await api.compile(id);
      // the view moved to another project meanwhile: this result isn't its
      if (bufferProject !== id) return;
      outcome = o;
      if (o.pdf) pdf = await api.readPdf(id);
      const errs = o.diagnostics.filter((d) => d.severity === "error").length;
      if (o.cancelled) app.toast("info", "Compile stopped.");
      else if (o.timed_out) app.toast("error", "The compile took over 5 minutes and was stopped.");
      else if (errs) showProblems = true;
    } catch (e) {
      app.toast("error", errText(e));
    } finally {
      compiling = false;
      slow = false;
      clearTimeout(slowTimer);
      if (saveAfterCompile) {
        saveAfterCompile = false;
        void save();
      }
    }
  }
  async function forward(line: number) {
    if (!linkId || !current) return;
    try {
      const spots = await api.synctexForward(linkId, current, line);
      if (spots[0]) pdfView?.show(spots[0]);
      else app.toast("info", "That line isn't in the PDF yet. Compile first.");
    } catch (e) {
      app.toast("error", errText(e));
    }
  }

  async function backward(page: number, x: number, y: number) {
    if (!linkId) return;
    try {
      const hit = await api.synctexBackward(linkId, page, x, y);
      if (hit) await openFile(hit.path, hit.line);
    } catch (e) {
      app.toast("error", errText(e));
    }
  }

  function goTo(d: Diagnostic) {
    if (d.file && !d.file.includes(":") && !d.file.startsWith("/")) void openFile(d.file, d.line ?? undefined);
  }

  async function createFile() {
    const name = (newFile ?? "").trim().replace(/\\/g, "/");
    newFile = null;
    if (!name || !linkId) return;
    try {
      await api.createText(linkId, name);
      files = await api.localFiles(linkId);
      await openFile(name);
    } catch (e) {
      app.toast("error", errText(e));
    }
  }

  async function setMain(path: string) {
    if (!linkId) return;
    try {
      await api.setMainFile(linkId, path);
      mainFile = path;
    } catch (e) {
      app.toast("error", errText(e));
    }
  }

  // a sync may have brought new versions of open files: reload the ones
  // without unsaved edits, warn about the others
  let lastSeen: number | null = null;
  $effect(() => {
    const s = linkId ? app.sync[linkId] : undefined;
    const at = s?.lastSync ?? null;
    if (!linkId || at === null || at === lastSeen) return;
    lastSeen = at;
    void refreshFromDisk(linkId);
  });

  const warned: Record<string, string> = {};

  async function refreshFromDisk(id: string) {
    if (id !== bufferProject) return;
    try {
      files = await api.localFiles(id);
    } catch {
      return;
    }
    for (const path of openPaths) {
      try {
        const text = await api.readText(id, path);
        if (editor?.dirty(path)) {
          // changed since the buffer was loaded: say so once per change
          if (text !== editor.savedText(path) && warned[path] !== text) {
            warned[path] = text;
            app.toast("info", `${path} changed on Overleaf while you have unsaved edits here. Saving merges the two.`);
          }
        } else {
          editor?.reloadIfClean(path, text);
        }
      } catch {
        // deleted on the other side: leave the buffer, the user can save it back
      }
    }
  }

  // dragging the divider between editor and PDF
  let dragging = false;
  let body: HTMLDivElement | undefined = $state();
  function startDrag(e: PointerEvent) {
    dragging = true;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }
  function drag(e: PointerEvent) {
    if (!dragging || !body) return;
    const r = body.getBoundingClientRect();
    pdfShare = Math.min(0.8, Math.max(0.2, (r.right - e.clientX) / r.width));
  }
  function endDrag() {
    if (!dragging) return;
    dragging = false;
    store("canopy.pdfShare", String(pdfShare));
  }

  let unlisten: (() => void) | null = null;
  onMount(async () => {
    unlisten = await listen<{ linkId: string; running: boolean }>("compile", (e) => {
      if (e.payload.linkId === linkId) compiling = e.payload.running;
    });
  });
  onDestroy(() => unlisten?.());

  // leaving with unsaved edits saves them: they'd be lost otherwise
  onDestroy(() => {
    void saveAll();
  });

  function size(n: number | undefined) {
    if (n === undefined) return "";
    return n < 1024 ? `${n} B` : n < 1048576 ? `${Math.round(n / 1024)} KB` : `${(n / 1048576).toFixed(1)} MB`;
  }
</script>

{#if !link}
  <section class="empty">
    <Icon name="code" size={30} />
    <h2>Edit a project on this computer</h2>
    <p>
      Sync a project to a folder first (Projects, then <strong>Sync to a folder</strong>). Then it opens here, with a
      PDF preview compiled by the TeX install on this computer.
    </p>
    <button class="primary" onclick={() => (app.view = "projects")}>Go to projects</button>
  </section>
{:else}
  <section class="editor">
    <header class="bar">
      {#if links.length > 1}
        <select value={linkId} onchange={(e) => (app.editorLink = e.currentTarget.value)} aria-label="Project">
          {#each links as l (l.id)}<option value={l.id}>{l.project_name}</option>{/each}
        </select>
      {:else}
        <strong class="title">{link.project_name}</strong>
      {/if}
      <label class="main">
        Main
        <select value={mainFile} onchange={(e) => setMain(e.currentTarget.value)}>
          {#each files.filter((f) => f.path.toLowerCase().endsWith(".tex")) as f (f.path)}
            <option value={f.path}>{f.path}</option>
          {/each}
        </select>
      </label>
      <span class="spacer"></span>
      <label class="toggle" title="Compile every time you save">
        <input type="checkbox" class="switch" bind:checked={compileOnSave} onchange={() => store("canopy.compileOnSave", compileOnSave ? "1" : "0")} />
        Compile on save
      </label>
      {#if outcome}
        <button class="status" class:bad={errors > 0} onclick={() => (showProblems = !showProblems)} title="Problems">
          {#if errors}<Icon name="alert" size={14} /> {errors} error{errors === 1 ? "" : "s"}{:else}<Icon name="check" size={14} /> Built in {outcome.seconds.toFixed(1)}s{/if}
          {#if warnings}<span class="muted">· {warnings} warning{warnings === 1 ? "" : "s"}</span>{/if}
        </button>
      {/if}
      {#if compiling && slow}
        <span class="hint" title="MiKTeX asks before installing a missing package. Its window may be behind Canopy.">
          Still compiling. Is MiKTeX asking to install a package?
        </span>
      {/if}
      {#if compiling}
        <button class="ghost small" onclick={() => linkId && api.cancelCompile(linkId)}>Stop</button>
      {/if}
      <button class="primary small" disabled={compiling} onclick={compile} title="Compile (Ctrl+Enter)">
        {compiling ? "Compiling…" : "Compile"}
      </button>
    </header>

    <div class="body" bind:this={body} onpointermove={drag} onpointerup={endDrag} role="presentation">
      <nav class="tree" aria-label="Files">
        <div class="tree-head">
          <span>Files</span>
          <button class="icon" title="New file" aria-label="New file" onclick={() => (newFile = "")}><Icon name="plus" size={15} /></button>
        </div>
        {#if newFile !== null}
          <form onsubmit={(e) => (e.preventDefault(), createFile())}>
            <!-- svelte-ignore a11y_autofocus -->
            <input class="newfile" bind:value={newFile} placeholder="chapters/intro.tex" autofocus onblur={() => (newFile = null)} />
          </form>
        {/if}
        <ul>
          {#each rows as r (r.kind + r.path)}
            <li style={`padding-left: ${8 + r.depth * 12}px`}>
              {#if r.kind === "dir"}
                <span class="dir"><Icon name="folder" size={14} /> {r.name}</span>
              {:else}
                <button
                  class="file"
                  class:on={current === r.path}
                  class:dim={!TEXT.test(r.path)}
                  title={`${r.path} · ${size(r.size)}`}
                  onclick={() => openFile(r.path)}
                >
                  {r.name}{#if r.path === mainFile}<span class="badge">main</span>{/if}
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      </nav>

      <div class="center">
        <div class="tabs" role="tablist">
          {#each openPaths as p (p)}
            <div class="tab" class:on={current === p} role="tab" aria-selected={current === p}>
              <button class="tab-name" onclick={() => (current = p)} title={p}>
                {p.split("/").pop()}{#if dirty[p]}<span class="dot" aria-label="unsaved">●</span>{/if}
              </button>
              <button class="tab-x" aria-label={`Close ${p}`} onclick={() => closeFile(p)}><Icon name="x" size={12} /></button>
            </div>
          {/each}
          <span class="spacer"></span>
          {#if current}
            <button class="ghost tiny" title="Show this line in the PDF (Ctrl+Alt+J)" onclick={() => editor && forward(editor.line())}>Show in PDF</button>
          {/if}
        </div>
        <div class="code">
          <CodeEditor
            bind:api={editor}
            path={current}
            diagnostics={fileProblems}
            onchange={(p) => (dirty[p] = editor?.dirty(p) ?? true)}
            onsave={save}
            oncompile={compile}
            onforward={forward}
          />
        </div>
        {#if showProblems && outcome}
          <div class="problems">
            <div class="problems-head">
              <strong>Problems</strong>
              <span class="muted">{outcome.engine} · {outcome.program} · {outcome.runs} run{outcome.runs === 1 ? "" : "s"}</span>
              <span class="spacer"></span>
              <button class="icon" aria-label="Close problems" onclick={() => (showProblems = false)}><Icon name="x" size={14} /></button>
            </div>
            {#if problems.length === 0}
              <p class="muted pad">No problems in the log.</p>
              {#if !outcome.ok && outcome.output_tail}
                <pre class="tail">{outcome.output_tail}</pre>
              {/if}
            {:else}
              <ul>
                {#each problems as d, i (i)}
                  <li>
                    <button class="problem {d.severity}" onclick={() => goTo(d)}>
                      <span class="sev">{d.severity === "error" ? "Error" : d.severity === "warning" ? "Warning" : "Bad box"}</span>
                      <span class="msg">{d.message}</span>
                      <span class="where">{d.file ?? ""}{d.line ? `:${d.line}` : ""}</span>
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        {/if}
      </div>

      <div class="divider" onpointerdown={startDrag} role="separator" aria-orientation="vertical" aria-label="Resize the PDF"></div>

      <div class="pdf" style={`width: ${pdfShare * 100}%`}>
        <PdfView bind:api={pdfView} data={pdf} onpick={backward} />
      </div>
    </div>
  </section>
{/if}

<style>
  .editor {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .bar {
    flex: none;
    height: 44px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-raised);
  }
  .title {
    font-size: 14px;
  }
  .main {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: 12.5px;
  }
  select {
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    font-size: 12.5px;
    max-width: 260px;
  }
  .spacer {
    flex: 1;
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: 12.5px;
    cursor: pointer;
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 9px;
    border: 0;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 12px;
    cursor: pointer;
  }
  .status.bad {
    background: rgba(194, 69, 61, 0.12);
    color: var(--danger);
  }
  .muted {
    color: var(--text-faint);
  }
  .hint {
    color: var(--warn);
    font-size: 12px;
  }
  button.small {
    padding: 5px 12px;
    font-size: 12.5px;
  }
  button.tiny {
    padding: 2px 8px;
    font-size: 12px;
    margin-right: 6px;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .tree {
    flex: none;
    width: 210px;
    overflow: auto;
    border-right: 1px solid var(--border);
    background: var(--surface-raised);
    font-size: 12.5px;
  }
  .tree-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 6px 4px 10px;
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 11px;
  }
  .tree ul {
    list-style: none;
    margin: 0;
    padding: 0 0 12px;
  }
  .dir {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 0;
    color: var(--text-muted);
  }
  .file {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px 3px 19px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .file:hover {
    background: var(--hover);
  }
  .file.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .file.dim {
    color: var(--text-faint);
  }
  .badge {
    font-size: 10px;
    padding: 0 5px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .newfile {
    width: calc(100% - 16px);
    margin: 4px 8px;
    padding: 4px 6px;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
  }
  .icon {
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .icon:hover {
    background: var(--hover);
  }
  .center {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .tabs {
    flex: none;
    height: 34px;
    display: flex;
    align-items: stretch;
    overflow-x: auto;
    border-bottom: 1px solid var(--border);
    background: var(--surface-sunken);
  }
  .tab {
    display: flex;
    align-items: center;
    border-right: 1px solid var(--border-soft);
    color: var(--text-muted);
  }
  .tab.on {
    background: var(--surface);
    color: var(--text);
  }
  .tab-name {
    border: 0;
    background: transparent;
    color: inherit;
    padding: 0 4px 0 12px;
    font-size: 12.5px;
    cursor: pointer;
    white-space: nowrap;
  }
  .dot {
    margin-left: 5px;
    color: var(--accent);
    font-size: 10px;
  }
  .tab-x {
    width: 22px;
    height: 22px;
    margin-right: 4px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
  }
  .tab-x:hover {
    background: var(--hover);
  }
  .tabs .spacer {
    flex: 1;
  }
  .tabs .tiny {
    align-self: center;
  }
  .code {
    flex: 1;
    min-height: 0;
  }
  .problems {
    flex: none;
    max-height: 34%;
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
    background: var(--surface-raised);
  }
  .problems-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    font-size: 12.5px;
  }
  .problems ul {
    list-style: none;
    margin: 0;
    padding: 0 4px 6px;
    overflow: auto;
  }
  .problem {
    width: 100%;
    display: grid;
    grid-template-columns: 70px 1fr auto;
    gap: 10px;
    padding: 4px 8px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text);
    text-align: left;
    font-size: 12.5px;
    cursor: pointer;
  }
  .problem:hover {
    background: var(--hover);
  }
  .problem .sev {
    font-weight: 600;
  }
  .problem.error .sev {
    color: var(--danger);
  }
  .problem.warning .sev {
    color: var(--warn);
  }
  .problem.badbox .sev {
    color: var(--text-faint);
  }
  .where {
    color: var(--text-faint);
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .pad {
    padding: 0 12px;
  }
  .tail {
    margin: 0 12px 10px;
    padding: 8px;
    max-height: 160px;
    overflow: auto;
    background: var(--surface-sunken);
    border-radius: 6px;
    font-size: 11.5px;
  }
  .divider {
    flex: none;
    width: 5px;
    cursor: col-resize;
    background: var(--border);
    touch-action: none;
  }
  .divider:hover {
    background: var(--accent);
  }
  .pdf {
    flex: none;
    min-width: 0;
  }
  .empty {
    text-align: center;
    padding: 14vh 24px;
    color: var(--accent);
  }
  .empty h2 {
    color: var(--text);
    font-size: 17px;
    margin: 12px 0 6px;
  }
  .empty p {
    color: var(--text-muted);
    max-width: 480px;
    margin: 0 auto 16px;
    line-height: 1.55;
  }
</style>
