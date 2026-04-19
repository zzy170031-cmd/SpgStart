# Five-Agent Skill Guide

This repository includes the `five-agent-governance` skill at:

- `skills/five-agent-governance/`

If you want one stable entry point first, start here:

- [SPG Governance Index](./SPG_governance_index.md)

## What This Skill Does

It upgrades project-thread governance from an informal discussion habit into a reusable real five-subagent baseline.

The fixed agents are:

- `Maxwell`
- `Dirac`
- `Parfit`
- `Wegener`
- `Popper`

## Core Rules

- every governed project thread starts from a fixed startup protocol
- all five real subagents must start successfully before formal governance begins
- `Wegener` must complete two interrogation rounds and two self-reviews
- `Popper` must complete three challenge rounds and issue the final five-state verdict
- incomplete startup or incomplete verdicts must never be presented as full governance

## Directory Layout

- `SKILL.md`
- `agents/openai.yaml`
- `references/`
  - `protocol-overview.md`
  - `startup-protocol.md`
  - `maxwell-discovery.md`
  - `wegener-interrogation.md`
  - `popper-verdict.md`
  - `thread-checkpoints.md`
  - `output-contracts.md`
  - `checklists.md`
  - `samples.md`

## Install

Run from the repo root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-five-agent-governance-skill.ps1
```

Default install target:

```text
%USERPROFILE%\.codex\skills\five-agent-governance
```

## Startup Example

### English

```text
Enable five-agent-governance real five-subagent mode in this project thread. Launch Maxwell, Dirac, Parfit, Wegener, and Popper as real subagents. Current stage: new project startup.
```

### Chinese

```text
本线程启用 five-agent-governance 真实五代理模式。真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper。当前阶段是新项目启动。
```

For more templates, see:

- [Five Agent Thread Startup Template](./five-agent-thread-startup-template.md)
