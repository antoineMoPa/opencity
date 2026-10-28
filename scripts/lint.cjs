#!/usr/bin/env node
// Keep this script identical in mtl_jobines and OpenCity; each repo works independently.
const fs = require("fs");
const path = require("path");
const { execFileSync, spawnSync } = require("child_process");
const root = path.resolve(__dirname, "..");
process.chdir(root);
const args = process.argv.slice(2);
const staged = args.includes("--staged");
const fix = args.includes("--fix");
if (args.some((arg) => !["--staged", "--fix"].includes(arg)) || (staged && fix)) {
  console.error("Usage: node scripts/lint.cjs [--staged | --fix]");
  process.exit(2);
}
function git(args) {
  return execFileSync("git", args, { cwd: root, maxBuffer: 32 * 1024 * 1024 });
}
const files = [
  ...new Set(
    git(
      staged
        ? ["diff", "--cached", "--name-only", "--diff-filter=ACMR", "-z"]
        : ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    )
      .toString("utf8")
      .split("\0")
      .filter((file) => /\.(md|rs)$/.test(file)),
  ),
].filter((file) => staged || fs.existsSync(file));
const rust = files.filter((file) => file.endsWith(".rs"));
const markdown = files.filter((file) => file.endsWith(".md"));
let failed = false;
function run(command, args, input) {
  const result = spawnSync(command, args, {
    cwd: root,
    input,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) {
    console.error(result.error ? result.error.message : result.stdout + result.stderr);
    failed = true;
  }
  return result;
}
let prettier;
if (markdown.length) {
  try {
    prettier = path.join(
      path.dirname(require.resolve("prettier/package.json")),
      "bin/prettier.cjs",
    );
  } catch (_) {
    console.error("Missing Markdown formatter. Run npm ci, then make fmt or make lint.");
    process.exit(1);
  }
}
if (!staged) {
  if (rust.length)
    run("rustfmt", [
      ...(fix ? [] : ["--check"]),
      "--edition",
      "2021",
      "--config",
      "skip_children=true",
      "--config-path",
      root,
      ...rust,
    ]);
  if (markdown.length)
    run(process.execPath, [prettier, fix ? "--write" : "--check", "--", ...markdown]);
}
for (const file of files) {
  const source = staged
    ? git(["show", ":" + file]).toString("utf8")
    : fs.readFileSync(file, "utf8");
  const isRust = file.endsWith(".rs");
  if (staged) {
    const result = isRust
      ? run(
          "rustfmt",
          [
            "--edition",
            "2021",
            "--config",
            "skip_children=true",
            "--config-path",
            root,
            "--emit",
            "stdout",
          ],
          source,
        )
      : run(process.execPath, [prettier, "--stdin-filepath", file], source);
    if (result.status === 0 && result.stdout !== source) {
      console.error(file + ": formatting differs; run make fmt and stage the result.");
      failed = true;
    }
  }
  const lines = source.split(/\r?\n/);
  if (lines[lines.length - 1] === "") lines.pop();
  if (isRust && lines.length > 1200) {
    console.error(file + ": " + lines.length + " lines; Rust files must have at most 1200.");
    failed = true;
  }
  lines.forEach((line, index) => {
    const width = Array.from(line.replace(/\t/g, "    ")).length;
    if (width > 100) {
      console.error(file + ":" + (index + 1) + ": " + width + " columns; maximum is 100.");
      failed = true;
    }
  });
}
if (failed) {
  console.error("Run make fmt; split long Rust modules and wrap remaining long lines manually.");
  process.exit(1);
}
console.log("Formatting and source limits passed" + (staged ? " for staged files." : "."));
