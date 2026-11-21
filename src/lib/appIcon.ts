// The app's icon: `~/` on a rounded tile, drawn here so one shape serves the
// icon files the build ships (scripts/make-icons.ts), the picker in Settings
// and the icon shown while the app runs. Nothing here touches the window, so
// the build script can load this file by itself.
//
// Every theme has an icon: the tile takes its background, `~/` its accent.
// Which one shows is the user's own pick, not the theme on show.

import type { Theme } from "./themes";

/** The three colours an icon takes from a theme. */
export type IconColours = Pick<Theme, "bg" | "accent" | "faint">;

export type IconOptions = {
  /** A band of hazard stripes along the bottom: a build run from source, told apart from the real app at a glance. */
  dev?: boolean;
  /** The tile fills the picture. Windows icons have no margin; a Mac's keep the margin every Dock icon has. */
  bleed?: boolean;
};

// Drawn on a 1024 square. The tile is the size and corner a Mac icon has.
const TILE = `x="100" y="100" width="824" height="824" rx="186"`;

// The tilde is a stroked wave, drawn longer than it shows and cut level at
// both ends: below the left end and above the right one. It is drawn centred
// on the square, then shrunk and moved left to make room for the slash.
const WAVE = "M 264 607 L 282 550 C 312 456, 424 450, 512 512 C 600 574, 712 568, 742 474 L 760 417";
const LEVEL_CUT = "0,0 679,0 679,452 1024,452 1024,1024 345,1024 345,572 0,572";
const TILDE_PLACE = "translate(424 512) scale(0.74) translate(-512 -512)";
// The slash, cut level top and bottom like the tilde's ends.
const SLASH = "647,676 717,676 801,348 731,348";

// The dev band: its top edge, and how far the glyph moves up to stay centred in what is left.
const BAND_TOP = 730;
const GLYPH_LIFT = 78;

function hazardBand(c: IconColours): string {
  const stripes = Array.from({ length: 6 }, (_, i) => {
    const x = 20 + i * 200;
    return `<polygon points="${x},930 ${x + 100},930 ${x + 200},${BAND_TOP} ${x + 100},${BAND_TOP}" fill="${c.bg}"/>`;
  }).join("");
  return `<g clip-path="url(#tile)"><rect x="100" y="${BAND_TOP}" width="824" height="${930 - BAND_TOP}" fill="${c.accent}"/>${stripes}</g>`;
}

/** The icon as a standalone SVG picture. */
export function iconSvg(c: IconColours, { dev = false, bleed = false }: IconOptions = {}): string {
  const glyph =
    `<g transform="${TILDE_PLACE}"><path clip-path="url(#level)" d="${WAVE}" fill="none" stroke="${c.accent}" stroke-width="92"/></g>` +
    `<polygon points="${SLASH}" fill="${c.accent}"/>`;
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="${bleed ? "100 100 824 824" : "0 0 1024 1024"}">` +
    `<defs>` +
    `<linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.05"/><stop offset="1" stop-color="#000" stop-opacity="0.14"/></linearGradient>` +
    `<clipPath id="tile"><rect ${TILE}/></clipPath>` +
    `<clipPath id="level"><polygon points="${LEVEL_CUT}"/></clipPath>` +
    `</defs>` +
    `<rect ${TILE} fill="${c.bg}"/>` +
    `<rect ${TILE} fill="url(#sheen)"/>` +
    (dev ? hazardBand(c) : "") +
    // A faint edge, so a tile close to the wallpaper's colour still has an outline.
    `<rect x="103" y="103" width="818" height="818" rx="183" fill="none" stroke="${c.faint}" stroke-opacity="0.55" stroke-width="6"/>` +
    (dev ? `<g transform="translate(0 -${GLYPH_LIFT})">${glyph}</g>` : glyph) +
    `</svg>`
  );
}

/** The picture as an address an `<img>` can show. */
export function iconUrl(c: IconColours, options?: IconOptions): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(iconSvg(c, options))}`;
}
