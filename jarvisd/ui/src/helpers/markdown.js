import { copyText, esc, toast } from "./core.js";

const FENCE_RE = /^\s{0,3}```/;
const HR_RE = /^\s{0,3}([-*_])(?:\s*\1){2,}\s*$/;
const HEADING_RE = /^\s{0,3}(#{1,6})\s+(.+?)\s*#*$/;
const QUOTE_RE = /^\s{0,3}>\s?(.*)$/;
const UL_RE = /^(\s*)[-*+]\s+(.*)$/;
const OL_RE = /^(\s*)\d+[.)]\s+(.*)$/;

function safeUrl(raw) {
  const url = String(raw || "").trim();
  if (!/^https?:\/\//i.test(url)) return "";
  if (/[\u0000-\u0020\u007f]/.test(url)) return "";
  try {
    const u = new URL(url);
    return u.protocol === "http:" || u.protocol === "https:" ? u.toString() : "";
  } catch {
    return "";
  }
}

function formatText(src, codeTokens, tokens) {
  let s = src.replace(/\0/g, "");
  // Protect inline code first so HTML/Markdown inside backticks stays literal.
  s = s.replace(/`([^`\n]+)`/g, (_, code) => {
    const idx = codeTokens.length;
    codeTokens.push(`<code>${esc(code)}</code>`);
    return `\0C${idx}\0`;
  });
  // Protect safe external links before escaping raw HTML.
  s = s.replace(/\[([^\]\n]+)\]\(([^)\s\n]+)\)/g, (full, label, href) => {
    const clean = safeUrl(href);
    if (!clean) return full;
    const idx = tokens.length;
    const inner = formatStyledText(esc(label));
    tokens.push(`<a href="${esc(clean)}" target="_blank" rel="noopener noreferrer">${inner}</a>`);
    return `\0L${idx}\0`;
  });
  s = formatStyledText(esc(s));
  s = s.replace(/\0L(\d+)\0/g, (_, i) => tokens[Number(i)] || "");
  s = s.replace(/\0C(\d+)\0/g, (_, i) => codeTokens[Number(i)] || "");
  return s;
}

function formatStyledText(escaped) {
  return escaped
    .replace(/\*\*([^*\n]+?)\*\*/g, "<strong>$1</strong>")
    .replace(/~~([^~\n]+?)~~/g, "<del>$1</del>")
    .replace(/\*([^*\n]+?)\*/g, "<em>$1</em>")
    .replace(/(^|\s)_([^_\n]+?)_(?=\s|$|[.,!?;:])/g, "$1<em>$2</em>");
}

function formatInline(src) {
  return formatText(src, [], []);
}

export function renderMarkdown(markdown) {
  if (!markdown) return "";
  const lines = String(markdown).replace(/\r\n?/g, "\n").replace(/\0/g, "").split("\n");
  const out = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (!line.trim()) {
      i += 1;
      continue;
    }

    if (FENCE_RE.test(line)) {
      i += 1;
      const code = [];
      while (i < lines.length && !FENCE_RE.test(lines[i])) {
        code.push(lines[i]);
        i += 1;
      }
      if (i < lines.length && FENCE_RE.test(lines[i])) i += 1;
      out.push(`<pre><code>${esc(code.join("\n"))}</code></pre>`);
      continue;
    }

    if (HR_RE.test(line)) {
      out.push("<hr>");
      i += 1;
      continue;
    }

    const heading = line.match(HEADING_RE);
    if (heading) {
      const level = Math.min(heading[1].length + 2, 5);
      out.push(`<h${level}>${formatInline(heading[2])}</h${level}>`);
      i += 1;
      continue;
    }

    if (QUOTE_RE.test(line)) {
      const quoteLines = [];
      while (i < lines.length) {
        const match = lines[i].match(QUOTE_RE);
        if (!match) break;
        quoteLines.push(formatInline(match[1]));
        i += 1;
      }
      out.push(`<blockquote><p>${quoteLines.join("<br>")}</p></blockquote>`);
      continue;
    }

    const ulMatch = line.match(UL_RE);
    const olMatch = !ulMatch && line.match(OL_RE);
    if (ulMatch || olMatch) {
      const tag = ulMatch ? "ul" : "ol";
      const re = ulMatch ? UL_RE : OL_RE;
      const items = [];
      while (i < lines.length) {
        const m = lines[i].match(re);
        if (!m) break;
        const indent = m[1].length;
        const body = [formatInline(m[2].trim())];
        i += 1;
        while (i < lines.length && lines[i].trim() && !FENCE_RE.test(lines[i]) && !HEADING_RE.test(lines[i]) && !HR_RE.test(lines[i]) && !QUOTE_RE.test(lines[i])) {
          const nextUl = lines[i].match(UL_RE);
          const nextOl = lines[i].match(OL_RE);
          if (nextUl || nextOl) {
            const subIndent = (nextUl || nextOl)[1].length;
            if (subIndent <= indent) break;
            const subTag = nextUl ? "ul" : "ol";
            const subRe = nextUl ? UL_RE : OL_RE;
            const subItems = [];
            while (i < lines.length) {
              const sm = lines[i].match(subRe);
              if (!sm || sm[1].length <= indent) break;
              subItems.push(`<li>${formatInline(sm[2].trim())}</li>`);
              i += 1;
            }
            body.push(`<${subTag}>${subItems.join("")}</${subTag}>`);
            continue;
          }
          if (/^\s{2,}\S/.test(lines[i])) {
            body.push(`<br>${formatInline(lines[i].trim())}`);
            i += 1;
            continue;
          }
          break;
        }
        items.push(`<li>${body.join("")}</li>`);
      }
      out.push(`<${tag}>${items.join("")}</${tag}>`);
      continue;
    }

    const para = [];
    while (
      i < lines.length &&
      lines[i].trim() &&
      !FENCE_RE.test(lines[i]) &&
      !HR_RE.test(lines[i]) &&
      !HEADING_RE.test(lines[i]) &&
      !QUOTE_RE.test(lines[i]) &&
      !UL_RE.test(lines[i]) &&
      !OL_RE.test(lines[i])
    ) {
      para.push(formatInline(lines[i]));
      i += 1;
    }
    out.push(`<p>${para.join("<br>")}</p>`);
  }

  return out.join("");
}

