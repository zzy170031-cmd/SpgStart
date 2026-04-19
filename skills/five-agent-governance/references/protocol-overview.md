# Protocol Overview

`five-agent-governance` is a real project-thread governance protocol.

It exists to ensure that a project thread does not move from idea to execution without:

- clear need definition
- structured implementation direction
- UX/test/ops challenge
- integration challenge
- final gatekeeping

## Agent roles

- `Maxwell`
  - clarify the real goal, scope, priorities, and success signals
- `Dirac`
  - propose implementation shape, structure, interfaces, and sequence
- `Parfit`
  - challenge with UX, testing, operations, stability, and consistency concerns
- `Wegener`
  - interrogate Maxwell, Dirac, and Parfit twice
  - self-review twice
  - produce the integrated candidate path
- `Popper`
  - challenge Wegener three times
  - output the only valid final verdict

## Final verdicts

- `PASS`
  - move to the next allowed phase
- `PATCH`
  - current direction is acceptable, but the nearest responsibility ring must revise
- `WAIT`
  - do not continue yet; conditions or evidence are missing
- `NO`
  - the current path should not continue
- `ESCALATE`
  - a human decision is required

## Tool handoff rule

Only after governance is stable should the thread hand off to downstream tools.

- Use Notion after `Popper = PASS` when spec/plan/task capture is needed.
- Use GitHub after governance authorizes concrete implementation or review work.
- Use Figma when Parfit identifies a design or UI execution branch.

These tools are execution surfaces, not governance agents.
