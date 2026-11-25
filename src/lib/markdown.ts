// Markdown to HTML for chat text.
//
// Safety: raw HTML in the source is escaped (`html: false`), markdown-it
// refuses javascript: and similar links (file: links are let back in: they
// only ever open in the side panel or Finder), and highlight.js escapes
// the code it colours. So the output contains only tags this file produces.
// Web images wait for a click (see the image rule below).

import MarkdownIt from "markdown-it";
import hljs from "highlight.js/lib/core";
import bash from "highlight.js/lib/languages/bash";
import c from "highlight.js/lib/languages/c";
import cpp from "highlight.js/lib/languages/cpp";
import css from "highlight.js/lib/languages/css";
import diff from "highlight.js/lib/languages/diff";
import go from "highlight.js/lib/languages/go";
import java from "highlight.js/lib/languages/java";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import sql from "highlight.js/lib/languages/sql";
import swift from "highlight.js/lib/languages/swift";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

// A fixed set keeps the bundle small. Each also registers its usual aliases
// (ts, js, sh, py, rs, html, yml and so on).
for (const [name, language] of Object.entries({
  bash, c, cpp, css, diff, go, java, javascript, json, markdown, python, rust, sql, swift, typescript, xml, yaml,
})) {
  hljs.registerLanguage(name, language);
}

const md = new MarkdownIt({ html: false, linkify: true });
const validateLink = md.validateLink.bind(md);
md.validateLink = (url) => /^file:/i.test(url.trim()) || validateLink(url);

function codeBlock(code: string, lang: string): string {
  const source = code.replace(/\n$/, "");
  const body =
    lang && hljs.getLanguage(lang)
      ? hljs.highlight(source, { language: lang, ignoreIllegals: true }).value
      : md.utils.escapeHtml(source);
  return (
    `<div class="code"><div class="code-head"><span>${md.utils.escapeHtml(lang || "text")}</span>` +
    `<button type="button" class="code-copy" data-copy>copy</button></div>` +
    `<pre><code>${body}</code></pre></div>\n`
  );
}

md.renderer.rules.fence = (tokens, idx) => {
  const token = tokens[idx];
  return codeBlock(token.content, token.info.trim().split(/\s+/)[0] ?? "");
};

md.renderer.rules.code_block = (tokens, idx) => codeBlock(tokens[idx].content, "");

// Web images are never fetched on their own. Fetching one sends its address
// out, and an agent misled by what it read can put data in that address
// (`![](https://site/x.png?key=...)`), so drawing the message would leak it.
// Each shows as `[image: ...]` until clicked (see clicks.ts); one clicked
// stays loaded when its text is drawn again. Images written into the text
// (`data:`) send nothing and show at once. Any other address is a link.
const loadedImages = new Set<string>();

/** Remembers that the user loaded the web image at `src`. */
export function imageLoaded(src: string) {
  loadedImages.add(src);
}

md.renderer.rules.image = (tokens, idx, options, env, self) => {
  const token = tokens[idx];
  const src = String(token.attrGet("src") ?? "");
  const alt = self.renderInlineAsText(token.children ?? [], options, env);
  const esc = md.utils.escapeHtml;
  if (/^data:/i.test(src) || (/^https?:/i.test(src) && loadedImages.has(src))) {
    return `<img src="${esc(src)}" alt="${esc(alt)}">`;
  }
  if (/^https?:/i.test(src)) {
    let label = alt;
    if (!label) {
      try {
        label = new URL(src).host;
      } catch {
        label = src;
      }
    }
    return (
      `<button type="button" class="md-image" data-image="${esc(src)}" data-alt="${esc(alt)}" ` +
      `title="${esc(src)}">[image: ${esc(label)}]</button>`
    );
  }
  return `<a href="${esc(src)}">${esc(alt || src)}</a>`;
};

export function renderMarkdown(text: string): string {
  return md.render(text);
}

/**
 * The same HTML, one string per top-level block (a paragraph, a list, a code
 * block...). Text streams in at the end, so every block but the last stays
 * the same string while it does: drawn once, never redone. That keeps
 * comments on finished blocks in place (see chatComments.ts). Parsed once
 * for the whole text, so reference links still reach across blocks.
 */
export function renderBlocks(text: string): string[] {
  const env = {};
  const tokens = md.parse(text, env);
  const starts: number[] = [];
  tokens.forEach((t, i) => {
    if (t.level === 0 && t.nesting !== -1) starts.push(i);
  });
  if (!starts.length) return [md.renderer.render(tokens, md.options, env)];
  return starts.map((start, k) => md.renderer.render(tokens.slice(start, starts[k + 1]), md.options, env));
}
