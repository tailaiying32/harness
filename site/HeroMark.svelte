<script lang="ts">
  // Centerpiece `~/` shade-block mark for Tilde.
  // Samples the exact `~/` geometry from src/lib/appIcon.ts onto a 64x21
  // terminal shade-block grid (· ░ ▒ ▓ █). Assembles cleanly on load and
  // responds to hover with crisp stepped shade relief (no click, no glints).
  import { onMount } from "svelte";

  let {
    onReady,
  }: {
    onReady?: () => void;
  } = $props();

  const COLS = 64;
  const ROWS = 21;

  // Matches markSvg viewBox="225 340 585 340" in src/lib/appIcon.ts.
  const VX = 215;
  const VY = 330;
  const VW = 605;
  const VH = 360;

  type Pt = { x: number; y: number; s: number };
  const WAVE_PTS: Pt[] = (() => {
    const raw: [number, number][] = [];
    const pushLine = (x0: number, y0: number, x1: number, y1: number, n: number) => {
      for (let i = 0; i < n; i++) {
        const u = i / n;
        raw.push([x0 + (x1 - x0) * u, y0 + (y1 - y0) * u]);
      }
    };
    const pushCubic = (
      x0: number, y0: number,
      x1: number, y1: number,
      x2: number, y2: number,
      x3: number, y3: number,
      n: number,
    ) => {
      for (let i = 0; i < n; i++) {
        const u = i / n;
        const m = 1 - u;
        raw.push([
          m * m * m * x0 + 3 * m * m * u * x1 + 3 * m * u * u * x2 + u * u * u * x3,
          m * m * m * y0 + 3 * m * m * u * y1 + 3 * m * u * u * y2 + u * u * u * y3,
        ]);
      }
    };
    pushLine(264, 607, 282, 550, 8);
    pushCubic(282, 550, 312, 456, 424, 450, 512, 512, 32);
    pushCubic(512, 512, 600, 574, 712, 568, 742, 474, 32);
    pushLine(742, 474, 760, 417, 9);
    raw.push([760, 417]);

    let total = 0;
    const out: Pt[] = [{ x: raw[0][0], y: raw[0][1], s: 0 }];
    for (let i = 1; i < raw.length; i++) {
      total += Math.hypot(raw[i][0] - raw[i - 1][0], raw[i][1] - raw[i - 1][1]);
      out.push({ x: raw[i][0], y: raw[i][1], s: total });
    }
    for (const p of out) p.s /= total;
    return out;
  })();

  function hitTilde(x: number, y: number): { hit: boolean; progress: number } {
    const wx = (x - 424) / 0.74 + 512;
    const wy = (y - 512) / 0.74 + 512;
    if (wx < 345 && wy > 572) return { hit: false, progress: 0 };
    if (wx > 679 && wy < 452) return { hit: false, progress: 0 };
    if (wx < 210 || wx > 815 || wy < 365 || wy > 655) return { hit: false, progress: 0 };

    let bestD2 = Infinity;
    let bestS = 0;
    for (let i = 0; i < WAVE_PTS.length; i++) {
      const p = WAVE_PTS[i];
      const d2 = (wx - p.x) ** 2 + (wy - p.y) ** 2;
      if (d2 < bestD2) {
        bestD2 = d2;
        bestS = p.s;
      }
    }
    return { hit: bestD2 <= 46 * 46, progress: bestS };
  }

  function hitSlash(x: number, y: number): { hit: boolean; progress: number } {
    if (y < 348 || y > 676) return { hit: false, progress: 0 };
    const u = (y - 348) / (676 - 348);
    const xLeft = 731 - u * (731 - 647);
    if (x < xLeft || x > xLeft + 70) return { hit: false, progress: 0 };
    return { hit: true, progress: 1 - u };
  }

  function cellHash(c: number, r: number): number {
    let n = (c * 374761393 + r * 668265263) >>> 0;
    n = ((n ^ (n >>> 13)) * 1274126177) >>> 0;
    return ((n ^ (n >>> 16)) >>> 0) / 4294967295;
  }

  type CellMeta = {
    col: number;
    row: number;
    cov: number;
    part: "none" | "tilde" | "slash";
    progress: number;
    jitter: number;
  };

  const BASE: CellMeta[] = (() => {
    const out: CellMeta[] = [];
    const cellW = VW / COLS;
    const cellH = VH / ROWS;

    for (let row = 0; row < ROWS; row++) {
      for (let col = 0; col < COLS; col++) {
        let tildeHits = 0;
        let slashHits = 0;
        let tildeS = 0;
        let slashS = 0;
        for (let sy = 0; sy < 3; sy++) {
          for (let sx = 0; sx < 3; sx++) {
            const x = VX + (col + (sx + 0.5) / 3) * cellW;
            const y = VY + (row + (sy + 0.5) / 3) * cellH;
            const sl = hitSlash(x, y);
            if (sl.hit) {
              slashHits++;
              slashS += sl.progress;
            } else {
              const ti = hitTilde(x, y);
              if (ti.hit) {
                tildeHits++;
                tildeS += ti.progress;
              }
            }
          }
        }
        const hits = tildeHits + slashHits;
        const part: CellMeta["part"] =
          slashHits > 0 && slashHits >= tildeHits
            ? "slash"
            : tildeHits > 0
              ? "tilde"
              : "none";
        const progress =
          part === "slash"
            ? slashS / slashHits
            : part === "tilde"
              ? tildeS / tildeHits
              : 0;

        out.push({
          col,
          row,
          cov: hits / 9,
          part,
          progress,
          jitter: cellHash(col, row),
        });
      }
    }
    return out;
  })();

  const SHADE_STEPS = [" ", "·", "░", "▒", "▓", "█"];
  const READY_AT = 880;
  const INTRO_END = 1350;

  type Tone = "none" | "faint" | "dim" | "accent" | "bright";
  type RenderRun = { tone: Tone; text: string };

  let rows = $state<RenderRun[][]>([]);
  let gridEl: HTMLElement | undefined;

  const reduced =
    typeof window !== "undefined"
      ? window.matchMedia("(prefers-reduced-motion: reduce)")
      : null;

  const N = COLS * ROWS;
  let introStart = 0;
  let readyFired = false;
  let pointerCol = -999;
  let pointerRow = -999;
  let pointerIn = false;

  const trail = new Float32Array(N);
  let raf = 0;

  function compress(chars: string[], tones: Tone[]): RenderRun[][] {
    const out: RenderRun[][] = [];
    for (let r = 0; r < ROWS; r++) {
      const line: RenderRun[] = [];
      let curTone: Tone = tones[r * COLS];
      let buf = chars[r * COLS];
      for (let c = 1; c < COLS; c++) {
        const idx = r * COLS + c;
        if (tones[idx] === curTone) {
          buf += chars[idx];
        } else {
          line.push({ tone: curTone, text: buf });
          curTone = tones[idx];
          buf = chars[idx];
        }
      }
      line.push({ tone: curTone, text: buf });
      out.push(line);
    }
    return out;
  }

  function renderFrame(now: number): boolean {
    const chars = new Array<string>(N);
    const tones = new Array<Tone>(N);
    const isReduced = Boolean(reduced?.matches);
    const elapsed = isReduced ? 9999 : now - introStart;

    if (!readyFired && elapsed >= READY_AT) {
      readyFired = true;
      onReady?.();
    }

    let active = elapsed < INTRO_END;

    for (let i = 0; i < N; i++) {
      if (trail[i] > 0.006) {
        trail[i] *= 0.91;
        active = true;
      } else {
        trail[i] = 0;
      }
    }

    if (pointerIn && !isReduced) {
      active = true;
      const maxR = 4.6;
      for (let i = 0; i < N; i++) {
        const cell = BASE[i];
        const dx = (cell.col - pointerCol) * 0.58;
        const dy = cell.row - pointerRow;
        const d = Math.hypot(dx, dy);
        if (d < maxR) {
          const u = 1 - d / maxR;
          const boost = u * u;
          if (boost > trail[i]) trail[i] = boost;
        }
      }
    }

    const introDone = elapsed >= INTRO_END;

    for (let i = 0; i < N; i++) {
      const cell = BASE[i];

      if (!introDone) {
        if (cell.cov > 0.14) {
          const targetStep = Math.min(5, Math.max(1, Math.round(cell.cov * 5)));
          const dotAt = (cell.col / COLS) * 240 + cell.jitter * 110;
          const climbAt =
            cell.part === "tilde"
              ? 260 + cell.progress * 440 + cell.jitter * 90
              : 760 + cell.progress * 260 + cell.jitter * 70;
          const climbDur = 270;

          if (elapsed < dotAt) {
            chars[i] = " ";
            tones[i] = "none";
          } else if (elapsed < climbAt) {
            const pre = (elapsed - dotAt) / Math.max(1, climbAt - dotAt);
            chars[i] = pre > 0.55 && targetStep >= 3 ? "░" : "·";
            tones[i] = "faint";
          } else {
            const p = Math.min(1, (elapsed - climbAt) / climbDur);
            const step = Math.max(1, Math.round(1 + (targetStep - 1) * p));
            const crest = p > 0.56 && p < 0.96 && targetStep >= 4;

            chars[i] = SHADE_STEPS[step];
            if (crest) tones[i] = "bright";
            else if (step >= 4) tones[i] = "accent";
            else if (step >= 2) tones[i] = "dim";
            else tones[i] = "faint";
          }
        } else {
          chars[i] = " ";
          tones[i] = "none";
        }
        continue;
      }

      const energy = trail[i];

      if (cell.cov > 0.14) {
        const targetStep = Math.min(5, Math.max(1, Math.round(cell.cov * 5)));
        if (energy > 0.12) {
          const dip = Math.round(energy * 3);
          const step = Math.max(1, targetStep - dip);
          chars[i] = SHADE_STEPS[step];
          tones[i] = energy > 0.55 ? "bright" : "accent";
        } else {
          chars[i] = SHADE_STEPS[targetStep];
          tones[i] = targetStep >= 4 ? "accent" : targetStep >= 2 ? "dim" : "faint";
        }
      } else if (energy > 0.34) {
        chars[i] = energy > 0.65 ? "░" : "·";
        tones[i] = "faint";
      } else {
        chars[i] = " ";
        tones[i] = "none";
      }
    }

    rows = compress(chars, tones);
    return active;
  }

  function kick() {
    if (raf) return;
    const step = (now: number) => {
      if (renderFrame(now)) {
        raf = requestAnimationFrame(step);
      } else {
        raf = 0;
      }
    };
    raf = requestAnimationFrame(step);
  }

  function place(clientX: number, clientY: number) {
    if (!gridEl) return;
    const r = gridEl.getBoundingClientRect();
    pointerCol = ((clientX - r.left) / r.width) * COLS;
    pointerRow = ((clientY - r.top) / r.height) * ROWS;
    pointerIn = true;
    kick();
  }

  // A finger is followed through touch events on the whole window (see
  // HeroField): they keep coming while the page scrolls, and a drag that
  // starts outside the mark still lights it as it passes over.
  // After a tap, phones also send a made-up mouse move that no leave ever
  // follows, which would pin the glow in place; moves just after a touch
  // are dropped.
  let lastTouch = -Infinity;
  function onMove(e: PointerEvent) {
    if (e.pointerType === "touch" || performance.now() - lastTouch < 1000) return;
    place(e.clientX, e.clientY);
  }

  function onLeave(e: PointerEvent) {
    if (e.pointerType === "touch") return;
    pointerIn = false;
    kick();
  }

  function onTouch(e: TouchEvent) {
    lastTouch = performance.now();
    const t = e.touches[0];
    if (t) {
      place(t.clientX, t.clientY);
    } else {
      pointerIn = false;
      kick();
    }
  }

  onMount(() => {
    introStart = performance.now();
    kick();
    const TOUCH = ["touchstart", "touchmove", "touchend", "touchcancel"] as const;
    for (const name of TOUCH) window.addEventListener(name, onTouch, { passive: true });
    // Leaving the window does not always send a leave to the mark itself,
    // so the page's own exit is watched too.
    const root = document.documentElement;
    const onExit = () => {
      pointerIn = false;
      kick();
    };
    const onOut = (e: PointerEvent) => {
      if (!e.relatedTarget) onExit();
    };
    root.addEventListener("mouseleave", onExit);
    document.addEventListener("pointerout", onOut);
    window.addEventListener("blur", onExit);
    return () => {
      if (raf) cancelAnimationFrame(raf);
      for (const name of TOUCH) window.removeEventListener(name, onTouch);
      root.removeEventListener("mouseleave", onExit);
      document.removeEventListener("pointerout", onOut);
      window.removeEventListener("blur", onExit);
    };
  });
