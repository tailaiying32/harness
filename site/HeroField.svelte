<script lang="ts">
  // Rolling ASCII topographic contour wave field on a crisp DPR canvas.
  // Fixed monospace grid (no bouncy physics), follows all 20 live themes,
  // and brightens cleanly under the pointer.
  import { onMount } from "svelte";

  let {
    host,
  }: {
    host?: HTMLElement;
  } = $props();

  let canvasEl: HTMLCanvasElement | undefined;

  const reduced =
    typeof window !== "undefined"
      ? window.matchMedia("(prefers-reduced-motion: reduce)")
      : null;

  type Palette = {
    faint: string;
    dim: string;
    accent: string;
    font: string;
  };

  function readPalette(): Palette {
    const cs = getComputedStyle(document.documentElement);
    return {
      faint: cs.getPropertyValue("--faint").trim() || "#666",
      dim: cs.getPropertyValue("--dim").trim() || "#888",
      accent: cs.getPropertyValue("--accent").trim() || "#d8a657",
      font: cs.getPropertyValue("--font-family").trim() || "ui-monospace, monospace",
    };
  }

  const CELL_W = 15;
  const CELL_H = 21;

  onMount(() => {
    const canvas = canvasEl;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let palette = readPalette();
    let w = 0;
    let h = 0;
    let cols = 0;
    let rows = 0;
    let dpr = 1;
    let visible = true;
    let raf = 0;
    const born = performance.now();

    let pointerX = -9999;
    let pointerY = -9999;
    let pointerIn = false;
    let stir = new Float32Array(0);

    function resize() {
      const box = (host ?? canvas!.parentElement)?.getBoundingClientRect();
      if (!box) return;
      w = Math.max(1, Math.floor(box.width));
      h = Math.max(1, Math.floor(box.height));
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas!.width = Math.floor(w * dpr);
      canvas!.height = Math.floor(h * dpr);
      cols = Math.ceil(w / CELL_W) + 1;
      rows = Math.ceil(h / CELL_H) + 1;
      stir = new Float32Array(cols * rows);
    }

    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(host ?? canvas.parentElement ?? canvas);

    const mo = new MutationObserver(() => {
      palette = readPalette();
    });
    mo.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["style", "class", "data-theme"],
    });

    const io = new IntersectionObserver(
      (entries) => {
        visible = entries.some((e) => e.isIntersecting);
        if (visible && !raf) raf = requestAnimationFrame(draw);
      },
      { threshold: 0.01 },
    );
    io.observe(host ?? canvas);

    const target = host ?? window;
    const onMove = (e: Event) => {
      const pe = e as PointerEvent;
      const rect = canvas.getBoundingClientRect();
      pointerX = pe.clientX - rect.left;
      pointerY = pe.clientY - rect.top;
      pointerIn =
        pointerX >= 0 && pointerX <= rect.width && pointerY >= 0 && pointerY <= rect.height;
    };
    const onLeave = () => {
      pointerIn = false;
    };
    target.addEventListener("pointermove", onMove, { passive: true });
    target.addEventListener("pointerleave", onLeave, { passive: true });

    function draw(now: number) {
      raf = 0;
      if (!visible || !ctx) return;

      const isReduced = Boolean(reduced?.matches);
      const t = (now - born) / 1000;

      for (let i = 0; i < stir.length; i++) {
        stir[i] = stir[i] > 0.008 ? stir[i] * 0.93 : 0;
      }

      if (pointerIn && !isReduced) {
        const pc = Math.floor(pointerX / CELL_W);
        const pr = Math.floor(pointerY / CELL_H);
        const rad = 6;
        const maxR = 5.4;
        for (let r = Math.max(0, pr - rad); r <= Math.min(rows - 1, pr + rad); r++) {
          for (let c = Math.max(0, pc - rad); c <= Math.min(cols - 1, pc + rad); c++) {
            const idx = r * cols + c;
            const d = Math.hypot((c - pc) * 0.85, r - pr);
            if (d < maxR) {
              const u = 1 - d / maxR;
              const boost = u * u;
              if (boost > stir[idx]) stir[idx] = boost;
            }
          }
        }
      }

      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      ctx.font = `12px ${palette.font}`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";

      const cx = w * 0.5;
      const cy = h * 0.48;
      const haloRx = Math.min(w * 0.43, 470);
      const haloRy = Math.min(h * 0.37, 285);
      const bootFade = isReduced ? 1 : Math.min(1, t / 0.95);

      const vertBand = Math.max(54, h * 0.065);
      const sideBand = Math.max(48, w * 0.045);

      for (let r = 0; r < rows; r++) {
        const y0 = r * CELL_H + CELL_H * 0.5;
        const edgeY = Math.min(1, y0 / vertBand) * Math.min(1, (h - y0) / vertBand);
        if (edgeY <= 0.02) continue;

        for (let c = 0; c < cols; c++) {
          const x0 = c * CELL_W + CELL_W * 0.5;
          const edgeX = Math.min(1, x0 / sideBand) * Math.min(1, (w - x0) / sideBand);
          const edge = edgeX * edgeY;
          if (edge <= 0.02) continue;

          const hdx = (x0 - cx) / haloRx;
          const hdy = (y0 - cy) / haloRy;
          const haloDist = Math.hypot(hdx, hdy);
          if (haloDist < 0.72) continue;
          const haloFade = Math.min(1, (haloDist - 0.72) / 0.45);

          const vis = edge * haloFade * bootFade;
          if (vis <= 0.02) continue;

          const idx = r * cols + c;
          const localStir = stir[idx];

          // Layered low-frequency elevation field -> crisp topographic contour bands
          const e1 = Math.sin(c * 0.085 - r * 0.115 - t * 0.42);
          const e2 = Math.cos(c * 0.055 + r * 0.095 - t * 0.28);
          const e3 = Math.sin(c * 0.04 + r * 0.07 + t * 0.19);
          const elev = isReduced ? 0.5 * (e1 + e2) : e1 * 0.48 + e2 * 0.34 + e3 * 0.18;

          const band = Math.abs((((elev + 1) * 2.2) % 1) - 0.5) * 2;

          let ch = "";
          let color = palette.faint;
          let alpha = 0;

          if (localStir > 0.14) {
            ch = localStir > 0.62 ? "~" : localStir > 0.34 ? "░" : ":";
            color = localStir > 0.5 ? palette.accent : palette.dim;
            alpha = Math.min(0.75, (0.28 + localStir * 0.46) * vis);
          } else if (band < 0.15) {
            ch = elev > 0.22 ? "~" : "░";
            color = elev > 0.34 ? palette.dim : palette.faint;
            alpha = (elev > 0.34 ? 0.58 : 0.44) * vis;
          } else if (band < 0.32) {
            ch = ":";
            color = palette.faint;
            alpha = 0.32 * vis;
          } else if (band < 0.5 && ((c + r) & 1) === 0) {
            ch = "·";
            color = palette.faint;
            alpha = 0.22 * vis;
          } else {
            continue;
          }

          ctx.globalAlpha = alpha;
          ctx.fillStyle = color;
          ctx.fillText(ch, x0, y0);
        }
      }

      if (!isReduced) {
        raf = requestAnimationFrame(draw);
      }
    }

    raf = requestAnimationFrame(draw);

    return () => {
      if (raf) cancelAnimationFrame(raf);
      ro.disconnect();
      mo.disconnect();
      io.disconnect();
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerleave", onLeave);
    };
  });
</script>

<canvas class="hero-field" bind:this={canvasEl} aria-hidden="true"></canvas>

<style>
  .hero-field {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    z-index: 0;
  }
</style>
