<script lang="ts">
  // A chat's name edited in place, in the sidebar and the top bar: the name's
  // own text with a thin underline, none of the system's field box or blue
  // focus ring. Opens with the whole name selected. Enter or clicking away
  // keeps the text; Escape drops it.
  let {
    value,
    done,
    class: extra = "",
  }: {
    value: string;
    /** Called once: whether to keep the text, and the text. */
    done: (save: boolean, text: string) => void;
    class?: string;
  } = $props();

  // svelte-ignore state_referenced_locally
  let draft = $state(value);
  let finished = false;

  function finish(save: boolean) {
    // Enter, then the blur as the box goes away: only the first counts.
    if (finished) return;
    finished = true;
    done(save, draft);
  }

  // A one-line box doesn't scroll sideways on a trackpad or mouse wheel by
  // itself, so a long name's start was out of reach. Either direction moves
  // it. Added by hand: Svelte's own wheel listeners can't stop the page scrolling.
  function setup(input: HTMLInputElement) {
    input.focus();
    input.select();
    const scroll = (e: WheelEvent) => {
      const by = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      if (!by || input.scrollWidth <= input.clientWidth) return;
      e.preventDefault();
      input.scrollLeft += by;
    };
    input.addEventListener("wheel", scroll, { passive: false });
    return { destroy: () => input.removeEventListener("wheel", scroll) };
  }
</script>

<input
  class="rename {extra}"
  use:setup
  bind:value={draft}
  onblur={() => finish(true)}
  onkeydown={(e) => {
    if (e.key === "Enter") finish(true);
    else if (e.key === "Escape") {
      e.preventDefault();
      finish(false);
    }
  }}
/>

<style>
  .rename {
    flex: 1;
    min-width: 0;
    color: var(--bright);
    background: none;
    border: 0;
    border-bottom: 1px solid var(--dim);
    border-radius: 0;
    padding: 0;
    margin: 0;
    outline: 0;
    appearance: none;
    line-height: inherit;
    /* Bars around it may turn off selecting; a name being edited needs it. */
    user-select: text;
    -webkit-user-select: text;
  }
</style>
