# Security Policy

## Security properties

repowalk is designed to be safe to point at any checkout:

- **No network code.** There is no HTTP client, no socket use and no telemetry;
  every dependency added for v2 was checked for network capability and the
  first-party source has zero `std::net` use. Analysis is entirely local and
  deterministic.
- **It reads the repository you open** and writes only inside
  `<repo>/.repowalk/` plus the `repowalk-*` files you explicitly export from the
  export menu or the CLI.
- **No code execution.** Files are parsed with tree-sitter, never run, and no
  build system is invoked.
- Text mode can save edits to a file you opened (`Ctrl+S`) — the single write
  path into your source tree, and it is manual and explicit.

## Supported versions
repowalk is in early development. Only the latest release is supported.

## Reporting a vulnerability
If you discover a security issue, please report it by opening a GitHub security advisory or by contacting the maintainers privately. Do not disclose it publicly until we've had a chance to address it.
