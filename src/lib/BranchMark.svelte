<script lang="ts">
  // A branch mark for a chat, coloured by its pull requests taken together:
  // green while any is open, dim while only drafts are left, purple once the
  // work has merged, red if every one was closed. Drawn rather than a font
  // glyph, so it looks the same in every font.
  import type { PrInfo } from "./ipc";
  import { prSummary } from "./paths";

  let { prs }: { prs: PrInfo[] } = $props();
  const state = $derived(prSummary(prs) ?? "none");
</script>

<svg class="mark {state}" viewBox="0 0 16 16" aria-hidden="true">
  <circle cx="5" cy="3.5" r="1.7" />
  <circle cx="5" cy="12.5" r="1.7" />
  <circle cx="11.5" cy="4.5" r="1.7" />
  <path d="M5 5.2v5.6M11.5 6.2c0 3-2.6 3.6-5.2 4.8" />
</svg>

<style>
  .mark {
    width: 1em;
    height: 1em;
    flex-shrink: 0;
    vertical-align: -0.15em;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    color: var(--faint);
  }
  .draft {
    color: var(--dim);
  }
  .open {
    color: var(--ok);
  }
  .merged {
    color: var(--purple);
  }
  .closed {
    color: var(--bad);
  }
</style>
