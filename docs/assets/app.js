"use strict";

const site = {
  repo: document.body.dataset.repo || "",
  branch: document.body.dataset.branch || "main",
  home: document.body.dataset.home || "",
  pages: "pages/",
  sidebar: "sidebar.md",
  name: "luv docs",
};

const state = {
  sections: [],
  pages: [],
  byId: new Map(),
  cache: new Map(),
  current: null,
  headings: [],
  openSections: new Set(),
  index: null,
  targets: [],
};

const elements = {
  toc: document.querySelector(".toc"),
  content: document.querySelector(".content"),
  crumbs: document.querySelector(".crumbs"),
  pager: document.querySelector(".pager"),
  footer: document.querySelector(".footer"),
  search: document.querySelector(".search"),
  version: document.querySelector(".version"),
  theme: document.querySelector(".theme-button"),
  menu: document.querySelector(".menu-button"),
  searchButton: document.querySelector(".search-button"),
  overlay: document.querySelector(".overlay"),
  rail: document.querySelector(".rail"),
  railList: document.querySelector(".rail-list"),
  railLinks: document.querySelector(".rail-links"),
};

const LANGUAGE_NAMES = {
  luau: "Luau",
  lua: "Luau",
  c: "C",
  h: "C",
  rust: "Rust",
  rs: "Rust",
  wgsl: "WGSL",
  glsl: "GLSL",
  toml: "TOML",
  json: "JSON",
  yaml: "YAML",
  yml: "YAML",
  sh: "Shell",
  bash: "Shell",
  shell: "Shell",
  powershell: "PowerShell",
  ps1: "PowerShell",
  tree: "Files",
  text: "Text",
  txt: "Text",
};

const LANGUAGE_ALIASES = {
  lua: "luau",
  h: "c",
  rs: "rust",
  bash: "shell",
  sh: "shell",
  ps1: "powershell",
  yml: "yaml",
  txt: "text",
};

function escapeHtml(text) {
  return text.replace(/[&<>"']/g, (character) => {
    return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[character];
  });
}

function words(list) {
  return new Set(list.split(/\s+/).filter(Boolean));
}

