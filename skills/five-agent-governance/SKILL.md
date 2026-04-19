---
name: five-agent-governance
description: Use when the user wants a project thread to start with a real five-subagent governance workflow, or when ongoing project work must be reviewed through Maxwell, Dirac, Parfit, Wegener, and Popper before implementation, merge, or release. Best for new projects, major features, architecture changes, and any thread that should explicitly run a real multi-agent council instead of a single-agent shortcut.
---

# Five Agent Governance

Use this skill to turn a project thread into a real five-subagent working council.

This is not a generic "think harder" skill. It is a strict project governance skill with two jobs:

1. Detect or request the startup protocol that authorizes real subagent use.
2. Run the project through the fixed five-agent chain:
   - `Maxwell`
   - `Dirac`
   - `Parfit`
   - `Wegener`
   - `Popper`

## Trigger

Use this skill when any of the following is true:

- The user explicitly asks for `five-agent-governance`.
- The user asks to start a project thread with real five-subagent mode.
- The user wants a project, feature, refactor, or release process to be governed by named agents.
- The user expects real subagents, not role-play or a single-agent summary.

## Non-trigger

Do not use this skill for:

- casual chat
- one-off factual Q&A
- tiny non-project requests
- threads where the user has not asked for real subagent governance

## Core rule

If the user wants this mode, prefer **real subagents**.

Do not silently simulate the five agents inside one voice when real subagents are available and the user has requested this mode.

If real subagents cannot be started, say so clearly and stop short of claiming that full governance ran.

## Startup protocol

For a new project thread, first check whether the user has provided a clear startup instruction such as:

- `Enable five-agent-governance real five-subagent mode in this project thread`
- `Start five-agent-governance and launch Maxwell, Dirac, Parfit, Wegener, and Popper as real subagents`

If the user has already said the equivalent in plain language, treat that as sufficient authorization.

Once authorized:

1. Start the five real subagents.
2. Report which agents started successfully.
3. Only continue if all five are online.

Read [references/startup-protocol.md](references/startup-protocol.md) for the exact boot sequence and failure handling.

## Operating modes

Use one of two governance modes after boot:

- `Full Council`
  - new projects
  - major features
  - architecture changes
  - merge gates
  - release gates
- `Fast Gate`
  - ongoing implementation inside an already-governed project thread
  - smaller scoped work that still needs the five-agent gate

Read [references/thread-checkpoints.md](references/thread-checkpoints.md) to choose the right checkpoint and mode.

## Fixed chain

Run the agents in this order:

1. `Maxwell`
2. `Dirac`
3. `Parfit`
4. `Wegener`
5. `Popper`

The chain is strict:

- `Maxwell` clarifies and stabilizes the need.
- `Dirac` proposes structure and implementation direction.
- `Parfit` reviews UX, testing, operations, and consistency risks.
- `Wegener` interrogates the first three agents twice, then self-reviews twice.
- `Popper` challenges Wegener three times and issues the final verdict.

Read:

- [references/maxwell-discovery.md](references/maxwell-discovery.md)
- [references/wegener-interrogation.md](references/wegener-interrogation.md)
- [references/popper-verdict.md](references/popper-verdict.md)

## Required output discipline

Every formal governance pass must produce structured records, not vague summaries.

Use the contracts in [references/output-contracts.md](references/output-contracts.md).

At minimum, keep these visible in the thread:

- current stage and checkpoint
- agent startup status
- each agent's position
- Wegener interrogation and self-review results
- Popper challenge record
- final governance verdict

## Gate checks

Always run the checklists in [references/checklists.md](references/checklists.md) before allowing a phase to pass.

Especially protect against:

- premature execution
- false completeness
- hidden assumptions presented as confirmed facts
- skipping the final Popper verdict

## Downstream tools

This skill governs decision flow first. Tools come after governance.

Use downstream integrations only after the governance result supports them:

- Notion for spec, plan, and task capture
- GitHub for implementation and review flow
- Figma for UI/design execution

See [references/protocol-overview.md](references/protocol-overview.md) for the allowed handoff logic.
