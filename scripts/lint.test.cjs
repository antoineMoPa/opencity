// Exercise the checker against a disposable Git index, never the user's index.
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync, spawnSync } = require("child_process");
const source = path.resolve(__dirname, "..");
const root = fs.mkdtempSync(path.join(os.tmpdir(), "source-lint-"));
function git(...args) {
  return execFileSync("git", args, { cwd: root, stdio: "pipe" });
}
function check(args, passes, message) {
  const index = git("diff", "--cached", "--binary").toString();
  const result = spawnSync(process.execPath, ["scripts/lint.cjs", ...args], {
    cwd: root,
    encoding: "utf8",
  });
  assert.strictEqual(result.status === 0, passes, message + "\n" + result.stdout + result.stderr);
  assert.strictEqual(git("diff", "--cached", "--binary").toString(), index);
}
try {
  fs.mkdirSync(path.join(root, "scripts"));
  for (const file of ["scripts/lint.cjs", "rustfmt.toml", ".prettierrc.json"]) {
    fs.copyFileSync(path.join(source, file), path.join(root, file));
  }
  fs.symlinkSync(path.join(source, "node_modules"), path.join(root, "node_modules"), "dir");
  fs.writeFileSync(path.join(root, ".gitignore"), "node_modules/\nignored.md\n");
  git("init", "--quiet");
  const rust = path.join(root, "source with spaces.rs");
  const markdown = path.join(root, "guide with spaces.md");
  const validRust = ("//" + "x".repeat(98) + "\n").repeat(1200);
  const validMarkdown = "x".repeat(100) + "\n";
  fs.writeFileSync(rust, validRust);
  fs.writeFileSync(markdown, validMarkdown);
  fs.writeFileSync(path.join(root, "ignored.md"), "x".repeat(101));
  check([], true, "100 columns and 1200 Rust lines are allowed; ignored files are skipped");
  git("add", "--", "source with spaces.rs", "guide with spaces.md");
  fs.appendFileSync(rust, "// extra line\n");
  check([], false, "1201-line Rust files must fail");
  fs.writeFileSync(rust, validRust);
  fs.writeFileSync(markdown, "x".repeat(101) + "\n");
  check([], false, "101-column Markdown lines must fail");
  fs.appendFileSync(rust, "// extra line\n");
  check(["--staged"], true, "unstaged changes must not affect the staged check");
  check([], false, "working tree violations must fail");
  git("add", "--", "source with spaces.rs", "guide with spaces.md");
  fs.writeFileSync(rust, validRust);
  fs.writeFileSync(markdown, validMarkdown);
  check(["--staged"], false, "staged violations must fail even with a valid working tree");
  git("add", "--", "source with spaces.rs", "guide with spaces.md");
  fs.writeFileSync(rust, "fn main(){let _x=1;}\n");
  git("add", "--", "source with spaces.rs");
  check(["--staged"], false, "unformatted staged Rust must fail");
  check(["--fix"], true, "formatting should repair ordinary Rust layout");
  git("add", "--", "source with spaces.rs");
  fs.writeFileSync(markdown, "hello   world\n");
  git("add", "--", "guide with spaces.md");
  check(["--staged"], false, "unformatted staged Markdown must fail");
  fs.writeFileSync(markdown, validMarkdown);
  fs.writeFileSync(rust, "//" + "x".repeat(99) + "\n");
  check([], false, "101-column Rust lines must fail");
  fs.writeFileSync(rust, validRust);
  fs.writeFileSync(path.join(root, "new.md"), "x".repeat(101) + "\n");
  check([], false, "unignored new files must be checked");
  fs.unlinkSync(path.join(root, "new.md"));
  git("rm", "--cached", "--force", "--", "guide with spaces.md");
  fs.unlinkSync(markdown);
  git("add", "--", "source with spaces.rs");
  check(["--staged"], true, "staged deletions must be skipped");
  assert.strictEqual(fs.readFileSync(rust, "utf8"), validRust, "checks must not modify files");
  console.log("Source lint regression checks passed.");
} finally {
  fs.rmSync(root, { recursive: true, force: true });
}
