<script lang="ts" module>
  export interface PdfViewApi {
    // scroll to a spot and flash it (SyncTeX forward search)
    show(spot: { page: number; x: number; y: number; width: number; height: number }): void;
  }
</script>

<script lang="ts">
  import * as pdfjs from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import { onDestroy } from "svelte";

  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

  let {
    data,
    onpick,
    api = $bindable(),
  }: {
    // the PDF's bytes; a new buffer re-renders, keeping the scroll position
    data: ArrayBuffer | null;
    // a double-click on a page, in PDF points from the page's top-left
    onpick?: (page: number, x: number, y: number) => void;
    api?: PdfViewApi;
  } = $props();

  let scroller: HTMLDivElement | undefined = $state();
  let pagesEl: HTMLDivElement | undefined = $state();
  let doc: pdfjs.PDFDocumentProxy | null = null;
  // what loaded `doc`; destroying it frees the document and its worker side
  let task: pdfjs.PDFDocumentLoadingTask | null = null;
  let zoom = $state(1);
  let error = $state<string | null>(null);
  let pageCount = $state(0);
  // CSS pixels per PDF point for the current layout
  let scale = 1;
  let observer: IntersectionObserver | null = null;
  let loadToken = 0;

  async function load(bytes: ArrayBuffer) {
    const token = ++loadToken;
    error = null;
    // remember where we were, as a fraction of the document
    const keep = scroller && scroller.scrollHeight > 0 ? scroller.scrollTop / scroller.scrollHeight : 0;
    try {
      // pdf.js takes ownership of the buffer it's given; hand it a copy
      const nextTask = pdfjs.getDocument({ data: new Uint8Array(bytes.slice(0)) });
      const next = await nextTask.promise;
      if (token !== loadToken) {
        void nextTask.destroy();
        return;
      }
      const old = task;
      task = nextTask;
      doc = next;
      pageCount = next.numPages;
      await layout();
      if (scroller) scroller.scrollTop = keep * scroller.scrollHeight;
      void old?.destroy();
    } catch (e) {
      error = `Couldn't show the PDF: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  // one placeholder per page at the right size; pages draw when they scroll into view
  // a newer layout (resize, zoom, new PDF) makes an older one stop
  let layoutGen = 0;
  async function layout() {
    if (!doc || !pagesEl || !scroller) return;
    const gen = ++layoutGen;
    observer?.disconnect();
    // free the old pages' canvases now rather than whenever GC gets to them
    for (const c of pagesEl.querySelectorAll("canvas")) {
      c.width = 0;
      c.height = 0;
    }
    pagesEl.replaceChildren();
    const first = await doc.getPage(1);
    if (gen !== layoutGen) return;
    const base = first.getViewport({ scale: 1 });
    const width = Math.max(200, scroller.clientWidth - 32);
    scale = (width / base.width) * zoom;
    observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) void draw(e.target as HTMLDivElement);
        }
      },
      { root: scroller, rootMargin: "600px 0px" },
    );
    for (let n = 1; n <= doc.numPages; n++) {
      const page = n === 1 ? first : await doc.getPage(n);
      if (gen !== layoutGen) return;
      const vp = page.getViewport({ scale });
      const div = document.createElement("div");
      div.className = "page";
      div.dataset.page = String(n);
      div.style.width = `${vp.width}px`;
      div.style.height = `${vp.height}px`;
      div.addEventListener("dblclick", (ev) => {
        const r = div.getBoundingClientRect();
        onpick?.(n, (ev.clientX - r.left) / scale, (ev.clientY - r.top) / scale);
      });
      pagesEl.appendChild(div);
      observer.observe(div);
    }
  }

  async function draw(div: HTMLDivElement) {
    if (!doc || div.dataset.drawn === String(scale)) return;
    div.dataset.drawn = String(scale);
    const page = await doc.getPage(Number(div.dataset.page));
    const vp = page.getViewport({ scale });
    const dpr = window.devicePixelRatio || 1;
    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(vp.width * dpr);
    canvas.height = Math.floor(vp.height * dpr);
    canvas.style.width = `${vp.width}px`;
    canvas.style.height = `${vp.height}px`;
    await page.render({ canvas, viewport: vp, transform: dpr === 1 ? undefined : [dpr, 0, 0, dpr, 0, 0] }).promise;
    div.querySelector("canvas")?.remove();
    div.prepend(canvas);
  }

  $effect(() => {
    if (data) void load(data);
  });

  // re-lay out when the pane is resized or the zoom changes
  $effect(() => {
    void zoom;
    if (!scroller) return;
    let width = scroller.clientWidth;
    const ro = new ResizeObserver(() => {
      if (scroller && Math.abs(scroller.clientWidth - width) > 24) {
        width = scroller.clientWidth;
        const keep = scroller.scrollTop / Math.max(1, scroller.scrollHeight);
        void layout().then(() => {
          if (scroller) scroller.scrollTop = keep * scroller.scrollHeight;
        });
      }
    });
    ro.observe(scroller);
    void layout();
    return () => ro.disconnect();
  });

  api = {
    show(spot) {
      const div = pagesEl?.querySelector<HTMLDivElement>(`.page[data-page="${spot.page}"]`);
      if (!div || !scroller) return;
      const mark = document.createElement("div");
      mark.className = "mark";
      mark.style.left = `${spot.x * scale - 4}px`;
      mark.style.top = `${spot.y * scale - 3}px`;
      mark.style.width = `${Math.max(spot.width * scale, 12) + 8}px`;
      mark.style.height = `${Math.max(spot.height * scale, 10) + 6}px`;
      div.appendChild(mark);
      setTimeout(() => mark.remove(), 1800);
      const top = div.offsetTop + spot.y * scale - scroller.clientHeight / 3;
      scroller.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
    },
  };

  onDestroy(() => {
    observer?.disconnect();
    void task?.destroy();
  });
</script>

<div class="pdf">
  <div class="bar">
    <span class="muted">{pageCount ? `${pageCount} page${pageCount === 1 ? "" : "s"}` : ""}</span>
    <span class="spacer"></span>
    <button class="icon" title="Zoom out" aria-label="Zoom out" onclick={() => (zoom = Math.max(0.4, zoom - 0.15))}>−</button>
    <button class="ghost zoom" title="Fit width" onclick={() => (zoom = 1)}>{Math.round(zoom * 100)}%</button>
    <button class="icon" title="Zoom in" aria-label="Zoom in" onclick={() => (zoom = Math.min(3, zoom + 0.15))}>+</button>
  </div>
  <div class="scroller" bind:this={scroller}>
    {#if error}
      <p class="error">{error}</p>
    {:else if !data}
      <p class="empty">Compile (Ctrl+Enter) to see the PDF here. Double-click the PDF to jump to the source.</p>
    {/if}
    <div class="pages" bind:this={pagesEl}></div>
  </div>
</div>

<style>
  .pdf {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface-sunken);
  }
  .bar {
    flex: none;
    height: 34px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-raised);
    font-size: 12px;
  }
  .spacer {
    flex: 1;
  }
  .muted {
    color: var(--text-faint);
  }
  .icon {
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 16px;
  }
  .icon:hover {
    background: var(--hover);
  }
  .zoom {
    padding: 2px 8px;
    font-size: 12px;
    min-width: 54px;
  }
  .scroller {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 16px;
  }
  .pages {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
  }
  .pages :global(.page) {
    position: relative;
    background: #fff;
    box-shadow: var(--shadow-sm);
  }
  .pages :global(.page canvas) {
    display: block;
  }
  .pages :global(.mark) {
    position: absolute;
    border-radius: 4px;
    background: rgba(79, 191, 130, 0.28);
    outline: 2px solid rgba(47, 143, 91, 0.7);
    pointer-events: none;
    animation: fade 1.8s ease-out forwards;
  }
  @keyframes fade {
    0%,
    60% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }
  .empty,
  .error {
    text-align: center;
    color: var(--text-faint);
    font-size: 13px;
    margin: 12vh 24px;
  }
  .error {
    color: var(--danger);
  }
</style>
