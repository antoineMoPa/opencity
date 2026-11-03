# Cleanup task

```sh
cargo xtask                    # Format Rust and wrap Markdown prose, then check limits.
cargo xtask --staged           # Format staged source snapshots and update the index.
cargo xtask --check            # Read-only check of working-tree sources.
cargo xtask --staged --check   # Read-only check of staged snapshots.
```

Rust sources use rustfmt, including the standalone xtask package. Markdown prose wraps at
100 columns while preserving frontmatter, raw HTML, code fences, indented code,
tables and inline code.
Long code/table lines, unbreakable tokens and Rust modules over 1200 lines still
need manual changes.

Staged formatting updates the working copy when it matches the staged contents.
For partially staged files, only the staged snapshot is formatted; unstaged bytes
remain untouched. Source-file modes are preserved. The pre-commit hook already
runs this command, so it now applies safe formatting fixes automatically.

`make fmt` applies formatting; `make lint` checks without writing.
