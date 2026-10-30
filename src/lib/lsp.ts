// a small LSP client for texlab: completion, hover, go to definition and
// diagnostics. messages go through the Rust side (lsp.rs), which runs the
// server; documents are kept in step by sending their full text.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Diagnostic } from "./types";

interface Position {
  line: number;
  character: number;
}
interface Range {
  start: Position;
  end: Position;
}
interface LspDiagnostic {
  range: Range;
  severity?: number;
  message: string;
}
export interface CompletionItem {
  label: string;
  detail?: string;
  kind?: number;
  insertText?: string;
  textEdit?: { newText: string; range?: Range };
}

// a folder path as a file:// URI, the way texlab spells it
export function toUri(path: string): string {
  let p = path.replace(/\\/g, "/");
  if (/^[A-Za-z]:/.test(p)) p = "/" + p;
  return "file://" + p.split("/").map(encodeURIComponent).join("/").replace(/%3A/gi, ":");
}

// a file:// URI back to a project path, or null when it's outside `root`
export function fromUri(uri: string, root: string): string | null {
  if (!uri.startsWith("file://")) return null;
  let p = decodeURIComponent(uri.slice("file://".length));
  if (/^\/[A-Za-z]:/.test(p)) p = p.slice(1);
  const r = root.replace(/\\/g, "/").replace(/\/$/, "");
  const windows = /^[A-Za-z]:/.test(r);
  const a = windows ? p.toLowerCase() : p;
  const b = windows ? r.toLowerCase() : r;
  return a.startsWith(b + "/") ? p.slice(r.length + 1) : null;
}

export class LspClient {
  private versions = new Map<string, number>();
  private unlisten: UnlistenFn | null = null;
  private ready = false;

  constructor(
    private linkId: string,
    private root: string,
    // diagnostics for one project file, replacing its earlier ones
    private onDiagnostics: (path: string, list: Diagnostic[]) => void,
  ) {}

  private uri(path: string) {
    return toUri(`${this.root.replace(/\\/g, "/").replace(/\/$/, "")}/${path}`);
  }

  /** Starts texlab for the project. False when it isn't installed. */
  async start(): Promise<boolean> {
    const how = await invoke<"started" | "running" | "missing">("lsp_start", { linkId: this.linkId });
    if (how === "missing") return false;
    this.unlisten = await listen<{ linkId: string; message: { method: string; params: unknown } }>("lsp", (e) => {
      if (e.payload.linkId !== this.linkId) return;
      const { method, params } = e.payload.message;
      if (method === "textDocument/publishDiagnostics") {
        const p = params as { uri: string; diagnostics: LspDiagnostic[] };
        const path = fromUri(p.uri, this.root);
        if (!path) return;
        this.onDiagnostics(
          path,
          p.diagnostics.map((d) => ({
            severity: d.severity === 1 ? "error" : d.severity === 2 ? "warning" : "badbox",
            file: path,
            line: d.range.start.line + 1,
            message: d.message,
          })),
        );
      }
    });
    // a server another editor already set up is ready; texlab refuses a
    // second initialize
    if (how === "started") {
      await this.request("initialize", {
        processId: null,
        rootUri: toUri(this.root),
        workspaceFolders: [{ uri: toUri(this.root), name: "project" }],
        capabilities: {
          textDocument: {
            synchronization: { didSave: true },
            completion: { completionItem: { snippetSupport: false } },
            hover: { contentFormat: ["plaintext", "markdown"] },
            definition: {},
            publishDiagnostics: {},
          },
          workspace: { configuration: true, workspaceFolders: true },
        },
      });
      await this.notify("initialized", {});
    }
    this.ready = true;
    return true;
  }

  stop() {
    this.unlisten?.();
    this.ready = false;
    void invoke("lsp_stop", { linkId: this.linkId }).catch(() => {});
  }

  private request<T>(method: string, params: unknown): Promise<T> {
    return invoke<T>("lsp_request", { linkId: this.linkId, method, params });
  }

  private notify(method: string, params: unknown): Promise<void> {
    return invoke<void>("lsp_notify", { linkId: this.linkId, method, params });
  }

  open(path: string, text: string) {
    if (!this.ready) return;
    this.versions.set(path, 1);
    const languageId = path.endsWith(".bib") ? "bibtex" : "latex";
    void this.notify("textDocument/didOpen", { textDocument: { uri: this.uri(path), languageId, version: 1, text } }).catch(() => {});
  }

  // resolves once the server has the text, so a request sent after it sees it
  change(path: string, text: string): Promise<void> {
    if (!this.ready || !this.versions.has(path)) return Promise.resolve();
    const v = (this.versions.get(path) ?? 1) + 1;
    this.versions.set(path, v);
    return this.notify("textDocument/didChange", {
      textDocument: { uri: this.uri(path), version: v },
      contentChanges: [{ text }],
    }).catch(() => {});
  }

  saved(path: string) {
    if (!this.ready || !this.versions.has(path)) return;
    void this.notify("textDocument/didSave", { textDocument: { uri: this.uri(path) } }).catch(() => {});
  }

  close(path: string) {
    if (!this.ready || !this.versions.has(path)) return;
    this.versions.delete(path);
    void this.notify("textDocument/didClose", { textDocument: { uri: this.uri(path) } }).catch(() => {});
  }

  async completion(path: string, line: number, character: number): Promise<CompletionItem[]> {
    if (!this.ready) return [];
    const r = await this.request<CompletionItem[] | { items: CompletionItem[] } | null>("textDocument/completion", {
      textDocument: { uri: this.uri(path) },
      position: { line, character },
    }).catch(() => null);
    if (!r) return [];
    return Array.isArray(r) ? r : r.items;
  }

  async hover(path: string, line: number, character: number): Promise<string | null> {
    if (!this.ready) return null;
    const r = await this.request<{ contents: string | { value: string } | ({ value: string } | string)[] } | null>(
      "textDocument/hover",
      { textDocument: { uri: this.uri(path) }, position: { line, character } },
    ).catch(() => null);
    if (!r) return null;
    const c = r.contents;
    const text = (x: string | { value: string }) => (typeof x === "string" ? x : x.value);
    return Array.isArray(c) ? c.map(text).join("\n\n") : text(c);
  }

  async definition(path: string, line: number, character: number): Promise<{ path: string; line: number } | null> {
    if (!this.ready) return null;
    type Loc = { uri: string; range: Range } | { targetUri: string; targetSelectionRange: Range };
    const r = await this.request<Loc | Loc[] | null>("textDocument/definition", {
      textDocument: { uri: this.uri(path) },
      position: { line, character },
    }).catch(() => null);
    const first = Array.isArray(r) ? r[0] : r;
    if (!first) return null;
    const uri = "uri" in first ? first.uri : first.targetUri;
    const range = "range" in first ? first.range : first.targetSelectionRange;
    const p = fromUri(uri, this.root);
    return p ? { path: p, line: range.start.line + 1 } : null;
  }
}