const SYNTAX = {
  luau: {
    rules: [
      ["comment", /--\[(=*)\[[\s\S]*?\]\1\]|--[^\n]*/y],
      ["string", /\[(=*)\[[\s\S]*?\]\1\]|"(?:\\[\s\S]|[^"\\\n])*"|'(?:\\[\s\S]|[^'\\\n])*'|`(?:\\[\s\S]|[^`\\])*`/y],
      ["number", /0[xX][\da-fA-F_]+|0[bB][01_]+|(?:\d[\d_]*(?:\.[\d_]*)?|\.\d[\d_]*)(?:[eE][+-]?\d+)?/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words("and break continue do else elseif end export for function if in local not or repeat return then type until while"),
    literals: words("true false nil self"),
    builtins: words("import enum udim color vector print warn error assert pcall xpcall require select typeof tostring tonumber ipairs pairs next setmetatable getmetatable rawget rawset rawequal rawlen unpack coroutine string table math buffer bit32 utf8 os debug task EnterParallel ExitParallel"),
  },
  c: {
    rules: [
      ["comment", /\/\*[\s\S]*?\*\/|\/\/[^\n]*/y],
      ["meta", /#\s*[a-z]+[^\n]*/y],
      ["string", /"(?:\\[\s\S]|[^"\\\n])*"|'(?:\\[\s\S]|[^'\\\n])*'/y],
      ["number", /0[xX][\da-fA-F]+[uUlL]*|(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?[fFuUlL]*/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words("auto break case const continue default do else enum extern for goto if inline register restrict return sizeof static struct switch typedef union volatile while"),
    literals: words("NULL true false"),
    types: words("void char short int long float double signed unsigned bool int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t size_t uintptr_t intptr_t"),
    builtins: words("malloc free memcpy memset strlen printf"),
  },
  rust: {
    rules: [
      ["comment", /\/\*[\s\S]*?\*\/|\/\/[^\n]*/y],
      ["meta", /#!?\[[^\]\n]*\]/y],
      ["string", /b?"(?:\\[\s\S]|[^"\\])*"|b?'(?:\\[\s\S]|[^'\\\n])'/y],
      ["meta", /'[a-z_]\w*/y],
      ["number", /0[xX][\da-fA-F_]+|(?:\d[\d_]*(?:\.[\d_]+)?)(?:[eE][+-]?\d+)?(?:[iuf](?:8|16|32|64|128|size))?/y],
      ["builtin", /[a-z_]\w*!/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words("as async await break const continue crate dyn else enum extern fn for if impl in let loop match mod move mut pub ref return static struct super trait type unsafe use where while"),
    literals: words("true false self Self None Some Ok Err"),
    types: words("i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str"),
    builtins: words(""),
  },
  wgsl: {
    rules: [
      ["comment", /\/\*[\s\S]*?\*\/|\/\/[^\n]*/y],
      ["meta", /@\w+/y],
      ["number", /0[xX][\da-fA-F]+[iu]?|(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?[fhiu]?/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words("fn let var const struct return if else for loop while switch case default break continue continuing discard alias override enable uniform storage read write read_write private workgroup function"),
    literals: words("true false"),
    types: words("f32 f16 i32 u32 bool vec2 vec3 vec4 vec2f vec3f vec4f vec2u vec4u mat2x2 mat3x3 mat4x4 mat4x4f array atomic ptr sampler sampler_comparison texture_2d texture_3d texture_cube texture_depth_2d texture_storage_2d texture_multisampled_2d"),
    builtins: words(""),
  },
  glsl: {
    rules: [
      ["comment", /\/\*[\s\S]*?\*\/|\/\/[^\n]*/y],
      ["meta", /#\s*[a-z]+[^\n]*/y],
      ["number", /(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?[fu]?/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words("in out inout uniform buffer layout const return if else for while break continue discard struct"),
    literals: words("true false"),
    types: words("void bool int uint float vec2 vec3 vec4 ivec2 uvec2 mat3 mat4 sampler2D texture2D"),
    builtins: words("main"),
  },
  toml: {
    rules: [
      ["comment", /#[^\n]*/y],
      ["type", /\[\[?[^\]\n]*\]\]?/y, true],
      ["property", /[A-Za-z0-9_.-]+(?=\s*=)/y, true],
      ["string", /"""[\s\S]*?"""|'''[\s\S]*?'''|"(?:\\[\s\S]|[^"\\\n])*"|'[^'\n]*'/y],
      ["number", /[+-]?\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words(""),
    literals: words("true false"),
    builtins: words(""),
  },
  json: {
    rules: [
      ["property", /"(?:\\[\s\S]|[^"\\\n])*"(?=\s*:)/y],
      ["string", /"(?:\\[\s\S]|[^"\\\n])*"/y],
      ["number", /-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words(""),
    literals: words("true false null"),
    builtins: words(""),
  },
  yaml: {
    rules: [
      ["comment", /#[^\n]*/y],
      ["property", /[\w.-]+(?=\s*:)/y, true],
      ["string", /"(?:\\[\s\S]|[^"\\\n])*"|'[^'\n]*'/y],
      ["number", /-?\d+(?:\.\d+)?/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words(""),
    literals: words("true false null"),
    builtins: words(""),
  },
  shell: {
    rules: [
      ["comment", /#[^\n]*/y],
      ["string", /"(?:\\[\s\S]|[^"\\])*"|'[^']*'/y],
      ["builtin", /\$\w+|\$\{[^}]*\}/y],
      ["function", /[\w./~-]+/y, true],
      ["meta", /--?[\w-]+/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words("if then else fi for do done in while case esac export sudo cd"),
    literals: words("true false"),
    builtins: words(""),
  },
  powershell: {
    rules: [
      ["comment", /#[^\n]*/y],
      ["string", /"(?:`[\s\S]|[^"`])*"|'[^']*'/y],
      ["builtin", /\$[\w:]+/y],
      ["function", /[\w./~-]+/y, true],
      ["meta", /-[A-Za-z][\w-]*/y],
      ["word", /[A-Za-z_]\w*/y],
    ],
    keywords: words("if else foreach for while function return"),
    literals: words("$true $false $null"),
    builtins: words(""),
  },
  tree: {
    rules: [
      ["branch", /[│├└─┬┌┐┘┤┼|`+\\-]{1,}(?=[\s─]|$)/y],
      ["folder", /[^\s│├└─]+\/(?=\s|$)/y],
    ],
    keywords: words(""),
    literals: words(""),
    builtins: words(""),
  },
};

function atLineStart(code, index) {
  for (let cursor = index - 1; cursor >= 0; cursor--) {
    const character = code[cursor];
    if (character === "\n") {
      return true;
    }
    if (character !== " " && character !== "\t") {
      return false;
    }
  }
  return true;
}

function classify(word, code, after, spec) {
  if (spec.keywords.has(word)) {
    return "keyword";
  }
  if (spec.literals.has(word)) {
    return "literal";
  }
  if (spec.types && spec.types.has(word)) {
    return "type";
  }
  if (spec.builtins.has(word)) {
    return "builtin";
  }
  const next = code.slice(after).match(/^\s*(\S)/);
  if (next && (next[1] === "(" || (next[1] === "<" && spec === SYNTAX.wgsl))) {
    return "function";
  }
  if (/^[A-Z]/.test(word)) {
    return "type";
  }
  return null;
}

function highlight(code, language) {
  const spec = SYNTAX[language];
  if (!spec) {
    return escapeHtml(code);
  }
  let output = "";
  let plain = "";
  let index = 0;
  const flush = () => {
    if (plain) {
      output += escapeHtml(plain);
      plain = "";
    }
  };
  scan: while (index < code.length) {
    for (const [kind, rule, lineStart] of spec.rules) {
      if (lineStart && !atLineStart(code, index)) {
        continue;
      }
      rule.lastIndex = index;
      const match = rule.exec(code);
      if (!match || !match[0].length) {
        continue;
      }
      const text = match[0];
      const type = kind === "word" ? classify(text, code, index + text.length, spec) : kind;
      flush();
      output += type ? `<span class="tok-${type}">${escapeHtml(text)}</span>` : escapeHtml(text);
      index += text.length;
      continue scan;
    }
    plain += code[index];
    index += 1;
  }
  flush();
  return output;
}

function slugify(text) {
  return text
    .toLowerCase()
    .replace(/<[^>]+>/g, "")
    .replace(/&[a-z]+;/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "section";
}

function plainText(markdownText) {
  return markdownText
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[`*]/g, "")
    .replace(/\\(.)/g, "$1")
    .trim();
}

function pageIdFromPath(path) {
  return path
    .replace(/^\.\//, "")
    .replace(/^\/+/, "")
    .replace(/^pages\//, "")
    .replace(/\.md$/i, "");
}

function joinPath(base, relative) {
  const parts = relative.startsWith("/") ? [] : base.slice();
  for (const part of relative.replace(/^\/+/, "").split("/")) {
    if (part === "..") {
      parts.pop();
    } else if (part && part !== ".") {
      parts.push(part);
    }
  }
  return parts.join("/");
}

function pageHref(id, hash) {
  return `?page=${encodeURI(id)}${hash ? `#${hash}` : ""}`;
}

function resolveLink(href, context) {
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("//")) {
    return { href, external: true };
  }
  if (href.startsWith("#")) {
    return { href: pageHref(context.page, href.slice(1)), page: context.page };
  }
  const hashIndex = href.indexOf("#");
  const path = hashIndex >= 0 ? href.slice(0, hashIndex) : href;
  const hash = hashIndex >= 0 ? href.slice(hashIndex + 1) : "";
  const folder = context.page.split("/").slice(0, -1);
  if (/\.md$/i.test(path)) {
    const target = path.startsWith("/") ? pageIdFromPath(path) : joinPath(folder, path).replace(/\.md$/i, "");
    return { href: pageHref(target, hash), page: target };
  }
  const file = path.startsWith("/") ? joinPath([], path) : joinPath(["pages", ...folder], path);
  return { href: file + (hash ? `#${hash}` : "") };
}

const INLINE = new RegExp(
  [
    "(`+)([\\s\\S]*?[^`])\\1(?!`)",
    "!\\[([^\\]]*)\\]\\(\\s*([^)\\s]+)(?:\\s+\"([^\"]*)\")?\\s*\\)",
    "\\[((?:[^\\[\\]]|\\[[^\\]]*\\])+)\\]\\(\\s*([^)\\s]+)(?:\\s+\"([^\"]*)\")?\\s*\\)",
    "\\*\\*(?=\\S)([\\s\\S]*?\\S)\\*\\*",
    "\\*(?=[^\\s*])([\\s\\S]*?[^\\s*])\\*",
    "<(https?:\\/\\/[^\\s>]+)>",
    "\\\\([\\\\`*_{}\\[\\]()#+\\-.!|<>~])",
  ].join("|"),
  "g",
);

function inline(text, context) {
  let output = "";
  let last = 0;
  INLINE.lastIndex = 0;
  const pattern = new RegExp(INLINE.source, "g");
  let match;
  while ((match = pattern.exec(text))) {
    output += escapeHtml(text.slice(last, match.index));
    last = pattern.lastIndex;
    if (match[1] !== undefined) {
      let code = match[2];
      if (/^ .* $/.test(code)) {
        code = code.slice(1, -1);
      }
      output += `<code>${escapeHtml(code)}</code>`;
    } else if (match[3] !== undefined) {
      const target = resolveLink(match[4], context);
      const title = match[5] ? ` title="${escapeHtml(match[5])}"` : "";
      output += `<img src="${escapeHtml(target.href)}" alt="${escapeHtml(match[3])}"${title} loading="lazy">`;
    } else if (match[6] !== undefined) {
      const target = resolveLink(match[7], context);
      const title = match[8] ? ` title="${escapeHtml(match[8])}"` : "";
      const external = target.external ? ` target="_blank" rel="noopener"` : "";
      const page = target.page ? ` data-page="${escapeHtml(target.page)}"` : "";
      output += `<a href="${escapeHtml(target.href)}"${title}${external}${page}>${inline(match[6], context)}</a>`;
    } else if (match[9] !== undefined) {
      output += `<strong>${inline(match[9], context)}</strong>`;
    } else if (match[10] !== undefined) {
      output += `<em>${inline(match[10], context)}</em>`;
    } else if (match[11] !== undefined) {
      output += `<a href="${escapeHtml(match[11])}" target="_blank" rel="noopener">${escapeHtml(match[11])}</a>`;
    } else if (match[12] !== undefined) {
      output += escapeHtml(match[12]);
    }
  }
  output += escapeHtml(text.slice(last));
  return output;
}

const FENCE = /^( {0,3})(`{3,}|~{3,})\s*([^\s`]*)\s*(.*)$/;
const HEADING = /^ {0,3}(#{1,6})\s+(.*?)(?:\s+#+)?\s*$/;
const RULE = /^ {0,3}([-*_])(?:\s*\1){2,}\s*$/;
const QUOTE = /^ {0,3}>\s?(.*)$/;
const ITEM = /^( *)([-*+]|\d{1,9}[.)])(?: +(.*)|$)/;
const TABLE_RULE = /^\s*\|?\s*:?-+:?\s*(?:\|\s*:?-+:?\s*)*\|?\s*$/;
const HTML_BLOCK = /^ {0,3}<\/?(?:div|details|summary|table|thead|tbody|tr|td|th|p|img|br|figure|figcaption|section|span|kbd|video|iframe)\b/i;

function startsBlock(line, next) {
  return (
    FENCE.test(line) ||
    HEADING.test(line) ||
    RULE.test(line) ||
    QUOTE.test(line) ||
    ITEM.test(line) ||
    HTML_BLOCK.test(line) ||
    (line.includes("|") && next !== undefined && TABLE_RULE.test(next) && next.includes("-"))
  );
}

function readFence(lines, start, opener) {
  const indent = opener[1].length;
  const marker = opener[2];
  const closing = new RegExp(`^ {0,3}${marker[0] === "`" ? "`" : "~"}{${marker.length},}\\s*$`);
  const body = [];
  let index = start + 1;
  while (index < lines.length && !closing.test(lines[index])) {
    const line = lines[index];
    body.push(line.slice(Math.min(indent, line.match(/^ */)[0].length)));
    index += 1;
  }
  const raw = (opener[3] || "text").toLowerCase();
  return {
    language: LANGUAGE_ALIASES[raw] || raw,
    label: LANGUAGE_NAMES[raw] || opener[3] || "Text",
    title: opener[4].replace(/^title=/, "").replace(/^"|"$/g, ""),
    code: body.join("\n"),
    end: Math.min(index + 1, lines.length),
  };
}

function renderCodeGroup(blocks) {
  const copy = `<button class="copy" type="button">Copy</button>`;
  if (blocks.length === 1) {
    const block = blocks[0];
    const label = block.title || (block.language === "text" ? "" : block.label);
    return `<div class="code-box">${label ? `<span class="label">${escapeHtml(label)}</span>` : ""}${copy}<pre><code class="language-${escapeHtml(block.language)}">${highlight(block.code, block.language)}</code></pre></div>`;
  }
  const tabs = blocks
    .map((block, index) => `<button class="tab${index === 0 ? " active" : ""}" type="button" role="tab" data-language="${escapeHtml(block.label)}">${escapeHtml(block.title || block.label)}</button>`)
    .join("");
  const panels = blocks
    .map((block, index) => `<div class="tab-panel${index === 0 ? " active" : ""}" role="tabpanel">${copy}<pre><code class="language-${escapeHtml(block.language)}">${highlight(block.code, block.language)}</code></pre></div>`)
    .join("");
  return `<div class="code-tabs"><div class="tab-bar" role="tablist">${tabs}</div><div class="tab-panels">${panels}</div></div>`;
}

function splitRow(line) {
  let text = line.trim();
  if (text.startsWith("|")) {
    text = text.slice(1);
  }
  if (text.endsWith("|") && !text.endsWith("\\|")) {
    text = text.slice(0, -1);
  }
  const cells = [];
  let cell = "";
  let ticks = 0;
  for (let index = 0; index < text.length; index++) {
    const character = text[index];
    if (character === "\\" && text[index + 1] === "|") {
      cell += "|";
      index += 1;
      continue;
    }
    if (character === "`") {
      let run = 1;
      while (text[index + run] === "`") {
        run += 1;
      }
      if (ticks === 0) {
        ticks = run;
      } else if (ticks === run) {
        ticks = 0;
      }
      cell += "`".repeat(run);
      index += run - 1;
      continue;
    }
    if (character === "|" && ticks === 0) {
      cells.push(cell.trim());
      cell = "";
      continue;
    }
    cell += character;
  }
  cells.push(cell.trim());
  return cells;
}

function renderTable(lines, start, context) {
  const header = splitRow(lines[start]);
  const aligns = splitRow(lines[start + 1]).map((cell) => {
    const left = cell.startsWith(":");
    const right = cell.endsWith(":");
    return left && right ? "center" : right ? "right" : left ? "left" : "";
  });
  const rows = [];
  let index = start + 2;
  while (index < lines.length && lines[index].trim() && lines[index].includes("|")) {
    rows.push(splitRow(lines[index]));
    index += 1;
  }
  const cell = (tag, text, column) => {
    const align = aligns[column] ? ` style="text-align:${aligns[column]}"` : "";
    return `<${tag}${align}>${inline(text || "", context)}</${tag}>`;
  };
  const head = `<thead><tr>${header.map((text, column) => cell("th", text, column)).join("")}</tr></thead>`;
  const body = rows.map((row) => `<tr>${header.map((_, column) => cell("td", row[column], column)).join("")}</tr>`).join("");
  return { html: `<table>${head}<tbody>${body}</tbody></table>`, end: index };
}

function readList(lines, start, context) {
  const first = lines[start].match(ITEM);
  const ordered = /\d/.test(first[2]);
  const base = first[1].length;
  const items = [];
  let current = null;
  let blank = false;
  let loose = false;
  let index = start;
  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim()) {
      blank = true;
      index += 1;
      continue;
    }
    const indent = line.match(/^ */)[0].length;
    const match = line.match(ITEM);
    const sibling = match && match[1].length >= base && (!current || match[1].length < current.indent) && /\d/.test(match[2]) === ordered;
    if (sibling) {
      if (current && blank) {
        loose = true;
      }
      const content = match[3] === undefined ? "" : match[3];
      const markerEnd = match[1].length + match[2].length;
      const gap = match[3] === undefined ? 1 : Math.min(line.slice(markerEnd).match(/^ */)[0].length, 4);
      current = { indent: markerEnd + gap, lines: [content], number: parseInt(match[2], 10) };
      items.push(current);
      blank = false;
      index += 1;
      continue;
    }
    if (current && indent >= current.indent) {
      if (blank) {
        current.lines.push("");
        current.gap = true;
      }
      current.lines.push(line.slice(current.indent));
      blank = false;
      index += 1;
      continue;
    }
    if (current && !blank && !match && !startsBlock(line, lines[index + 1])) {
      current.lines.push(line.trim());
      index += 1;
      continue;
    }
    break;
  }
  while (index > start && !lines[index - 1].trim()) {
    index -= 1;
  }
  if (items.some((item) => item.gap && item.lines.some((line, position) => position > 0 && line === "" && item.lines[position + 1] !== undefined && !ITEM.test(item.lines[position + 1])))) {
    loose = true;
  }
  const tag = ordered ? "ol" : "ul";
  const startAttribute = ordered && items[0].number !== 1 ? ` start="${items[0].number}"` : "";
  const body = items.map((item) => `<li>${renderBlocks(item.lines, context, !loose)}</li>`).join("");
  return { html: `<${tag}${startAttribute}>${body}</${tag}>`, end: index };
}

const CALLOUTS = {
  NOTE: ["note", "Note"],
  TIP: ["tip", "Tip"],
  IMPORTANT: ["note", "Important"],
  WARNING: ["warning", "Warning"],
  CAUTION: ["warning", "Caution"],
};

function renderQuote(lines, start, context) {
  const body = [];
  let index = start;
  while (index < lines.length) {
    const match = lines[index].match(QUOTE);
    if (!match) {
      break;
    }
    body.push(match[1]);
    index += 1;
  }
  const marker = body[0] && body[0].match(/^\[!(\w+)\]\s*(.*)$/);
  if (marker && CALLOUTS[marker[1].toUpperCase()]) {
    const [kind, title] = CALLOUTS[marker[1].toUpperCase()];
    const rest = body.slice(1);
    if (marker[2]) {
      rest.unshift(marker[2]);
    }
    return {
      html: `<div class="callout ${kind}"><p class="callout-title">${title}</p>${renderBlocks(rest, context, false)}</div>`,
      end: index,
    };
  }
  return { html: `<blockquote>${renderBlocks(body, context, false)}</blockquote>`, end: index };
}

function renderHeading(level, text, context) {
  const plain = plainText(text);
  let id = slugify(plain);
  const seen = context.ids.get(id) || 0;
  context.ids.set(id, seen + 1);
  if (seen) {
    id = `${id}-${seen + 1}`;
  }
  if (level === 1 && !context.title) {
    context.title = plain;
  }
  if (level === 2) {
    context.headings.push({ id, text: plain });
  }
  context.sections.push({ id, text: plain, level });
  return `<h${level} id="${id}">${inline(text, context)}<a class="anchor" href="${pageHref(context.page, id)}" data-page="${escapeHtml(context.page)}" aria-label="Link to this section">&para;</a></h${level}>`;
}

function renderParagraph(text, context, tight) {
  const html = inline(text, context);
  const inherits = text.match(/^(Inherits|Inherited by):\s/);
  if (inherits) {
    const parts = html.replace(/^(Inherits|Inherited by):\s*/, "").split(/\s+&lt;\s+/);
    return `<p class="inherits"><strong>${inherits[1]}:</strong> ${parts.join('<span class="sep">&lt;</span>')}</p>`;
  }
  return tight ? html : `<p>${html}</p>`;
}

function renderBlocks(lines, context, tight) {
  const output = [];
  let index = 0;
  let paragraphs = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim()) {
      index += 1;
      continue;
    }
    const fence = line.match(FENCE);
    if (fence) {
      const group = [];
      let cursor = index;
      while (cursor < lines.length) {
        const opener = lines[cursor].match(FENCE);
        if (!opener) {
          break;
        }
        const block = readFence(lines, cursor, opener);
        group.push(block);
        cursor = block.end;
        let peek = cursor;
        while (peek < lines.length && !lines[peek].trim()) {
          peek += 1;
        }
        if (peek < lines.length && FENCE.test(lines[peek])) {
          cursor = peek;
          continue;
        }
        break;
      }
      output.push(renderCodeGroup(group));
      index = cursor;
      continue;
    }
    const heading = line.match(HEADING);
    if (heading) {
      output.push(renderHeading(heading[1].length, heading[2], context));
      index += 1;
      continue;
    }
    if (RULE.test(line)) {
      output.push("<hr>");
      index += 1;
      continue;
    }
    if (QUOTE.test(line)) {
      const quote = renderQuote(lines, index, context);
      output.push(quote.html);
      index = quote.end;
      continue;
    }
    if (line.includes("|") && index + 1 < lines.length && TABLE_RULE.test(lines[index + 1]) && lines[index + 1].includes("-")) {
      const table = renderTable(lines, index, context);
      output.push(table.html);
      index = table.end;
      continue;
    }
    if (ITEM.test(line)) {
      const list = readList(lines, index, context);
      output.push(list.html);
      index = list.end;
      continue;
    }
    if (HTML_BLOCK.test(line)) {
      const block = [];
      while (index < lines.length && lines[index].trim()) {
        block.push(lines[index]);
        index += 1;
      }
      output.push(block.join("\n"));
      continue;
    }
    const paragraph = [line.trim()];
    index += 1;
    while (index < lines.length && lines[index].trim() && !startsBlock(lines[index], lines[index + 1])) {
      paragraph.push(lines[index].trim());
      index += 1;
    }
    paragraphs += 1;
    output.push(renderParagraph(paragraph.join("\n"), context, tight && paragraphs === 1));
  }
  return output.join("\n");
}

function renderMarkdown(source, page) {
  const context = { page, ids: new Map(), headings: [], sections: [], title: null };
  const lines = source.replace(/\r\n?/g, "\n").replace(/\t/g, "    ").split("\n");
  const html = renderBlocks(lines, context, false);
  return { html, title: context.title, headings: context.headings, sections: context.sections };
}

function parseSidebar(source) {
  const sections = [];
  const pages = [];
  let section = null;
  let stack = [];
  for (const raw of source.replace(/\r\n?/g, "\n").replace(/\t/g, "    ").split("\n")) {
    if (!raw.trim()) {
      continue;
    }
    const heading = raw.match(/^#{1,6}\s+(.+?)\s*$/);
    if (heading) {
      section = { title: heading[1], items: [], index: sections.length };
      sections.push(section);
      stack = [];
      continue;
    }
    const item = raw.match(/^( *)[-*+]\s+\[([^\]]+)\]\(([^)\s]+)\)/);
    if (!item) {
      continue;
    }
    if (!section) {
      section = { title: "Pages", items: [], index: sections.length };
      sections.push(section);
    }
    const indent = item[1].length;
    while (stack.length && stack[stack.length - 1].indent >= indent) {
      stack.pop();
    }
    const parent = stack.length ? stack[stack.length - 1].entry : null;
    const entry = { title: item[2], id: pageIdFromPath(item[3]), children: [], parent, section };
    (parent ? parent.children : section.items).push(entry);
    stack.push({ indent, entry });
    pages.push(entry);
  }
  return { sections, pages };
}

async function load(path) {
  if (!state.cache.has(path)) {
    const request = fetch(path, { cache: "no-cache" }).then((response) => {
      if (!response.ok) {
        throw new Error(`${path} returned ${response.status}`);
      }
      return response.text();
    });
    request.catch(() => state.cache.delete(path));
    state.cache.set(path, request);
  }
  return state.cache.get(path);
}

function contains(entry, id) {
  if (entry.id === id) {
    return true;
  }
  return entry.children.some((child) => contains(child, id));
}

function renderItems(items, depth) {
  return items
    .map((entry) => {
      const current = state.current === entry.id;
      const expanded = contains(entry, state.current || "");
      const classes = ["page-item", current ? "current" : "", expanded ? "expanded" : ""].filter(Boolean).join(" ");
      let inner = "";
      if (expanded && entry.children.length) {
        inner += `<ul>${renderItems(entry.children, depth + 1)}</ul>`;
      }
      if (current && state.headings.length) {
        inner += `<ul class="headings">${state.headings
          .map((heading) => `<li><a href="${pageHref(entry.id, heading.id)}" data-page="${escapeHtml(entry.id)}" data-heading="${heading.id}">${escapeHtml(heading.text)}</a></li>`)
          .join("")}</ul>`;
      }
      return `<li class="${classes}"><a href="${pageHref(entry.id)}" data-page="${escapeHtml(entry.id)}"><span class="toggle"></span><span>${escapeHtml(entry.title)}</span></a>${inner}</li>`;
    })
    .join("");
}

function renderSidebar() {
  elements.toc.innerHTML = state.sections
    .map((section) => {
      const open = state.openSections.has(section.index);
      return `<div class="section${open ? " open" : ""}" data-section="${section.index}"><button class="section-title" type="button"><span class="chevron"></span><span>${escapeHtml(section.title)}</span></button><ul>${renderItems(section.items, 0)}</ul></div>`;
    })
    .join("");
  const active = elements.toc.querySelector(".page-item.current > a");
  if (active) {
    const box = elements.toc.getBoundingClientRect();
    const spot = active.getBoundingClientRect();
    if (spot.top < box.top || spot.bottom > box.bottom) {
      active.scrollIntoView({ block: "center" });
    }
  }
}

function renderCrumbs(entry) {
  const trail = [`<span><a href="${pageHref(site.home)}" data-page="${escapeHtml(site.home)}">Docs</a></span>`];
  if (entry) {
    trail.push(`<span>${escapeHtml(entry.section.title)}</span>`);
    const chain = [];
    for (let step = entry.parent; step; step = step.parent) {
      chain.unshift(step);
    }
    for (const step of chain) {
      trail.push(`<span><a href="${pageHref(step.id)}" data-page="${escapeHtml(step.id)}">${escapeHtml(step.title)}</a></span>`);
    }
    trail.push(`<span>${escapeHtml(entry.title)}</span>`);
  }
  const edit = entry && site.repo ? `<a href="https://github.com/${site.repo}/edit/${site.branch}/docs/${site.pages}${entry.id}.md" target="_blank" rel="noopener">Edit on GitHub</a>` : "";
  elements.crumbs.innerHTML = `<div class="trail">${trail.join("")}</div>${edit}`;
}

function renderPager(entry) {
  if (!entry) {
    elements.pager.innerHTML = "";
    return;
  }
  const position = state.pages.indexOf(entry);
  const previous = state.pages[position - 1];
  const next = state.pages[position + 1];
  elements.pager.innerHTML = [
    previous ? `<a class="previous" href="${pageHref(previous.id)}" data-page="${escapeHtml(previous.id)}"><small>&larr; Previous</small>${escapeHtml(previous.title)}</a>` : "",
    next ? `<a class="next" href="${pageHref(next.id)}" data-page="${escapeHtml(next.id)}"><small>Next &rarr;</small>${escapeHtml(next.title)}</a>` : "",
  ].join("");
}

function preferredLanguage() {
  try {
    return localStorage.getItem("luv-code-tab");
  } catch {
    return null;
  }
}

function selectTab(group, language) {
  const tabs = [...group.querySelectorAll(".tab")];
  const panels = [...group.querySelectorAll(".tab-panel")];
  const index = tabs.findIndex((tab) => tab.dataset.language === language);
  if (index < 0) {
    return false;
  }
  tabs.forEach((tab, position) => tab.classList.toggle("active", position === index));
  panels.forEach((panel, position) => panel.classList.toggle("active", position === index));
  return true;
}

function enhance() {
  const language = preferredLanguage();
  if (language) {
    elements.content.querySelectorAll(".code-tabs").forEach((group) => selectTab(group, language));
  }
}

function renderRail(sections, entry) {
  const items = sections.filter((section) => section.level === 2 || section.level === 3);
  const page = state.current || "";
  elements.railList.innerHTML = items
    .map((section) => `<li class="level-${section.level}"><a href="${pageHref(page, section.id)}" data-page="${escapeHtml(page)}" data-heading="${section.id}">${escapeHtml(section.text)}</a></li>`)
    .join("");
  const links = [];
  if (entry && site.repo) {
    links.push(`<a href="https://github.com/${site.repo}/edit/${site.branch}/docs/${site.pages}${entry.id}.md" target="_blank" rel="noopener">Edit this page</a>`);
    links.push(`<a href="https://github.com/${site.repo}/issues/new?title=${encodeURIComponent(`Docs: ${entry.title}`)}" target="_blank" rel="noopener">Report a problem</a>`);
  }
  links.push(`<a href="#" class="to-top">Back to top</a>`);
  elements.railLinks.innerHTML = links.join("");
  elements.rail.classList.toggle("empty", items.length === 0);
}

function clearRail() {
  elements.railList.innerHTML = "";
  elements.railLinks.innerHTML = "";
  elements.rail.classList.add("empty");
  state.targets = [];
}

function trackHeadings() {
  state.targets = [...elements.content.querySelectorAll("h2[id], h3[id]")];
  updateActiveHeading();
}

function updateActiveHeading() {
  const targets = state.targets;
  if (!targets.length) {
    return;
  }
  let index = -1;
  for (let position = 0; position < targets.length; position++) {
    if (targets[position].getBoundingClientRect().top > 130) {
      break;
    }
    index = position;
  }
  const bottom = window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 4;
  if (bottom && window.scrollY > 0) {
    index = targets.length - 1;
  }
  if (index < 0) {
    index = 0;
  }
  const current = targets[index];
  let section = null;
  for (let position = index; position >= 0; position--) {
    if (targets[position].tagName === "H2") {
      section = targets[position];
      break;
    }
  }
  let activeLink = null;
  elements.railList.querySelectorAll("a").forEach((link) => {
    const active = link.dataset.heading === current.id;
    link.classList.toggle("active", active);
    if (active) {
      activeLink = link;
    }
  });
  elements.toc.querySelectorAll(".headings a").forEach((link) => {
    link.classList.toggle("active", Boolean(section) && link.dataset.heading === section.id);
  });
  const box = elements.rail.querySelector(".rail-inner");
  if (activeLink && box.clientHeight > 0 && box.scrollHeight > box.clientHeight) {
    const boxRect = box.getBoundingClientRect();
    const linkRect = activeLink.getBoundingClientRect();
    if (linkRect.top < boxRect.top + 40 || linkRect.bottom > boxRect.bottom - 40) {
      box.scrollTop += linkRect.top - boxRect.top - box.clientHeight / 3;
    }
  }
}

function scrollToHash(hash) {
  if (!hash) {
    window.scrollTo(0, 0);
    return;
  }
  const target = document.getElementById(decodeURIComponent(hash));
  if (target) {
    target.scrollIntoView();
  } else {
    window.scrollTo(0, 0);
  }
}

function showNotice(title, message) {
  elements.content.innerHTML = `<h1>${escapeHtml(title)}</h1><div class="notice">${message}</div>`;
  elements.pager.innerHTML = "";
  clearRail();
}

function localFileNotice() {
  return `This page loads its Markdown files with <code>fetch</code>, and browsers block that for files opened straight from disk. Run the preview server inside the <code>docs</code> folder. It opens the site in your browser.<div class="code-box"><pre><code>${highlight("python serve.py", "shell")}</code></pre></div>`;
}

async function showPage(id, hash) {
  const entry = state.byId.get(id) || null;
  state.current = id;
  state.headings = [];
  if (entry) {
    state.openSections.add(entry.section.index);
  }
  elements.content.innerHTML = `<p class="loading">Loading...</p>`;
  let source;
  try {
    source = await load(site.pages + id + ".md");
  } catch (error) {
    renderCrumbs(entry);
    renderSidebar();
    if (location.protocol === "file:") {
      showNotice("Open with a web server", localFileNotice());
    } else {
      showNotice("Page not found", `There is no page at <code>${escapeHtml(site.pages + id + ".md")}</code>. Check the link or the sidebar file.`);
    }
    return;
  }
  if (state.current !== id) {
    return;
  }
  const page = renderMarkdown(source, id);
  state.headings = page.headings;
  elements.content.innerHTML = page.html;
  document.title = `${page.title || (entry ? entry.title : id)} | ${site.name}`;
  renderCrumbs(entry);
  renderPager(entry);
  renderSidebar();
  renderRail(page.sections, entry);
  enhance();
  scrollToHash(hash);
  trackHeadings();
}

function sectionEntries(source, entry) {
  const found = [];
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  let current = { id: "", text: entry.title, body: [] };
  let fence = null;
  const ids = new Map();
  for (const line of lines) {
    const opener = line.match(FENCE);
    if (fence) {
      if (new RegExp(`^ {0,3}${fence}\\s*$`).test(line)) {
        fence = null;
      }
      current.body.push(line);
      continue;
    }
    if (opener) {
      fence = opener[2][0] === "`" ? "`{" + opener[2].length + ",}" : "~{" + opener[2].length + ",}";
      continue;
    }
    const heading = line.match(HEADING);
    if (heading) {
      found.push(current);
      const text = plainText(heading[2]);
      let id = slugify(text);
      const seen = ids.get(id) || 0;
      ids.set(id, seen + 1);
      if (seen) {
        id = `${id}-${seen + 1}`;
      }
      current = { id: heading[1].length === 1 ? "" : id, text: heading[1].length === 1 ? entry.title : text, body: [] };
      continue;
    }
    current.body.push(line);
  }
  found.push(current);
  return found
    .map((section) => ({
      page: entry,
      id: section.id,
      heading: section.text,
      text: plainText(
        section.body
          .map((line) => line.replace(/^\s*(?:[-*+]|\d+[.)]|>)\s+/, "").replace(/^\s*\|?\s*:?-{3,}.*$/, "").replace(/\|/g, " "))
          .join(" "),
      ).replace(/\s+/g, " "),
    }))
    .filter((section) => section.heading || section.text);
}

async function buildIndex() {
  if (state.index) {
    return state.index;
  }
  const lists = await Promise.all(
    state.pages.map(async (entry) => {
      try {
        return sectionEntries(await load(site.pages + entry.id + ".md"), entry);
      } catch {
        return [];
      }
    }),
  );
  state.index = lists.flat();
  return state.index;
}

function snippet(text, terms) {
  const lower = text.toLowerCase();
  const hit = terms.map((term) => lower.indexOf(term)).filter((position) => position >= 0).sort((a, b) => a - b)[0];
  const start = Math.max(0, (hit === undefined ? 0 : hit) - 70);
  let piece = text.slice(start, start + 220);
  if (start > 0) {
    piece = "..." + piece;
  }
  if (start + 220 < text.length) {
    piece += "...";
  }
  let html = escapeHtml(piece);
  for (const term of terms) {
    html = html.replace(new RegExp(`(${term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi"), "<mark>$1</mark>");
  }
  return html;
}

async function showSearch(query) {
  state.current = null;
  state.headings = [];
  renderSidebar();
  renderCrumbs(null);
  clearRail();
  elements.pager.innerHTML = "";
  elements.search.q.value = query;
  document.title = `Search | ${site.name}`;
  elements.content.innerHTML = `<h1>Search results</h1><p class="loading">Searching...</p>`;
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) {
    elements.content.innerHTML = `<h1>Search results</h1><p>Type something in the search box.</p>`;
    return;
  }
  const index = await buildIndex();
  const scored = [];
  for (const entry of index) {
    const heading = entry.heading.toLowerCase();
    const title = entry.page.title.toLowerCase();
    const text = entry.text.toLowerCase();
    let score = 0;
    let all = true;
    for (const term of terms) {
      const inHeading = heading.includes(term);
      const inTitle = title.includes(term);
      const inText = text.includes(term);
      if (!inHeading && !inTitle && !inText) {
        all = false;
        break;
      }
      score += (inHeading ? 12 : 0) + (heading === term ? 20 : 0) + (inTitle ? 5 : 0) + (inText ? 1 : 0);
    }
    if (all) {
      scored.push({ entry, score });
    }
  }
  scored.sort((a, b) => b.score - a.score);
  const shown = scored.slice(0, 60);
  const list = shown
    .map(({ entry }) => {
      const where = entry.id ? `${escapeHtml(entry.page.section.title)} &raquo; ${escapeHtml(entry.page.title)}` : escapeHtml(entry.page.section.title);
      return `<li><a href="${pageHref(entry.page.id, entry.id)}" data-page="${escapeHtml(entry.page.id)}">${escapeHtml(entry.heading)}</a><div class="where">${where}</div><p class="snippet">${snippet(entry.text, terms)}</p></li>`;
    })
    .join("");
  elements.content.innerHTML = `<h1>Search results</h1><p>${scored.length ? `Found ${scored.length} match${scored.length === 1 ? "" : "es"} for <strong>${escapeHtml(query)}</strong>.` : `Nothing matched <strong>${escapeHtml(query)}</strong>.`}</p><ul class="results">${list}</ul>`;
}

function route() {
  const params = new URLSearchParams(location.search);
  const search = params.get("search");
  if (search !== null) {
    showSearch(search);
    return;
  }
  const id = params.get("page") || site.home || (state.pages[0] && state.pages[0].id) || "";
  showPage(id, location.hash.slice(1));
}

function navigate(url) {
  const target = new URL(url, location.href);
  if (target.search === location.search && target.pathname === location.pathname) {
    history.pushState(null, "", target.href);
    const params = new URLSearchParams(target.search);
    if (params.get("search") === null && state.current) {
      scrollToHash(target.hash.slice(1));
      return;
    }
  } else {
    history.pushState(null, "", target.href);
  }
  route();
}

function closeMenu() {
  document.body.classList.remove("menu-open");
}

function applyTheme(theme) {
  if (theme) {
    document.documentElement.dataset.theme = theme;
  } else {
    delete document.documentElement.dataset.theme;
  }
  try {
    if (theme) {
      localStorage.setItem("luv-theme", theme);
    } else {
      localStorage.removeItem("luv-theme");
    }
  } catch {}
}

function currentTheme() {
  const chosen = document.documentElement.dataset.theme;
  if (chosen) {
    return chosen;
  }
  return window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

async function showVersion() {
  if (!site.repo || !elements.version) {
    return;
  }
  const key = `luv-version-${site.repo}`;
  const remember = (value) => {
    try {
      sessionStorage.setItem(key, value);
    } catch {}
  };
  try {
    const cached = sessionStorage.getItem(key);
    if (cached !== null) {
      if (cached) {
        elements.version.textContent = cached;
      }
      return;
    }
  } catch {}
  try {
    const response = await fetch(`https://api.github.com/repos/${site.repo}/releases/latest`);
    const release = response.ok ? await response.json() : null;
    const tag = release && release.tag_name ? String(release.tag_name) : "";
    if (tag) {
      elements.version.textContent = tag;
    }
    remember(tag);
  } catch {
    remember("");
  }
}

document.addEventListener("click", (event) => {
  const tab = event.target.closest(".tab");
  if (tab) {
    const language = tab.dataset.language;
    const group = tab.closest(".code-tabs");
    selectTab(group, language);
    elements.content.querySelectorAll(".code-tabs").forEach((other) => {
      if (other !== group) {
        selectTab(other, language);
      }
    });
    try {
      localStorage.setItem("luv-code-tab", language);
    } catch {}
    return;
  }
  const copy = event.target.closest(".copy");
  if (copy) {
    const code = copy.parentElement.querySelector("pre code");
    if (code && navigator.clipboard) {
      navigator.clipboard.writeText(code.textContent).then(() => {
        copy.textContent = "Copied";
        setTimeout(() => {
          copy.textContent = "Copy";
        }, 1400);
      });
    }
    return;
  }
  const sectionTitle = event.target.closest(".section-title");
  if (sectionTitle) {
    const section = sectionTitle.parentElement;
    const index = Number(section.dataset.section);
    if (state.openSections.has(index)) {
      state.openSections.delete(index);
    } else {
      state.openSections.add(index);
    }
    section.classList.toggle("open");
    return;
  }
  const top = event.target.closest(".to-top");
  if (top) {
    event.preventDefault();
    window.scrollTo({ top: 0, behavior: "smooth" });
    return;
  }
  const link = event.target.closest("a[href]");
  if (!link || link.target === "_blank" || event.ctrlKey || event.metaKey || event.shiftKey || event.button !== 0) {
    return;
  }
  const href = link.getAttribute("href");
  if (!href.startsWith("?") && !href.startsWith("#")) {
    return;
  }
  event.preventDefault();
  closeMenu();
  if (href.startsWith("#")) {
    history.pushState(null, "", href);
    scrollToHash(href.slice(1));
    return;
  }
  navigate(href);
});

window.addEventListener("popstate", route);

elements.search.addEventListener("submit", (event) => {
  event.preventDefault();
  const query = elements.search.q.value.trim();
  closeMenu();
  navigate(`?search=${encodeURIComponent(query)}`);
});

elements.theme.addEventListener("click", () => {
  applyTheme(currentTheme() === "dark" ? "light" : "dark");
});

elements.menu.addEventListener("click", () => {
  document.body.classList.toggle("menu-open");
});

elements.searchButton.addEventListener("click", () => {
  document.body.classList.add("menu-open");
  setTimeout(() => elements.search.q.focus(), 50);
});

elements.overlay.addEventListener("click", closeMenu);

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    closeMenu();
    return;
  }
  const typing = event.target.closest && event.target.closest("input, textarea, select, [contenteditable]");
  if (event.key === "/" && !typing && !event.ctrlKey && !event.metaKey && !event.altKey) {
    event.preventDefault();
    if (window.matchMedia("(max-width: 900px)").matches) {
      document.body.classList.add("menu-open");
    }
    elements.search.q.focus();
  }
});

let scrollQueued = false;
window.addEventListener(
  "scroll",
  () => {
    if (scrollQueued) {
      return;
    }
    scrollQueued = true;
    requestAnimationFrame(() => {
      scrollQueued = false;
      updateActiveHeading();
    });
  },
  { passive: true },
);

window.addEventListener("resize", () => {
  if (!window.matchMedia("(max-width: 900px)").matches) {
    closeMenu();
  }
});

async function start() {
  elements.footer.innerHTML = `Pages are plain Markdown files in <code>docs/${site.pages}</code>. The sidebar comes from <code>docs/${site.sidebar}</code>.`;
  try {
    const parsed = parseSidebar(await load(site.sidebar));
    state.sections = parsed.sections;
    state.pages = parsed.pages;
    for (const entry of parsed.pages) {
      if (!state.byId.has(entry.id)) {
        state.byId.set(entry.id, entry);
      }
    }
  } catch (error) {
    if (location.protocol === "file:") {
      showNotice("Open with a web server", localFileNotice());
    } else {
      showNotice("Sidebar missing", `The file <code>docs/${site.sidebar}</code> could not be loaded.`);
    }
    return;
  }
  route();
  showVersion();
}

start();
