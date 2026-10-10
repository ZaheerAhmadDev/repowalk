---
name: repowalk-index-cache
description: Use when implementing or changing the .repowalk/index.json cache in repowalk core — atomic tmp+rename writes, schema-version + max-mtime invalidation, or writing tests for cache roundtrip/invalidation.
---

# repowalk — Index cache invariants

Governs `repowalk/crates/core/src/store.rs` (plan §7.6). The cache is an
optimization, never a source of truth: any doubt → re-index.

## Contract

- **Location:** `<root>/.repowalk/index.json` (`cache_path`). Directory created on demand.
- **Load:** missing file or unparseable JSON → `None`, never an error/panic.
- **Save:** atomic — serialize → write `index.json.tmp` → `fs::rename` onto
  `index.json`. Leave no `.tmp` behind; a crash mid-save keeps the old cache intact.
- **Validity (`cache_valid`):** cache exists AND `idx.version == SCHEMA_VERSION`
  AND `max(mtime of cached files) == max(mtime of current files)`.
  Any doubt → invalid → full re-index.

## Rules

- `Index.version` is `model::SCHEMA_VERSION` (u32), not the crate semver —
  bump it only for schema-breaking model changes (model.rs is FROZEN).
- `FileEntry.mtime` is unix seconds (u64) — same unit everywhere; never mix
  in millis.
- No new deps: `serde_json` + `std::fs` only (see repowalk-stack).

## Tests worth keeping (inline `mod tests`)

1. **Roundtrip** — save → load → fields equal; assert no `index.json.tmp` left.
2. **Invalidation** — valid when unchanged; `false` after a touched file
   (mtime + 1), after `version` mismatch, and when cache is missing/corrupt.
3. **Temp dirs** — no `tempfile` dep: `std::env::temp_dir()` +
   `process::id()` + test tag, `remove_dir_all` at start and end.
