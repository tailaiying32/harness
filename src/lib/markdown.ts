// Markdown to HTML for chat text.
//
// Safety: raw HTML in the source is escaped (`html: false`), markdown-it
// refuses javascript: and similar links by default, and highlight.js escapes
// the code it colours. So the output contains only tags this file produces.

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

export function renderMarkdown(text: string): string {
  return md.render(text);
}
