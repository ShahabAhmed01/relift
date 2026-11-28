#!/usr/bin/env node
// Documentation consistency checker.
//
// Verifies that key implementation facts (test counts, registered carve format
// count) agree across README.md, BUILD_STATUS.md, FINAL_REPORT.md,
// PROJECT_SUMMARY.md, TEST_RESULTS.md, llms.txt, llms-full.txt, AGENTS.md, and
// the carve.rs format stability test. Run this in CI to prevent claim drift.
//
// Usage: node scripts/check-docs-consistency.mjs

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const problems = [];

function read(name) {
  return readFileSync(resolve(root, name), "utf8");
}

// --- Source of truth: registered carve format count from carve.rs ---
let actualFormatCount = null;
try {
  const carve = read("crates/relift-core/src/carve.rs");
  const m = carve.match(/formats\.len\(\)\s*,\s*(\d+)/);
  if (m) actualFormatCount = Number(m[1]);
} catch (e) {
  problems.push("could not read carve.rs format count");
}

function findCounts(text, keyword) {
  const counts = new Set();
  // Negative lookbehind prevents matching version numbers like "1.90.0 test".
  const pattern = new RegExp(`(?<![\\d.])\\b(\\d+)\\s+${keyword}\\b`, "gi");
  for (const m of text.matchAll(pattern)) {
    counts.add(Number(m[1]));
  }
  return counts;
}

function checkFormatCount(name, text) {
  if (actualFormatCount === null) return;
  const counts = findCounts(text, "format\\w*");
  for (const c of counts) {
    if (c !== actualFormatCount) {
      problems.push(
        `${name}: claims ${c} carve formats, but carve.rs registers ${actualFormatCount}`,
      );
    }
  }
}

function checkTestCount(name, text, expected) {
  const counts = findCounts(text, "tests?");
  for (const c of counts) {
    if (c !== expected) {
      problems.push(
        `${name}: claims ${c} tests, but expected ${expected}`,
      );
    }
  }
}

// Historical reports retain the values that were true for their dated runs.
// Only check the current portion of those records against today's source tree.
function currentClaims(name, text) {
  if (name === "FINAL_REPORT.md" && /Historical audit snapshot from\s+\d{4}-\d{2}-\d{2}/i.test(text)) {
    return "";
  }
  if (name === "TEST_RESULTS.md") {
    return text.split(/^The historical \d{4}-\d{2}-\d{2} run is retained below\b.*$/im, 1)[0];
  }
  return text;
}

// Collect test counts claimed across all docs to verify they agree with each other.
const docTestCounts = {};

// --- Check files ---
const files = {
  "README.md": read("README.md"),
  "BUILD_STATUS.md": read("BUILD_STATUS.md"),
  "FINAL_REPORT.md": read("FINAL_REPORT.md"),
  "PROJECT_SUMMARY.md": read("PROJECT_SUMMARY.md"),
  "TEST_RESULTS.md": read("docs/TEST_RESULTS.md"),
  "llms.txt": read("llms.txt"),
  "llms-full.txt": read("llms-full.txt"),
  "AGENTS.md": read("AGENTS.md"),
};

for (const [name, text] of Object.entries(files)) {
  const current = currentClaims(name, text);
  checkFormatCount(name, current);
  const counts = findCounts(current, "tests?");
  for (const c of counts) {
    docTestCounts[c] = (docTestCounts[c] || 0) + 1;
  }
}

// Also check docs/src copies for format counts
for (const f of ["support-matrix.md", "VERIFICATION.md", "GAP_REPORT.md", "limitations.md"]) {
  try {
    const text = read(`docs/src/${f}`);
    checkFormatCount(`docs/src/${f}`, text);
  } catch (e) {
    // file may not exist
  }
}

// If there are multiple distinct test counts across docs, that's drift.
const distinctTestCounts = Object.keys(docTestCounts).map(Number);
if (distinctTestCounts.length > 1) {
  problems.push(
    `inconsistent test counts across documentation: ${JSON.stringify(docTestCounts)}`,
  );
}

// TEST_RESULTS.md is the most granular; use its total as the expected count if present.
const resultsText = files["TEST_RESULTS.md"];
const totalMatch = resultsText.match(/Total:\s*(\d+)\s+tests?/);
if (totalMatch) {
  const expected = Number(totalMatch[1]);
  for (const [name, text] of Object.entries(files)) {
    checkTestCount(name, currentClaims(name, text), expected);
  }
}

if (problems.length > 0) {
  console.error(`Documentation consistency problems: ${problems.length}`);
  for (const p of problems) console.error(`  - ${p}`);
  process.exit(1);
}

const testSummary = distinctTestCounts.length === 0
  ? "no test counts found"
  : `${distinctTestCounts[0]} tests (${distinctTestCounts.length === 1 ? "consistent" : "INCONSISTENT"})`;
console.log(
  `Documentation consistency OK: ${testSummary}, ${actualFormatCount} carve formats across ${Object.keys(files).length + 4} files.`,
);
