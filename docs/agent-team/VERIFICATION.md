# Independent Verification Guide

The verifier must validate all claims without assuming worker correctness.

## Checklist (must pass before merge)
- [ ] License: Apache-2.0 only; LICENSE present; no MIT; Cargo.toml license = "Apache-2.0"
- [ ] Community files: CODE_OF_CONDUCT, CONTRIBUTING, SECURITY present; issue/PR templates, CODEOWNERS, dependabot.yml
- [ ] Site: site/index.html, site/tutorial.html, site/css/style.css, site/js/app.js render; tutorial explains adding HTML/CSS/JS
- [ ] Pages workflow: .github/workflows/deploy-pages.yml valid YAML
- [ ] Workflows: >=20 unique workflows, all valid YAML (yamllint/actionlint clean)
- [ ] Clean repo: no tracked target/, no tracked __pycache__/, board/server.log removed or untracked appropriately, board-v2 is a gitlink/submodule artifact handled
- [ ] Rust: cargo fmt --all -- --check passes; cargo clippy --workspace --all-targets --locked -- -D warnings passes; cargo test --workspace --locked passes
- [ ] Release: v0.1.0 prepared; CHANGELOG.md present
