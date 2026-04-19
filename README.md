# SPG

> Governance note: the current project-thread baseline is the real `five-agent-governance` skill under `skills/five-agent-governance/`. Legacy four-agent documents remain in this repository only as historical archive and should not be used as the default rule for new work.

SPG is a Rust-first desktop workbench for Douyin-focused SLG content discovery, topic analysis, and creation support. The current mainline goal is to form a stable loop from provider configuration and real data discovery to content decomposition, topic pooling, and creation guidance.

## Workspace

- `crates/spg-core`
  Core entity recognition, rules, SQLite persistence, and pipeline semantics.
- `crates/spg-web`
  Local web service, routes, provider setup, workbench orchestration, and main system APIs.
- `crates/spg-desktop`
  Windows desktop shell, tray, local service bootstrap, and embedded window lifecycle.

## Current Product Scope

- Platform focus: `douyin`
- Track focus: `SLG`
- Current watchlist:
  - 率土之滨
  - 三国群英传策定九州
  - 三国谋定天下
  - 无尽冬日
  - 三国冰河时代
  - 天下归心
  - 九牧之野
- Primary analysis model:
  - `doubao-seed-2-0-code-preview-260215`

## Current Governance

Start here for any new project thread, major feature, merge gate, or release gate:

- [SPG Governance Index](./docs/SPG_governance_index.md)
- [Five-Agent Skill Guide](./docs/SPG_五代理治理技能说明.md)
- [Five Agent Thread Startup Template](./docs/five-agent-thread-startup-template.md)
- [five-agent-governance skill](./skills/five-agent-governance/SKILL.md)

The current governance chain is:

- `Maxwell`: need clarification, scope, success criteria
- `Dirac`: implementation shape, system structure, technical feasibility
- `Parfit`: UX, testability, ops, consistency, maintenance risk
- `Wegener`: two interrogation rounds, two self-reviews, integrated candidate plan
- `Popper`: three challenge rounds and final verdict

## Core Docs

- [Process Overview](./docs/SPG_全流程说明.md)
- [Implementation Logic](./docs/SPG产品实现逻辑.md)
- [Workbench Sidecar Boundary](./docs/SPG_Workbench_Sidecar边界说明.md)

## Legacy Archive

These files are kept for traceability, not as current operating rules:

- [Four-Agent Collaboration Archive](./docs/SPG_四代理协作机制.md)
- [Third-Page Historical Snapshot (2026-04-14)](./docs/third-page-next-step-2026-04-14.md)

## Engineering Rules

- Rust remains the main implementation language; JS/CSS are only for UI and interaction layers.
- New work should attach to the confirmed production mainline and avoid reviving abandoned temporary experiments.
- Major changes, merge decisions, and release checks should run through the `five-agent-governance` baseline instead of the older four-agent-only convention.
- Legacy four-agent notes may be cited as evolution context, but they are not the current default governance rule.

## Local Verification

Common local commands:

```powershell
cargo test --workspace
cargo build -p spg-desktop
```
