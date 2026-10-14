# Agent Team Orchestration — Lead Agent Model

This repository is designed to be developed/tested by a team of agents with clear roles and verification.

> **Update (2026-10-14):** this scheme is now scaled to a full 100-agent
> organization — 5 division directors, 10 team leads, 85 contributors, with
> per-team file ownership and board-task prefixes. See
> [`HIERARCHY.md`](HIERARCHY.md). Everything below still applies unchanged;
> the hierarchy is the roster, this file is the operating model.

## Structure
- **Lead Agent (Orchestrator)**: Plans work, creates/updates tasks, assigns subtasks, coordinates merges, runs final verification, and is the single point of truth for integration. The lead does not duplicate independent worker work; it reviews outputs.
- **Worker Agents (Specialists)**: Each owns specific files/areas. They execute one assigned task at a time, follow the governing skill, and return artifacts + verification command/result.
- **Independent Verifier Agent**: Tests other agents' work against stated contracts, without modifying the workers' implementations. It runs end-to-end checks, reports discrepancies, and blocks merge until all pass.
- **Reviewer/Integration Agent (optional)**: Reviews diffs for scope, conventions, and tests.

## Workflow
1. Lead reads `agent.md`, scans board/tasks, creates a master plan (todo list) and assigns tasks to workers (one writer per file).
2. Workers claim (status in_progress), perform work, verify, set done + artifacts.
3. Lead merges completed work into the integration branch in order; prevents parallel conflicting edits to same files.
4. Independent Verifier runs full verification suite (fmt, clippy, tests, YAML/site sanity) on the integrated state. "Don't work multiple at same task" — tasks are serialized by file ownership.
5. Lead finalizes: clean artifacts, update changelog, tag release.

## Principles
- One writer per file. No cross-editing without handoff.
- Verification is required before done. Independent verification is mandatory.
- Keep changes surgical. Minimize tokens/output.
