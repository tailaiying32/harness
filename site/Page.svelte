<script lang="ts">
  // Single-screen landing page for Tilde: large shade-block `~/` mark,
  // rolling ASCII contour background, understated one-liner, Download + GitHub,
  // and live 20-theme picker.
  import { markSvg } from "../src/lib/appIcon";
  import ThemeMenu from "./ThemeMenu.svelte";
  import HeroField from "./HeroField.svelte";
  import HeroMark from "./HeroMark.svelte";
  import { DOWNLOAD, GITHUB, header } from "./copy";

  const mark = markSvg("currentColor");
  const apple =
    '<svg viewBox="0 0 814 1000" aria-hidden="true"><path fill="currentColor" d="M788.1 340.9c-5.8 4.5-108.2 62.2-108.2 190.5 0 148.4 130.3 200.9 134.2 202.2-.6 3.2-20.7 71.9-68.7 141.9-42.8 61.6-87.5 123.1-155.5 123.1s-85.5-39.5-164-39.5c-76.5 0-103.7 40.8-165.9 40.8s-105.6-57-155.5-127C46.7 790.7 0 663 0 541.8c0-194.4 126.4-297.5 250.8-297.5 66.1 0 121.2 43.4 162.7 43.4 39.5 0 101.1-46 176.3-46 28.5 0 130.9 2.6 198.3 99.2zm-234-181.5c31.1-36.9 53.1-88.1 53.1-139.3 0-7.1-.6-14.3-1.9-20.1-50.6 1.9-110.8 33.7-147.1 75.8-28.5 32.4-55.1 83.6-55.1 135.5 0 7.8 1.3 15.6 1.9 18.1 3.2.6 8.4 1.3 13.6 1.3 45.4 0 102.5-30.4 135.5-71.3z"/></svg>';
  const github =
    '<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.56 0-.28-.01-1.02-.02-2-3.2.7-3.88-1.54-3.88-1.54-.52-1.33-1.28-1.69-1.28-1.69-1.05-.72.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.77 2.7 1.26 3.36.96.1-.75.4-1.26.73-1.55-2.55-.29-5.24-1.28-5.24-5.68 0-1.25.45-2.28 1.18-3.08-.12-.29-.51-1.46.11-3.04 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.78 0c2.2-1.49 3.17-1.18 3.17-1.18.62 1.58.23 2.75.11 3.04.74.8 1.18 1.83 1.18 3.08 0 4.41-2.69 5.39-5.26 5.67.41.36.78 1.06.78 2.14 0 1.55-.01 2.79-.01 3.17 0 .31.21.68.8.56A11.51 11.51 0 0 0 23.5 12C23.5 5.65 18.35.5 12 .5Z"/></svg>';

  let heroReady = $state(
    typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  let introEl = $state<HTMLElement | undefined>(undefined);
</script>

<header class="bar">
  <div class="bar-inner">
    <a class="brand" href="/"><span class="mark">{@html mark}</span>tilde</a>
    <nav>
      <ThemeMenu quiet />
    </nav>
  </div>
</header>

<main class="intro" bind:this={introEl}>
  <HeroField host={introEl} />
  <div class="intro-inner">
    <HeroMark onReady={() => (heroReady = true)} />
    <div class="hero-copy" class:ready={heroReady}>
      <h1 class="opener">{header.opener}</h1>
      <div class="actions">
        <a class="primary" href={DOWNLOAD}><span class="btn-icon apple">{@html apple}</span><span>{header.download}</span></a>
        <a class="secondary" href={GITHUB}><span class="btn-icon gh">{@html github}</span><span>{header.github}</span></a>
      </div>
    </div>
  </div>
</main>

<style>
  :global(body) {
    --site-max: 1560px;
    --site-edge: clamp(18px, 2.4vw, 34px);
  }

  .bar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 30;
    background: transparent;
  }
  .bar-inner {
    box-sizing: border-box;
    max-width: calc(var(--site-max) + 2 * var(--site-edge));
    height: var(--bar);
    margin: 0 auto;
    padding: 0 var(--site-edge);
    display: flex;
    align-items: center;
    white-space: nowrap;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 1ch;
    color: var(--bright);
    font-weight: 700;
  }
  .bar nav {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 2.5ch;
    color: var(--faint);
  }

  .mark {
    display: inline-flex;
    color: var(--accent);
  }
  .mark :global(svg) {
    display: block;
    height: 0.8em;
    width: auto;
  }

  .intro {
    position: relative;
    overflow: hidden;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: calc(var(--bar) + 8px) var(--site-edge) calc(var(--bar) + 16px);
    box-sizing: border-box;
  }
  @media (pointer: coarse) {
    .intro {
      min-height: 100svh;
    }
  }
  .intro-inner {
    position: relative;
    z-index: 1;
    width: fit-content;
    max-width: 100%;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .hero-copy {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    opacity: 0;
    transform: translateY(6px);
    pointer-events: none;
    transition:
      opacity 540ms ease,
      transform 540ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .hero-copy.ready {
    opacity: 1;
    transform: translateY(0);
    pointer-events: auto;
  }
  @media (prefers-reduced-motion: reduce) {
    .hero-copy {
      opacity: 1;
      transform: none;
      pointer-events: auto;
      transition: none;
    }
  }

  .opener {
    margin: 0;
    max-width: 74ch;
    color: var(--fg);
    font-size: clamp(14px, 1.05vw, 15.5px);
    line-height: 1.55;
    font-weight: 400;
  }

  .actions {
    margin-top: 28px;
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    align-items: center;
    gap: 12px;
    font-size: clamp(14px, 1.02vw, 15px);
  }
  a.primary,
  a.secondary {
    box-sizing: border-box;
    min-height: 44px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 0 20px 0 17px;
    line-height: 1;
    white-space: nowrap;
    transition:
      border-color 160ms ease,
      color 160ms ease,
      opacity 160ms ease;
  }
  a.primary > span:last-child,
  a.secondary > span:last-child {
    transform: translateY(0.5px);
  }
  a.primary {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--bg);
    font-weight: 700;
  }
  a.primary:hover {
    opacity: 0.9;
  }
  a.secondary {
    border: 1px solid color-mix(in srgb, var(--dim) 45%, var(--line));
    color: var(--fg);
    background: color-mix(in srgb, var(--bg) 82%, transparent);
  }
  a.secondary:hover {
    border-color: var(--dim);
    color: var(--bright);
  }
  .btn-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .btn-icon.apple {
    transform: translateY(-1px);
  }
  .btn-icon.gh {
    transform: translateY(-0.5px);
  }
  .btn-icon.apple :global(svg) {
    display: block;
    height: 15px;
    width: auto;
  }
  .btn-icon.gh :global(svg) {
    display: block;
    height: 15px;
    width: 15px;
  }

  @media (max-width: 640px) {
    .opener {
      max-width: 36ch;
      text-wrap: balance;
    }
    .actions {
      margin-top: 24px;
      width: 100%;
      flex-direction: column;
      gap: 10px;
    }
    a.primary,
    a.secondary {
      width: min(260px, 100%);
    }
  }
</style>
