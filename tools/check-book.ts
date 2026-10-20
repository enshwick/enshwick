// Checks the modding book's structure: every chapter SUMMARY.md links exists,
// every chapter is in SUMMARY.md, and every chapter's Rust examples compile
// as doctests (crates/enshwick-loader-api/src/book.rs, or src/test.rs for testing).
// Usage: bun tools/check-book.ts

import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

const SRC = 'book/src';
const HARNESSES = ['crates/enshwick-loader-api/src/book.rs', 'crates/enshwick-loader-api/src/test.rs'];

const chapters = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return chapters(path);
    return entry.name.endsWith('.md') && entry.name !== 'SUMMARY.md'
      ? [relative(SRC, path).split(sep).join('/')]
      : [];
  });

const summary = readFileSync(join(SRC, 'SUMMARY.md'), 'utf8');
const linked = [...summary.matchAll(/\]\(([^)]+\.md)\)/gu)].map((match) => match[1] ?? '');
const harnessed = HARNESSES.map((path) => readFileSync(path, 'utf8')).join('\n');
const problems: string[] = [];

for (const link of linked) {
  if (!existsSync(join(SRC, link))) problems.push(`SUMMARY.md links ${link}, which does not exist`);
}
for (const chapter of chapters(SRC)) {
  if (!linked.includes(chapter)) problems.push(`${chapter} is not in SUMMARY.md`);
  if (!harnessed.includes(`"${chapter}"`) && !harnessed.includes(`book/src/${chapter}"`)) {
    problems.push(`${chapter} is in no doctest harness`);
  }
}

if (problems.length > 0) {
  for (const problem of problems) console.error(problem);
  process.exit(1);
}
console.log(`book: ${linked.length} chapters checked`);
