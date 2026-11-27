#!/usr/bin/env node
// Check relative links in tracked Markdown files.
//
// Usage: node scripts/check-links.mjs [--all]
//
// By default only repository-relative Markdown is checked (README, docs,
// contributing guides). Pass --all to also check files under docs/src that
// are consumed by mdBook. External http(s) links are intentionally not
// fetched: that would make CI depend on third-party availability.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");

function trackedMarkdown() {
  const out = execFileSync("git", ["ls-files", "*.md"], {
    cwd: root,
    encoding: "utf8",
  });
  return out
    .split(/\r?\n/)
    .filter(Boolean)
    .filter((f) => existsSync(join(root, f)));
}

const files = trackedMarkdown();
const problems = [];

// [text](target) and [text](<target>) plus ![alt](target)
const inlineLink = /!?\[[^\]]*\]\(\s*(<[^>]+>|[^)\s]+)(?:\s+"[^"]*")?\s*\)/g;
// [label]: target
const referenceLink = /^\s{0,3}\[[^\]]+\]:\s*(\S+)/gm;

function isExternal(target) {
  return /^(https?:|mailto:|tel:|data:|ftp:)/i.test(target);
}

function check(file, target, line) {
  let clean = target.trim();
  if (clean.startsWith("<") && clean.endsWith(">")) {
    clean = clean.slice(1, -1);
  }
  if (!clean || clean.startsWith("#") || isExternal(clean)) return;
  // Strip anchor, query string, and trailing slash.
  clean = clean.split("#")[0].split("?")[0].replace(/\/$/, "");
  if (!clean) return;
  // Ignore template placeholders.
  if (clean.includes("{{") || clean.includes("${{")) return;

  const fromDir = dirname(join(root, file));
  const absolute = clean.startsWith("/")
    ? join(root, clean.slice(1))
    : join(fromDir, clean);

  if (!existsSync(absolute)) {
    problems.push(`${file}:${line}: missing target ${target}`);
  }
  // Directory links are valid on GitHub even without an index page.
}

for (const file of files) {
  const text = readFileSync(join(root, file), "utf8");
  const lines = text.split(/\r?\n/);
  lines.forEach((lineText, index) => {
    for (const match of lineText.matchAll(inlineLink)) {
      check(file, match[1], index + 1);
    }
    for (const match of lineText.matchAll(referenceLink)) {
      check(file, match[1], index + 1);
    }
  });
}

if (problems.length > 0) {
  console.error(`Broken relative links: ${problems.length}`);
  for (const problem of problems) console.error(`  ${problem}`);
  process.exit(1);
}

console.log(`Link check passed for ${files.length} Markdown files.`);
