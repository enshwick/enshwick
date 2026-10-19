// Refuses tool attribution in files. Usage: bun tools/check-attribution.ts <file>...
import { attributionLines } from './attribution.ts';

let failed = false;

for (const path of Bun.argv.slice(2)) {
  const file = Bun.file(path);
  if (!(await file.exists())) {
    continue;
  }

  for (const { line, text } of attributionLines(await file.text())) {
    console.error(`${path}:${line}: tool attribution: "${text.trim()}"`);
    failed = true;
  }
}

process.exit(failed ? 1 : 0);