function appendInlineDom(parent, text) {
  const tokenRe = /`([^`\n]+)`|\*\*([^*\n]+)\*\*|\*([^*\n]+)\*|\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)|(https?:\/\/[^\s<>()]+[^\s<>().,;!?])/g;
  let last = 0;
  let m;
  while ((m = tokenRe.exec(text)) !== null) {
    if (m.index > last) {
      parent.append(document.createTextNode(text.slice(last, m.index)));
    }
    if (m[1] !== undefined) {
      const code = document.createElement("code");
      code.className = "md-code";
      code.textContent = m[1];
      parent.append(code);
    } else if (m[2] !== undefined) {
      const strong = document.createElement("strong");
      strong.textContent = m[2];
      parent.append(strong);
    } else if (m[3] !== undefined) {
      const em = document.createElement("em");
      em.textContent = m[3];
      parent.append(em);
    } else if (m[4] !== undefined && m[5] !== undefined) {
      const clean = safeUrl(m[5]);
      if (clean) {
        const a = document.createElement("a");
        a.className = "md-link";
        a.href = clean;
        a.target = "_blank";
        a.rel = "noopener noreferrer";
        a.textContent = m[4];
        parent.append(a);
      } else {
        parent.append(document.createTextNode(m[0]));
      }
    } else if (m[6] !== undefined) {
      const clean = safeUrl(m[6]);
      if (clean) {
        const a = document.createElement("a");
        a.className = "md-link";
        a.href = clean;
        a.target = "_blank";
        a.rel = "noopener noreferrer";
        a.textContent = m[6];
        parent.append(a);
      } else {
        parent.append(document.createTextNode(m[6]));
      }
    }
    last = tokenRe.lastIndex;
  }
  if (last < text.length) {
    parent.append(document.createTextNode(text.slice(last)));
  }
}

function makeCodeBlockDom(lines, lang) {
  const wrap = document.createElement("div");
  wrap.className = "md-codeblock";

  const head = document.createElement("div");
  head.className = "md-codehead";

  const tag = document.createElement("span");
  tag.textContent = lang || "код";

  const codeText = lines.join("\n");
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "md-copy";
  btn.textContent = "Копировать";
  btn.addEventListener("click", () => {
    void copyText(codeText).then(
      () => toast("Код скопирован"),
      () => toast("Не удалось скопировать", true),
    );
  });

  head.append(tag, btn);

  const pre = document.createElement("pre");
  pre.className = "md-pre";
  const code = document.createElement("code");
  code.textContent = codeText;
  pre.append(code);

  wrap.append(head, pre);
  return wrap;
}

export function renderMarkdownInto(container, rawText) {
  const text = String(rawText ?? "").replace(/\r\n?/g, "\n");
  container.replaceChildren();
  if (!text) return;

  const lines = text.split("\n");
  let inCode = false;
  let codeLang = "";
  let codeLines = [];
  let paraLines = [];
  let listEl = null;
  let listTag = "";

  function flushPara() {
    if (!paraLines.length) return;
    const p = document.createElement("p");
    p.className = "md-p";
    paraLines.forEach((line, idx) => {
      if (idx > 0) p.append(document.createElement("br"));
      appendInlineDom(p, line);
    });
    container.append(p);
    paraLines = [];
  }

  function flushList() {
    if (!listEl) return;
    container.append(listEl);
    listEl = null;
    listTag = "";
  }

  for (const line of lines) {
    const fence = line.match(/^\s{0,3}```([a-zA-Z0-9_+-]*)\s*$/);
    if (fence) {
      if (inCode) {
        container.append(makeCodeBlockDom(codeLines, codeLang));
        inCode = false;
        codeLang = "";
        codeLines = [];
      } else {
        flushPara();
        flushList();
        inCode = true;
        codeLang = fence[1] || "";
        codeLines = [];
      }
      continue;
    }

    if (inCode) {
      codeLines.push(line);
      continue;
    }

    if (!line.trim()) {
      flushPara();
      flushList();
      continue;
    }

    const ulMatch = line.match(/^\s*[-*]\s+(.+)$/);
    if (ulMatch) {
      flushPara();
      if (listTag !== "ul") {
        flushList();
        listTag = "ul";
        listEl = document.createElement("ul");
        listEl.className = "md-list";
      }
      const li = document.createElement("li");
      appendInlineDom(li, ulMatch[1]);
      listEl.append(li);
      continue;
    }

    const olMatch = line.match(/^\s*\d+[.)]\s+(.+)$/);
    if (olMatch) {
      flushPara();
      if (listTag !== "ol") {
        flushList();
        listTag = "ol";
        listEl = document.createElement("ol");
        listEl.className = "md-list";
      }
      const li = document.createElement("li");
      appendInlineDom(li, olMatch[1]);
      listEl.append(li);
      continue;
    }

    flushList();
    paraLines.push(line);
  }

  if (inCode) {
    container.append(makeCodeBlockDom(codeLines, codeLang));
  }
  flushPara();
  flushList();
}
