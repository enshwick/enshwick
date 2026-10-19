// Patterns that mark text as attributed to a tool. Built from parts so this
// file does not match itself.
const parts = [
  ['co-authored', '-by:'],
  ['generated with', ' \\[?(claude|chatgpt|copilot|cursor)'],
  ['noreply@', 'anthropic\\.com'],
  ['\u{1F916}', ' generated'],
];

export const attributionPatterns: readonly RegExp[] = parts.map(
  ([head, tail]) => new RegExp(head + tail, 'iu'),
);

/** The lines of `text` that carry tool attribution, with their numbers. */
export const attributionLines = (text: string): Array<{ line: number; text: string }> =>
  text
    .split('\n')
    .map((line, index) => ({ line: index + 1, text: line }))
    .filter(({ text: line }) => attributionPatterns.some((pattern) => pattern.test(line)));
