<script lang="ts">
  // Keep awake, a cup in the top strip: one click holds the computer awake
  // while the user steps away (the core ends it when they're back, when no
  // chat is working, or after two hours). While on it is lit and shows the
  // time left; clicking it then offers stop, or overnight after a check of
  // what it means.
  import type { AppState } from "./state/app.svelte";
  import { keepAwakeStart, keepAwakeStop, onBattery } from "./ipc";
  import { loadPrefs } from "./themes";
  import BarIcon from "./BarIcon.svelte";

  let {
    app,
    short = false,
  }: {
    app: AppState;
    /** Just the cup, the time left only on hover: the sidebar is put away and
     *  the top bar has room for the cup alone. */
    short?: boolean;
  } = $props();

  let open = $state(false);
  /** The menu shows the overnight check instead of its choices. */
  let confirming = $state(false);
  /** Overnight can't start: on battery. Asked each time the menu opens. */
  let battery = $state(false);
  let problem = $state("");
  let box = $state<HTMLElement>();

  // The time left, updated while on: minutes are enough, so twice a minute.
  let now = $state(Date.now());
  $effect(() => {
    if (!app.keepAwake) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(timer);
  });
  // Closes the menu when it ends while open.
  $effect(() => {
    if (!app.keepAwake) open = false;
  });

  const timeLeft = $derived.by(() => {
    const on = app.keepAwake;
    if (!on) return "";
    return on.mode === "overnight" ? `till ${clock(on.ends_at)}` : left(on.ends_at - now);
  });

  const hover = $derived(app.keepAwake ? `Keeping awake: ${timeLeft}` : "Keep awake");

  /** "1h 52m", "2h", "40m". */
  function left(ms: number): string {
    const m = Math.max(1, Math.ceil(ms / 60_000));
    if (m < 60) return `${m}m`;
    return m % 60 ? `${Math.floor(m / 60)}h ${m % 60}m` : `${m / 60}h`;
  }

  /** "8:00", local time. */
  function clock(ms: number): string {
    const d = new Date(ms);
    return `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
  }

  /** The next time the clock shows "HH:MM": today if still ahead, else tomorrow. */
  function nextAt(hhmm: string): number {
    const [h, m] = hhmm.split(":").map(Number);
    const d = new Date();
    d.setHours(h, m, 0, 0);
    if (d.getTime() <= Date.now()) d.setDate(d.getDate() + 1);
    return d.getTime();
  }

  const until = $derived(open ? nextAt(loadPrefs().overnightUntil) : 0);
  const waiting = $derived(confirming ? app.waitingChats() : []);

  async function click() {
    if (!app.keepAwake) {
      try {
        app.keepAwake = await keepAwakeStart("away");
      } catch (e) {
        app.problem = String(e);
      }
      return;
    }
    open = !open;
    confirming = false;
    problem = "";
    if (open) battery = await onBattery().catch(() => false);
  }

  async function stop() {
    open = false;
    await keepAwakeStop();
    app.keepAwake = null;
  }

  async function overnight() {
    try {
      app.keepAwake = await keepAwakeStart("overnight", until);
      open = false;
    } catch (e) {
      problem = String(e);
    }
  }

  // Any click outside the menu, or Escape, closes it.
  $effect(() => {
    if (!open || !box) return;
    const menu = box;
    const away = (e: PointerEvent) => {
      if (!menu.contains(e.target as Node)) open = false;
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key === "Escape") open = false;
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", escape);
    };
  });
</script>

<span class="picker" bind:this={box}>
  <button
    class:on={app.keepAwake}
    class:open
    title={hover}
    aria-label={hover}
    aria-haspopup={app.keepAwake ? "menu" : undefined}
    aria-expanded={app.keepAwake ? open : undefined}
    onclick={click}
    ><BarIcon name="awake" />{#if timeLeft && !short}<span>{timeLeft}</span>{/if}</button
  >
  {#if open}
    {#if confirming}
      <div class="menu ask" role="alertdialog">
        <span class="question">stay awake until {clock(until)}</span>
        {#if waiting.length}<span class="waiting">waiting on you: {waiting.join(", ")}</span>{/if}
        {#if problem}<span class="problem">{problem}</span>{/if}
        <span class="answers">
          <button class="start" onclick={overnight}>start</button>
          <button onclick={() => (confirming = false)}>cancel</button>
        </span>
      </div>
    {:else}
      <div class="menu" role="menu">
        <button role="menuitem" onclick={stop}>stop</button>
        {#if app.keepAwake?.mode !== "overnight"}
          {#if battery}
            <button role="menuitem" disabled>overnight · plug in first</button>
          {:else}
            <button role="menuitem" onclick={() => (confirming = true)}>overnight until {clock(until)}</button>
          {/if}
        {/if}
      </div>
    {/if}
  {/if}
</span>

<style>
  .picker {
    position: relative;
  }
  .picker > button {
    display: flex;
    align-items: center;
    gap: 0.8ch;
    white-space: nowrap;
    color: var(--faint);
  }
  .picker > button:hover,
  .picker > button.on {
    color: var(--fg);
  }
  .picker > button.open {
    color: var(--bright);
  }
  /* The cup half a pixel up: its weight sits low under the thin steam, so on
     the toggle's line it looked lower than the box beside it. */
  .picker > button :global(svg) {
    position: relative;
    top: -0.5px;
  }
  .menu {
    position: absolute;
    left: 0;
    top: calc(100% + var(--gap) * 0.4);
    z-index: 10;
    display: flex;
    flex-direction: column;
    padding: calc(var(--gap) * 0.4) 0;
    background: var(--panel);
    border: 1px solid var(--line);
    white-space: nowrap;
  }
  .menu button {
    text-align: left;
    padding: 0 2ch;
    color: var(--dim);
  }
  .menu button:hover:not(:disabled) {
    color: var(--fg);
  }
  .menu button:disabled {
    color: var(--faint);
  }
  /* The overnight check: what it means, then start or cancel. */
  .ask {
    padding: calc(var(--gap) * 0.5) 2ch;
    gap: calc(var(--gap) * 0.4);
    white-space: normal;
    width: max-content;
    max-width: 40ch;
  }
  .ask button {
    padding: 0;
  }
  .question {
    color: var(--fg);
  }
  .waiting {
    color: var(--yellow);
  }
  .problem {
    color: var(--bad);
  }
  .answers {
    display: flex;
    gap: 3ch;
  }
  .ask .start {
    color: var(--bright);
  }
</style>
