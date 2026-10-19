// Checks a commit message: conventional form and no tool attribution.
// Usage: bun tools/check-commit-message.ts <message-file>
//        bun tools/check-commit-message.ts --range <from>..<to>
import { attributionLines } from './attribution.ts';

const TYPES = ['feat', 'fix', 'perf', 'refactor', 'docs', 'test', 'build', 'ci', 'chore', 'revert'];
const HEADER = new RegExp(`^(${TYPES.join('|')})(\\([a-z0-9-]+\\))?!?: [^A-Z\\s](.*[^.])?$`, 'u');
const MAX_HEADER = 72;
// Messages git writes itself.
const GENERATED = /^(Merge |Revert "|fixup! |squash! |amend! )/u;

const problems = (message: string): string[] => {
  const text = message
    .split('\n')
    .filter((line) => !line.startsWith('#'))
    .join('\n')
    .trim();
  const header = text.split('\n')[0] ?? '';
  const found: string[] = [];

  if (!GENERATED.test(header)) {
    if (!HEADER.test(header)) {
      found.push(
        `"${header}" is not "<type>(<scope>): <subject>" with a type of ${TYPES.join(', ')}, ` +
          'a lower-case subject and no final period',
      );
    }

    if (header.length > MAX_HEADER) {
      found.push(`the header is ${header.length} characters, at most ${MAX_HEADER}`);
    }
  }

  for (const { line, text: content } of attributionLines(text)) {
    found.push(`line ${line} carries tool attribution: "${content.trim()}"`);
  }

  return found;
};

const messages = async (args: string[]): Promise<Array<{ name: string; text: string }>> => {
  if (args[0] === '--range' && args[1]) {
    const log = Bun.spawnSync(['git', 'log', '--format=%H%x00%B%x01', args[1]]);
    if (log.exitCode !== 0) {
      throw new Error(log.stderr.toString());
    }

    return log.stdout
      .toString()
      .split('\x01')
      .map((entry) => entry.trim())
      .filter(Boolean)
      .map((entry) => {
        const [hash, body] = entry.split('\0');

        return { name: (hash ?? '').slice(0, 7), text: body ?? '' };
      });
  }

  if (!args[0]) {
    throw new Error('usage: check-commit-message.ts <message-file> | --range <from>..<to>');
  }

  return [{ name: 'this commit', text: await Bun.file(args[0]).text() }];
};

let failed = false;

for (const { name, text } of await messages(Bun.argv.slice(2))) {
  for (const problem of problems(text)) {
    console.error(`${name}: ${problem}`);
    failed = true;
  }
}

process.exit(failed ? 1 : 0);
