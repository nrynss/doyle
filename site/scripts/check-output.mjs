// Post-build link and source prose check.
//
// 1. Source prose check: every markdown/mdx documentation source under
//    `src/content/docs/` must satisfy the documentation style rules:
//    no semicolons, no em-dashes, and <= 30 words per prose sentence.
// 2. HTML and asset link check: every local link and asset the build emits
//    is resolved against `dist/` before deployment. Base-prefixed URLs only.

import { readFile, readdir, stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const exists = async (path) => {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
};

// ---------------------------------------------------------------------------
// Source prose validation rules.

function getBlocks(rawText) {
  const lines = rawText.split(/\r?\n/);
  const blocks = [];
  let currentBlock = [];

  for (const line of lines) {
    let trimmed = line.trim();
    if (!trimmed) {
      if (currentBlock.length) {
        blocks.push(currentBlock.join(' '));
        currentBlock = [];
      }
      continue;
    }

    if (trimmed.startsWith('>')) {
      trimmed = trimmed.replace(/^>\s*/, '').trim();
      if (!trimmed) continue;
    }

    const isHeading = /^#+\s+/.test(trimmed);
    const isListItem = /^[-*+]\s+/.test(trimmed) || /^\d+\.\s+/.test(trimmed);

    if (isHeading || isListItem) {
      if (currentBlock.length) {
        blocks.push(currentBlock.join(' '));
        currentBlock = [];
      }
      const cleaned = trimmed
        .replace(/^#+\s+/, '')
        .replace(/^[-*+]\s+/, '')
        .replace(/^\d+\.\s+/, '');
      currentBlock.push(cleaned);
    } else {
      currentBlock.push(trimmed);
    }
  }

  if (currentBlock.length) {
    blocks.push(currentBlock.join(' '));
  }

  return blocks;
}

function splitSentences(text) {
  const protectedText = text.replace(/(\d)\.(\d)/g, '$1__DECIMAL__$2');
  const rawSentences = protectedText.split(/(?<=[.?!]["\x27)\x5D*_]*)\s+/);
  return rawSentences
    .map((s) => s.replace(/__DECIMAL__/g, '.').trim())
    .filter(Boolean);
}

function checkSourceProse(content, filePath) {
  let text = content.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, '');
  text = text.replace(/^(\s*)(?:`{3,}|~{3,})[^\n]*\r?\n[\s\S]*?\r?\n\1(?:`{3,}|~{3,})\s*$/gm, '');
  text = text.replace(/<!--[\s\S]*?-->/g, '');
  text = text.replace(/^\s*import\s+.*$/gm, '');
  text = text.replace(/<svg[\s\S]*?<\/svg>/gi, '');
  text = text.replace(/<\/?[A-Za-z][A-Za-z0-9]*(?:\s+[^>]*)?\/?>/g, '');
  text = text.replace(/^\s*\|.*$/gm, '');
  text = text.replace(/!?\[([^\]]*)\]\([^)]+\)/g, '$1');
  text = text.replace(/`+[^`]+`+/g, 'CODE');

  if (text.includes(';')) {
    const lines = text.split('\n');
    const lineIndex = lines.findIndex((l) => l.includes(';'));
    throw new Error(
      `Prose rule violation: semicolon (;) found in ${filePath} at line ~${lineIndex + 1}:\n${lines[lineIndex]}`,
    );
  }
  if (text.includes('—')) {
    const lines = text.split('\n');
    const lineIndex = lines.findIndex((l) => l.includes('—'));
    throw new Error(
      `Prose rule violation: em-dash (—) found in ${filePath} at line ~${lineIndex + 1}:\n${lines[lineIndex]}`,
    );
  }

  const blocks = getBlocks(text);
  let maxWords = 0;
  let totalSentences = 0;

  for (const block of blocks) {
    const sentences = splitSentences(block);
    for (const sentence of sentences) {
      const words = sentence.trim().split(/\s+/).filter(Boolean);
      const count = words.length;
      if (count > maxWords) maxWords = count;
      totalSentences++;
      if (count > 30) {
        throw new Error(
          `Prose rule violation: sentence exceeds 30 words (${count} words) in ${filePath}:\n"${sentence}"`,
        );
      }
    }
  }

  return { totalSentences, maxWords };
}

// ---------------------------------------------------------------------------
// 1. Source prose check over all `site/src/content/docs/**/*.{md,mdx}` files.

const docsRoot = resolve('src/content/docs');
const docFiles = [];
async function walkDocs(dir) {
  try {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) await walkDocs(path);
      else if (/\.(md|mdx)$/i.test(entry.name)) docFiles.push(path);
    }
  } catch {
    // Directory might not exist yet
  }
}
await walkDocs(docsRoot);

for (const file of docFiles) {
  const content = await readFile(file, 'utf8');
  checkSourceProse(content, file);
}
console.log(`Checked prose in ${docFiles.length} source documentation file(s) (all passed prose rules).`);

// ---------------------------------------------------------------------------
// 2. HTML and asset checks on `dist/`.

const root = resolve('dist');
const base = (process.env.SITE_BASE ?? '/doyle').replace(/\/$/, '');
const proseOnly = process.argv.includes('--prose-only');

if (proseOnly) {
  console.log('Skipping dist/ HTML checks (--prose-only specified).');
} else {
  if (!(await exists(root))) {
    throw new Error('dist/ does not exist — run npm run build first');
  }

  const htmlFiles = [];
  async function walk(dir) {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.html')) htmlFiles.push(path);
    }
  }
  await walk(root);

  // The fifteen documentation pages plus 404.html.
  if (htmlFiles.length < 16) {
    throw new Error(`Expected at least 16 rendered routes (15 pages and 404.html), found ${htmlFiles.length}`);
  }

  const missing = new Set();
  const unsearchable = [];

  for (const file of htmlFiles) {
    const html = await readFile(file, 'utf8');

    for (const [, url] of html.matchAll(new RegExp(`(?:href|src)="(${base}/[^"#?]+)"`, 'g'))) {
      const relative = decodeURIComponent(url.slice(base.length));
      const path = resolve(root, `.${relative}`);
      if (await exists(path)) continue;
      // A route without its extension resolves to its directory's index.html,
      // whether or not the link carried the trailing slash.
      if (await exists(join(path, 'index.html'))) continue;
      missing.add(`${url} from ${file}`);
    }

    if (!html.includes('id="starlight__search"') && !file.endsWith('/404.html')) {
      unsearchable.push(file);
    }
  }

  if (missing.size) {
    throw new Error(`Missing local assets or pages:\n${[...missing].sort().join('\n')}`);
  }
  if (unsearchable.length) {
    throw new Error(`Missing Starlight search markup:\n${unsearchable.sort().join('\n')}`);
  }

  console.log(`Checked ${htmlFiles.length} HTML pages and every base-prefixed local link and asset.`);
}
