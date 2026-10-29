<script lang="ts" module>
  // what the editor component offers the view around it
  export interface CodeEditorApi {
    open(path: string, text: string): void;
    text(path?: string): string | null;
    dirty(path: string): boolean;
    dirtyPaths(): string[];
    markSaved(path: string, text: string): void;
    // the text as last loaded or saved, for merging on save
    savedText(path: string): string | null;
    // puts `text` in the buffer as its saved state (a merge result)
    replaceSaved(path: string, text: string): void;
    // replaces a file's text with what's on disk, unless it has unsaved edits;
    // true when it did
    reloadIfClean(path: string, text: string): boolean;
    forget(path: string): void;
    // drops every buffer: for switching to another project, whose files
    // share names with this one's
    reset(): void;
    goto(line: number): void;
    line(): number;
    focus(): void;
  }
</script>

<script lang="ts">
  import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from "@codemirror/autocomplete";
  import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
  import { bracketMatching, foldGutter, HighlightStyle, indentOnInput, StreamLanguage, syntaxHighlighting } from "@codemirror/language";
  import { stex } from "@codemirror/legacy-modes/mode/stex";
  import { type Diagnostic as CmDiagnostic, lintGutter, setDiagnostics } from "@codemirror/lint";
  import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
  import { EditorState, type Extension, Transaction } from "@codemirror/state";
  import {
    crosshairCursor,
    drawSelection,
    dropCursor,
    EditorView,
    highlightActiveLine,
    highlightActiveLineGutter,
    highlightSpecialChars,
    keymap,
    lineNumbers,
    rectangularSelection,
  } from "@codemirror/view";
  import { tags as t } from "@lezer/highlight";
  import { onDestroy, onMount, untrack } from "svelte";
  import type { Diagnostic } from "../types";

  let {
    path,
    diagnostics = [],
    onchange,
    onsave,
    oncompile,
    onforward,
    api = $bindable(),
  }: {
    path: string | null;
    diagnostics?: Diagnostic[];
    onchange?: (path: string) => void;
    onsave?: () => void;
    oncompile?: () => void;
    onforward?: (line: number) => void;
    api?: CodeEditorApi;
  } = $props();

  let host: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  // one editor state per open file, so each keeps its own undo history
  const states = new Map<string, EditorState>();
  const saved = new Map<string, string>();

  // colours come from app.css, so the editor follows the light/dark theme
  const highlight = HighlightStyle.define([
    { tag: [t.keyword, t.tagName, t.macroName], color: "var(--syn-command)" },
    { tag: [t.typeName, t.className, t.atom], color: "var(--syn-env)" },
    { tag: [t.special(t.string), t.number, t.processingInstruction], color: "var(--syn-math)" },
    { tag: t.comment, color: "var(--syn-comment)", fontStyle: "italic" },
    { tag: [t.bracket, t.brace, t.squareBracket], color: "var(--syn-bracket)" },
    { tag: t.string, color: "var(--syn-string)" },
    { tag: t.heading, fontWeight: "600" },
    { tag: t.strong, fontWeight: "600" },
    { tag: t.emphasis, fontStyle: "italic" },
  ]);

  const theme = EditorView.theme({
    "&": { height: "100%", color: "var(--text)", backgroundColor: "var(--surface)", fontSize: "13.5px" },
    ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.6" },
    ".cm-content": { caretColor: "var(--accent)", padding: "8px 0" },
    ".cm-cursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "2px" },
    ".cm-gutters": { backgroundColor: "var(--surface)", color: "var(--text-faint)", border: "none" },
    ".cm-activeLine": { backgroundColor: "var(--hover)" },
    ".cm-activeLineGutter": { backgroundColor: "var(--hover)", color: "var(--text-muted)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
      backgroundColor: "var(--accent-soft) !important",
    },
    ".cm-selectionMatch": { backgroundColor: "var(--accent-soft)" },
    ".cm-matchingBracket": { outline: "1px solid var(--accent)", backgroundColor: "transparent" },
    ".cm-panels": { backgroundColor: "var(--surface-raised)", color: "var(--text)" },
    ".cm-tooltip": { backgroundColor: "var(--surface-raised)", border: "1px solid var(--border)" },
    ".cm-searchMatch": { backgroundColor: "rgba(214, 150, 40, 0.3)" },
  });

  function extensions(): Extension[] {
    return [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      foldGutter(),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      autocompletion({ activateOnTyping: false }),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      search({ top: true }),
      lintGutter(),
      EditorView.lineWrapping,
      StreamLanguage.define(stex),
      syntaxHighlighting(highlight),
      theme,
      keymap.of([
        { key: "Mod-s", preventDefault: true, run: () => (onsave?.(), true) },
        { key: "Mod-Enter", preventDefault: true, run: () => (oncompile?.(), true) },
        // LaTeX Workshop's key for "show in PDF"
        { key: "Mod-Alt-j", preventDefault: true, run: () => (onforward?.(currentLine()), true) },
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...completionKeymap,
        indentWithTab,
      ]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged && path) onchange?.(path);
      }),
    ];
  }

  function stateFor(text: string): EditorState {
    return EditorState.create({ doc: text, extensions: extensions() });
  }

  function currentLine(): number {
    if (!view) return 1;
    return view.state.doc.lineAt(view.state.selection.main.head).number;
  }

  // the view shows `path`'s state; the old file's state is kept for later.
  // only a real switch (or `force`, for a file just opened) swaps the state:
  // swapping it otherwise would throw away the edits made since
  let shown: string | null = null;
  function show(p: string | null, force = false) {
    if (!view) return;
    if (p === shown && !force) return;
    if (shown && shown !== p) states.set(shown, view.state);
    shown = p;
    if (p && states.has(p)) {
      view.setState(states.get(p)!);
    } else {
      view.setState(stateFor(""));
    }
    applyDiagnostics();
  }

  function applyDiagnostics() {
    if (!view) return;
    const doc = view.state.doc;
    const list: CmDiagnostic[] = diagnostics
      .filter((d) => d.line && d.line >= 1 && d.line <= doc.lines)
      .map((d) => {
        const line = doc.line(d.line!);
        return {
          from: line.from,
          to: line.to,
          severity: d.severity === "error" ? "error" : d.severity === "warning" ? "warning" : "info",
          message: d.message,
        };
      });
    view.dispatch(setDiagnostics(view.state, list));
  }

  $effect(() => {
    void diagnostics;
    applyDiagnostics();
  });

  // reacts to `path` only; diagnostics and other props must not re-run it
  $effect(() => {
    const p = path;
    untrack(() => show(p));
  });

  api = {
    open(p, text) {
      if (!states.has(p)) {
        states.set(p, stateFor(text));
        saved.set(p, text);
      }
      if (p === shown) show(p, true);
    },
    text(p) {
      const key = p ?? shown;
      if (!key) return null;
      if (key === shown && view) return view.state.doc.toString();
      return states.get(key)?.doc.toString() ?? null;
    },
    dirty(p) {
      const now = this.text(p);
      return now !== null && now !== saved.get(p);
    },
    dirtyPaths() {
      const all = new Set([...states.keys(), ...(shown ? [shown] : [])]);
      return [...all].filter((p) => this.dirty(p));
    },
    markSaved(p, text) {
      saved.set(p, text);
    },
    savedText(p) {
      return saved.get(p) ?? null;
    },
    replaceSaved(p, text) {
      saved.set(p, text);
      if (p === shown && view) {
        if (view.state.doc.toString() === text) return;
        const head = Math.min(view.state.selection.main.head, text.length);
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: text },
          selection: { anchor: head },
          annotations: Transaction.addToHistory.of(false),
        });
      } else if (states.has(p)) {
        states.set(p, stateFor(text));
      }
    },
    reloadIfClean(p, text) {
      if (this.dirty(p) || this.text(p) === text) return false;
      // saved first: the change listener then sees a clean buffer
      saved.set(p, text);
      if (p === shown && view) {
        // keep the cursor where it was, as far as the new text allows; not an
        // undo step, since undoing it would bring back the old text unsaved
        const head = Math.min(view.state.selection.main.head, text.length);
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: text },
          selection: { anchor: head },
          annotations: Transaction.addToHistory.of(false),
        });
      } else {
        states.set(p, stateFor(text));
      }
      return true;
    },
    forget(p) {
      states.delete(p);
      saved.delete(p);
      // the closed file mustn't live on as the view's state
      if (p === shown) {
        shown = null;
        view?.setState(stateFor(""));
      }
    },
    reset() {
      states.clear();
      saved.clear();
      shown = null;
      view?.setState(stateFor(""));
    },
    goto(lineNo) {
      if (!view) return;
      const doc = view.state.doc;
      const line = doc.line(Math.max(1, Math.min(lineNo, doc.lines)));
      view.dispatch({ selection: { anchor: line.from }, effects: EditorView.scrollIntoView(line.from, { y: "center" }) });
      view.focus();
    },
    line: currentLine,
    focus() {
      view?.focus();
    },
  };

  onMount(() => {
    view = new EditorView({ parent: host!, state: stateFor("") });
    show(path, true);
  });

  onDestroy(() => view?.destroy());
</script>

<div class="code" bind:this={host}></div>

<style>
  .code {
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }
</style>