</script>

<div class="hero-mark">
  <div
    class="canvas"
    bind:this={gridEl}
    role="img"
    aria-label="Tilde mark"
    onpointermove={onMove}
    onpointerleave={onLeave}
  >
    <pre class="grid" aria-hidden="true">{#each rows as line, r (r)}<span class="row">{#each line as run, k (k)}{#if run.tone === "none"}{run.text}{:else}<span class={run.tone}>{run.text}</span>{/if}{/each}</span>{/each}</pre>
  </div>
</div>

<style>
  .hero-mark {
    display: flex;
    flex-direction: column;
    align-items: center;
    margin-bottom: clamp(32px, 5.4vh, 52px);
  }
  .canvas {
    display: block;
    padding: 4px 0;
    cursor: default;
    text-align: left;
    user-select: none;
    -webkit-user-select: none;
  }
  .grid {
    margin: 0;
    font-family: var(--font-family);
    /* Generous centerpiece scale while keeping room on laptops and phones. */
    font-size: clamp(8.6px, min(1.52vw, 2.08vh), 18.5px);
    line-height: 1.04;
    letter-spacing: 0;
    white-space: pre;
    display: flex;
    flex-direction: column;
  }
  .row {
    display: block;
    height: 1.04em;
  }
  .accent {
    color: var(--accent);
  }
  .bright {
    color: var(--bright);
  }
  .dim {
    color: var(--dim);
  }
  .faint {
    color: var(--faint);
  }
</style>
