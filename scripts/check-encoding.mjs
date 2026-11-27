#!/usr/bin/env node
// Guard tracked text files against BOMs, invalid UTF-8, NUL bytes, and the
// most common UTF-8/Windows-1252 mojibake sequences.
//
// Usage: node scripts/check-encoding.mjs

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { extname, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");

const textExtensions = new Set([
  ".md",
  ".txt",
  ".rs",
  ".toml",
  ".yml",
  ".yaml",
  ".json",
  ".svelte",
  ".ts",
  ".js",
  ".mjs",
  ".css",
  ".html",
  ".cff",
  ".sh",
  ".ps1",
  ".bat",
  ".yml",
]);

// Sequences produced when UTF-8 punctuation is decoded as Windows-1252.
// Stored as Unicode escapes so the file itself contains no mojibake.
const mojibake = [
  "\u00E2\u20AC", // a-euro sign (a-circumflex + euro)
  "\u00E2\u2020", // a-dagger
  "\u00E2\u2030", // a-permille
  "\u00E2\u2026", // a-ellipsis
  "\u00C3\u00A9", // A-eacute
  "\u00C3\u00A8", // A-grave
  "\u00C3\u00A2", // A-circumflex
  "\u00C3\u00BC", // A-umlaut-u
  "\u00C3\u00B6", // A-umlaut-o
  "\u00C3\u00A4", // A-umlaut-a
];

const files = execFileSync("git", ["ls-files"], { cwd: root, encoding: "utf8" })
  .split(/\r?\n/)
  .filter(Boolean)
  .filter((file) => textExtensions.has(extname(file).toLowerCase()));

const problems = [];

for (const file of files) {
  const path = join(root, file);
  if (!existsSync(path)) continue;
  const bytes = readFileSync(path);

  if (bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) {
    problems.push(`${file}: UTF-8 BOM`);
  }
  if (bytes.includes(0x00)) {
    problems.push(`${file}: NUL byte`);
  }

  const text = bytes.toString("utf8");
  if (Buffer.from(text, "utf8").compare(bytes) !== 0) {
    problems.push(`${file}: not valid UTF-8`);
    continue;
  }
  for (const needle of mojibake) {
    if (text.includes(needle)) {
      problems.push(`${file}: mojibake sequence ${JSON.stringify(needle)}`);
    }
  }
}

if (problems.length > 0) {
  console.error(`Encoding problems: ${problems.length}`);
  for (const problem of problems) console.error(`  ${problem}`);
  process.exit(1);
}

console.log(`Encoding check passed for ${files.length} text files.`);
