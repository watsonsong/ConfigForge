import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Marked } from 'marked';

const root = fileURLToPath(new URL('../../', import.meta.url));
const markdown = new Marked({ gfm: true });
const slug = text => text.toLowerCase().replace(/<[^>]*>/g, '')
  .replace(/[^\p{L}\p{N}\p{M}_\-\s]/gu, '').replace(/ /g, '-');
const files = ['README.md', 'README.zh-CN.md', 'AGENTS.md'];
function discover(directory) {
  for (const entry of readdirSync(path.join(root, directory), { withFileTypes: true })) {
    const name = `${directory}/${entry.name}`;
    if (entry.isDirectory() && entry.name !== 'DevLogs') discover(name);
    else if (entry.isFile() && entry.name.endsWith('.md')) files.push(name);
  }
}
discover('Documentation');
const cache = new Map();
function parse(file) {
  if (cache.has(file)) return cache.get(file);
  const tokens = markdown.lexer(readFileSync(file, 'utf8'));
  const anchors = new Set();
  const counts = new Map();
  const links = [];
  markdown.walkTokens(tokens, token => {
    if (token.type === 'heading') {
      const base = slug(token.text);
      const count = counts.get(base) ?? 0;
      counts.set(base, count + 1);
      anchors.add(`${base}${count ? `-${count}` : ''}`);
    }
    if (token.type === 'link' || token.type === 'image') links.push(token.href);
  });
  const result = { anchors, links };
  cache.set(file, result);
  return result;
}
const errors = [];
let checked = 0;
for (const name of files) {
  const file = path.join(root, name);
  for (const href of parse(file).links) {
    if (/^(?:[a-z]+:|\/\/)/i.test(href)) continue;
    checked++;
    const [relative, fragment] = href.split('#');
    const target = path.resolve(path.dirname(file), decodeURIComponent(relative || path.basename(file)));
    if (!existsSync(target)) errors.push(`${name}: missing ${href}`);
    else if (fragment && target.endsWith('.md') && !parse(target).anchors.has(decodeURIComponent(fragment))) {
      errors.push(`${name}: missing heading ${href}`);
    }
  }
}
if (errors.length) throw new Error(errors.join('\n'));
console.log(`Checked ${checked} local links in ${files.length} current Markdown documents (DevLogs excluded)`);
