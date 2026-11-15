<script lang="ts">
  // One of the chat's images, drawn over the chat and side panel the way
  // settings are, sized to fit. Clicking the picture switches between that and
  // its actual pixels (one image pixel per screen pixel), to make out detail.
  // ← and → step through the chat's images; Escape, × or a click beside the
  // picture closes it. The picture is read from the core only while shown.
  import { tick } from "svelte";
  import { readImage } from "./ipc";
  import type { Chat } from "./state/chat.svelte";

  let { chat }: { chat: Chat } = $props();

  const image = $derived(chat.viewing!);
  const list = $derived(chat.allImages);
  const at = $derived(list.findIndex((i) => i.n === image.n));

  let url = $state<string | null>(null);
  let failed = $state(false);
  let size = $state<{ w: number; h: number } | null>(null);
  let actual = $state(false);
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
    actual = false;
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

  async function toggle(e: MouseEvent) {
    if (!img || (!shrunk && !actual)) return;
    // Zooming in keeps the clicked spot in view.
    const fx = e.offsetX / img.clientWidth;
    const fy = e.offsetY / img.clientHeight;
    actual = !actual;
    await tick();
    if (actual) {
      stage.scrollLeft = img.offsetLeft + fx * img.clientWidth - stage.clientWidth / 2;
      stage.scrollTop = img.offsetTop + fy * img.clientHeight - stage.clientHeight / 2;
    }
    measure();
  }

  function step(by: number) {
    const next = list[at + by];
    if (next) chat.viewing = next;
  }

  const close = () => (chat.viewing = null);

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
    else if (e.key === "ArrowLeft") step(-1);
    else if (e.key === "ArrowRight") step(1);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  const label = $derived(
    [`image ${image.n}`, failed ? "couldn't load" : size ? `${size.w}×${size.h}` : "", actual ? "actual pixels" : ""]
      .filter(Boolean)
      .join(" · "),
  );
</script>

<svelte:window onresize={measure} />

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="viewer" bind:this={root} tabindex="-1" onkeydown={onKey} role="dialog" aria-label="image {image.n}">
  <div class="bar">
    <span class="label">{label}</span>
    {#if list.length > 1}
      <span class="steps">
        <button disabled={at <= 0} title="Previous image (←)" onclick={() => step(-1)}>‹</button>
        <span>{at + 1} of {list.length}</span>
        <button disabled={at >= list.length - 1} title="Next image (→)" onclick={() => step(1)}>›</button>
      </span>
    {/if}
    <button class="close" title="Close (Esc)" onclick={close}>×</button>
  </div>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="stage" class:actual bind:this={stage} onclick={(e) => e.target === stage && close()}>
    {#if url}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <img
        bind:this={img}
        src={url}
        alt="image {image.n}"
        class:zoom-in={shrunk}
        class:zoom-out={actual}
        style:width={actual ? `${actualWidth()}px` : null}
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
  .close {
    margin-left: auto;
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
  }
  .zoom-in {
    cursor: zoom-in;
  }
  .zoom-out {
    cursor: zoom-out;
  }
</style>
