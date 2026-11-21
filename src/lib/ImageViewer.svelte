<script lang="ts">
  // One of the chat's images, drawn over the top bar, chat and side panel
  // the way settings are, sized to fit. Clicking the picture switches between that and
  // its actual pixels (one image pixel per screen pixel), to make out detail. A pinch,
  // ⌘-scroll or + and - zoom freely, about the pointer; 0 fits it again.
  // ← and → step through the chat's images; Escape, close or a click beside the
  // picture closes it. The picture is read from the core only while shown.
  import { flushSync } from "svelte";
  import BarIcon from "./BarIcon.svelte";
  import { readImage } from "./ipc";
  import type { Chat } from "./state/chat.svelte";

  let { chat }: { chat: Chat } = $props();

  const image = $derived(chat.viewing!);
  const list = $derived(chat.allImages);
  const at = $derived(list.findIndex((i) => i.n === image.n));

  let url = $state<string | null>(null);
  let failed = $state(false);
  let size = $state<{ w: number; h: number } | null>(null);
  /** Null when fitted; else the size, where 1 is one image pixel per screen pixel. */
  let zoom = $state<number | null>(null);
  const actual = $derived(zoom !== null);
  /** Fitted, the picture is drawn smaller than its actual pixels. */
  let shrunk = $state(false);

  let root: HTMLElement;
  let stage: HTMLElement;
  let img = $state<HTMLImageElement>();

  $effect(() => {
    const { n, media_type } = image;
    const sessionId = chat.sessionId;
    url = null;
    failed = false;
    size = null;
    zoom = null;
    let made: string | null = null;
    let gone = false;
    readImage(sessionId, n).then(
      (bytes) => {
        if (gone) return;
        made = URL.createObjectURL(new Blob([bytes], { type: media_type }));
        url = made;
      },
      () => {
        if (!gone) failed = true;
      },
    );
    return () => {
      gone = true;
      if (made) URL.revokeObjectURL(made);
    };
  });

  // Take the keys from the message box while open; give them back after.
  $effect(() => {
    root.focus();
    return () => document.querySelector<HTMLTextAreaElement>(".composer textarea")?.focus();
  });

  /** The picture's width at one image pixel per screen pixel. */
  const actualWidth = () => (img ? img.naturalWidth / window.devicePixelRatio : 0);

  function measure() {
    if (!img) return;
    size = { w: img.naturalWidth, h: img.naturalHeight };
    shrunk = !actual && img.clientWidth < actualWidth() - 1;
  }

  /** The fitted picture's size, in the units of `zoom`. */
  function fitZoom() {
    if (!img) return 1;
    const cs = getComputedStyle(stage);
    const w = stage.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    const h = stage.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom);
    return Math.min(w / img.naturalWidth, h / img.naturalHeight, 1) * window.devicePixelRatio;
  }

  const current = () => zoom ?? fitZoom();

  /** Zooms to `next`, keeping the spot at (x, y) in the window where it is.
      Zooming out stops at the fitted size. */
  function zoomTo(next: number, x: number, y: number) {
    if (!img) return;
    const fit = fitZoom();
    next = Math.min(next, Math.max(8, fit * 4));
    const was = img.getBoundingClientRect();
    const fx = (x - was.left) / was.width;
    const fy = (y - was.top) / was.height;
    flushSync(() => (zoom = next <= fit * 1.001 ? null : next));
    const now = img.getBoundingClientRect();
    stage.scrollLeft += now.left + fx * now.width - x;
    stage.scrollTop += now.top + fy * now.height - y;
    measure();
  }

  /** Zooms by a factor about the middle of the view, for the keys. */
  function zoomBy(factor: number) {
    const r = stage.getBoundingClientRect();
    zoomTo(current() * factor, r.left + r.width / 2, r.top + r.height / 2);
  }

  function toggle(e: MouseEvent) {
    if (!img || (!shrunk && !actual)) return;
    if (actual) fit();
    else zoomTo(1, e.clientX, e.clientY);
  }

  function fit() {
    flushSync(() => (zoom = null));
    measure();
  }

  // A trackpad pinch arrives as a wheel with ctrl held (and ⌘-scroll is the
  // mouse's way); plain scrolling pans a zoomed picture.
  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    zoomTo(current() * Math.exp(-e.deltaY * 0.01), e.clientX, e.clientY);
  }

  // WebKit (the Mac app) reports a pinch as its own gesture events instead.
  $effect(() => {
    let from = 1;
    const start = (e: Event) => {
      e.preventDefault();
      from = current();
    };
    const change = (e: Event) => {
      e.preventDefault();
      const g = e as Event & { scale: number; clientX: number; clientY: number };
      zoomTo(from * g.scale, g.clientX, g.clientY);
    };
    stage.addEventListener("gesturestart", start);
    stage.addEventListener("gesturechange", change);
    return () => {
      stage.removeEventListener("gesturestart", start);
      stage.removeEventListener("gesturechange", change);
    };
  });

  function step(by: number) {
    const next = list[at + by];
    if (next) chat.viewing = next;
  }

  const close = () => (chat.viewing = null);

  // On the window, not the viewer, so the keys work wherever focus went (the
  // app gives it back to the message box when the window comes back to the
  // front). A box still being typed in (the sidebar's rename) keeps them.
  function onKey(e: KeyboardEvent) {
    const el = document.activeElement as HTMLElement | null;
    if (e.defaultPrevented || (el && (el.tagName === "INPUT" || el.isContentEditable))) return;
    if (e.key === "Escape") close();
    else if (e.key === "ArrowLeft") step(-1);
    else if (e.key === "ArrowRight") step(1);
    else if (e.key === "=" || e.key === "+") zoomBy(1.25);
    else if (e.key === "-") zoomBy(0.8);
    else if (e.key === "0") fit();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  const label = $derived(
    [`image ${image.n}`, failed ? "couldn't load" : size ? `${size.w}×${size.h}` : "", zoom === 1 ? "actual pixels" : zoom ? `${Math.round(zoom * 100)}%` : ""]
      .filter(Boolean)
      .join(" · "),
  );
</script>

<svelte:window onresize={measure} onkeydown={onKey} />

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="viewer" bind:this={root} tabindex="-1" role="dialog" aria-label="image {image.n}">
  <!-- It covers the title bar's strip, so its empty space moves the window. -->
  <div class="bar" data-tauri-drag-region>
    <span class="label">{label}</span>
    {#if list.length > 1}
      <span class="steps">
        <button disabled={at <= 0} title="Previous (←)" onclick={() => step(-1)}>‹</button>
        <span>{at + 1} of {list.length}</span>
        <button disabled={at >= list.length - 1} title="Next (→)" onclick={() => step(1)}>›</button>
      </span>
    {/if}
    <button class="close" title="Close (Esc)" onclick={close}><BarIcon name="close" /></button>
  </div>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="stage" class:actual bind:this={stage} onwheel={onWheel} onclick={(e) => e.target === stage && close()}>
    {#if url}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <img
        bind:this={img}
        src={url}
        alt="image {image.n}"
        class:zoom-in={shrunk}
        class:zoom-out={actual}
        style:width={zoom ? `${actualWidth() * zoom}px` : null}
        onload={measure}
        onclick={toggle}
      />
    {/if}
  </div>
</div>

<style>
  .viewer {
    position: absolute;
    inset: 0;
    /* Above the side panel's drag edge, like settings. */
    z-index: 2;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    outline: none;
  }
  .bar {
    display: flex;
    gap: 3ch;
    padding: var(--pad-y) var(--pad-x) var(--gap);
    color: var(--dim);
    white-space: nowrap;
  }
  /* Clear of the sidebar toggle, which sits over the top-left corner. */
  :global(.bare) .bar {
    padding-left: calc(var(--toggle-x) + 2em + 2ch + var(--pad-x) * 0.8);
  }
  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .steps {
    display: flex;
    gap: 1.5ch;
    color: var(--faint);
  }
  .steps button:disabled {
    color: var(--faint);
    opacity: 0.4;
  }
  .steps button:not(:disabled):hover,
  .close:hover {
    color: var(--fg);
  }
  /* The panel's close icon, so every close in the app looks the same. */
  .close {
    margin-left: auto;
    display: flex;
    align-items: center;
  }
  /* Auto margins centre the picture, yet let a large one scroll from its edge. */
  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    overflow: hidden;
    padding: 0 var(--pad-x) var(--pad-y);
  }
  .stage.actual {
    overflow: auto;
  }
  img {
    margin: auto;
    display: block;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border: 1px solid var(--line);
    box-sizing: border-box;
  }
  .actual img {
    max-width: none;
    max-height: none;
    height: auto;
    /* Else the row squeezes a picture wider than the view back down. */
    flex-shrink: 0;
  }
  .zoom-in {
    cursor: zoom-in;
  }
  .zoom-out {
    cursor: zoom-out;
  }
</style>
