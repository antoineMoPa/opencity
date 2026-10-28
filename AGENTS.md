# Agent instructions

- Do not add or commit Python source, packages, scripts, tests, or Python runtime steps to this
  repository.
- Keep map construction in `.road` files and application behavior in Rust. Use the Rust editor
  MCP/socket API for external automation, including clients written in Python outside this
  repository.
- Avoid normalization functions. Use explicit mapping functions or enums when values need
  conversion.
