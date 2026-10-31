import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Marked } from 'marked';

const root = fileURLToPath(new URL('../../', import.meta.url));
const design = path.join(root, 'Documentation/Design');
const files = ['README.md', ...readdirSync(design).filter(name => /^\d{2}-.+\.md$/.test(name)).sort()];
const ids = new Map(files.map((name, index) => [name, `design-${index}`]));
const escape = value => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;')
  .replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const slug = value => value.toLowerCase().replace(/<[^>]*>/g, '').replace(/[^\p{L}\p{N}\p{M}_\-\s]/gu, '').replace(/ /g, '-');
const sections = [];
const navigation = [];

for (const name of files) {
  const source = readFileSync(path.join(design, name), 'utf8').replaceAll('\r\n', '\n');
  const id = ids.get(name);
  const title = source.match(/^# (.+)$/m)[1];
  const counts = new Map();
  const markdown = new Marked({ gfm: true });
  markdown.use({ renderer: {
    heading({ tokens, depth, text }) {
      const base = slug(text);
      const count = counts.get(base) ?? 0;
      counts.set(base, count + 1);
      const anchor = `${id}-${base}${count ? `-${count}` : ''}`;
      return `<h${depth} id="${escape(anchor)}">${this.parser.parseInline(tokens)}</h${depth}>\n`;
    },
    link({ href, title, tokens }) {
      let target = href;
      if (!/^(?:[a-z]+:|\/\/)/i.test(href)) {
        const [file, fragment] = href.split('#');
        const resolved = path.resolve(design, file || name);
        const designId = ids.get(path.basename(resolved));
        if (path.dirname(resolved) === design && designId) {
          target = `#${designId}${fragment ? `-${decodeURIComponent(fragment)}` : ''}`;
        } else {
          target = `${path.relative(path.join(design, 'html'), resolved).split(path.sep).join('/')}${fragment ? `#${fragment}` : ''}`;
        }
      }
      return `<a href="${escape(target)}"${title ? ` title="${escape(title)}"` : ''}>${this.parser.parseInline(tokens)}</a>`;
    }
  } });
  navigation.push(`<a href="#${id}">${escape(title)}</a>`);
  sections.push(`<article id="${id}"><p class="source"><a href="../${name}">Markdown 源文档：${name}</a></p>${markdown.parse(source)}</article>`);
}

const html = `<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ConfigForge 设计文档阅读版</title>
<style>
*{box-sizing:border-box}html{scroll-behavior:smooth}body{margin:0;color:#243044;background:#fff;font:16px/1.7 system-ui,"Microsoft YaHei",sans-serif}a{color:#4338ca}header{padding:24px 32px;border-bottom:1px solid #d8dee8}header p{margin:6px 0}.layout{display:grid;grid-template-columns:260px minmax(0,1fr);max-width:1480px;margin:auto}nav{position:sticky;top:0;align-self:start;max-height:100vh;overflow:auto;padding:24px 16px}nav a{display:block;padding:7px 10px;font-size:14px}main{min-width:0;padding:0 32px 48px}article{padding:30px 0;border-bottom:1px solid #d8dee8;overflow-wrap:anywhere}h1,h2,h3,h4{line-height:1.4;scroll-margin-top:16px}h1{font-size:28px}h2{margin-top:36px;font-size:23px}h3{margin-top:28px;font-size:19px}code,pre{font-family:ui-monospace,Consolas,monospace}code{background:#f1f3f7;padding:2px 4px;border-radius:4px;font-size:.9em}pre{overflow:auto;padding:16px;background:#f5f6f8;border:1px solid #d8dee8;border-radius:10px;line-height:1.5}pre code{padding:0;background:none}table{display:block;max-width:100%;overflow:auto;border-collapse:collapse;font-size:14px}td,th{padding:9px 12px;border:1px solid #d8dee8;text-align:left;vertical-align:top;min-width:90px}th{background:#f5f6f8}blockquote{margin-left:0;padding:8px 16px;border-left:3px solid #a5b4fc;background:#f8f9fc}.source{font-size:13px;color:#667085}.task-list-item{list-style:none}@media(max-width:800px){.layout{display:block}nav{position:static;max-height:none;border-bottom:1px solid #d8dee8}main{padding:0 18px 32px}header{padding:20px 18px}h1{font-size:24px}}@media print{nav,header,.source{display:none}.layout{display:block}main{padding:0}article{break-before:page}table{display:table;font-size:10px}pre{white-space:pre-wrap}a{color:inherit;text-decoration:none}}
</style>
</head>
<body>
<header><h1>ConfigForge 设计文档阅读版</h1><p>由 Design Markdown 自动生成。涵盖当前契约、实现状态和明确标注的未来扩展。</p><p>修改源文档后运行 <code>npm run docs:build --prefix tools/docs</code>；工程维护方式见 <a href="../../Tech/documentation-maintenance.md">文档维护流程</a>。</p></header>
<div class="layout"><nav aria-label="设计文档目录">${navigation.join('\n')}</nav><main>${sections.join('\n')}</main></div>
</body>
</html>
`;

const anchors = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
const anchorSet = new Set(anchors);
if (anchorSet.size !== anchors.length) throw new Error('Duplicate HTML anchors');
for (const [, anchor] of html.matchAll(/href="#([^"]+)"/g)) {
  if (!anchorSet.has(anchor)) throw new Error(`Broken HTML anchor: ${anchor}`);
}
const output = path.join(design, 'html/index.html');
if (process.argv.includes('--check')) {
  if (readFileSync(output, 'utf8').replaceAll('\r\n', '\n') !== html) {
    throw new Error('Design HTML is stale; run npm run docs:build --prefix tools/docs');
  }
  console.log(`Design HTML is current: ${files.length} documents, ${anchors.length} anchors`);
} else {
  writeFileSync(output, html);
  console.log(`Rendered ${files.length} design documents to Documentation/Design/html/index.html`);
}
