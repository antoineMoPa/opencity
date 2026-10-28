# Formatting and source limits

Run `npm ci` once to install the pinned Markdown formatter. Rust formatting uses the installed
`rustfmt` component (`rustup component add rustfmt`). Node.js 14.17 or newer is required.

- `make fmt` formats repository-owned Rust and Markdown files, then checks the hard limits.
- `make lint` checks formatting, 100-character lines in Markdown and Rust, and a maximum of 1,200
  physical lines per Rust file. It includes tracked and unignored new files.
- `make test-lint` runs isolated regression checks for the checker and staged-file behavior.
- `make hooks` enables the checked-in pre-commit hook for this checkout. Install hooks separately in
  each repository. The hook checks the staged contents, including partially staged files, and leaves
  the index and working tree untouched.

Prettier wraps Markdown prose at 100 columns; rustfmt targets the same width. Both tools can leave
long literals, links, tables or code blocks, so the strict checker also checks every physical line.
Wrap those manually while preserving content, and split Rust files into modules when needed. Tabs
count as four columns. Nested Git submodules are checked by their own tooling.

GitHub Actions runs the same lint command on pushes and pull requests. Hooks are a local
convenience; CI also enforces the limits when a hook is bypassed.
