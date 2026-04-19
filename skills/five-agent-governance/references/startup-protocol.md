# Startup Protocol

This file defines how a thread enters real five-subagent mode.

## Required thread intent

Treat the following kinds of user instructions as sufficient startup authorization:

- "本线程启用 five-agent-governance 真实五代理模式"
- "按 five-agent-governance 启动，并真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper"
- any equivalent plain-language request that clearly asks for real five-subagent governance

## Boot sequence

1. Confirm this is a project thread, not casual chat.
2. Start five real subagents:
   - `Maxwell`
   - `Dirac`
   - `Parfit`
   - `Wegener`
   - `Popper`
3. Report boot results in-thread.
4. If all five started, enter governance mode.

## Boot status contract

Always report:

- `thread_type`
- `startup_protocol_detected`
- `agents_requested`
- `agents_started`
- `agents_failed`
- `boot_status`
- `can_enter_governance`

## Hard rule

If even one required agent fails to start:

- do not claim that full governance is active
- do not silently replace the missing agent with a simulated one
- stop and explain what failed
- ask the user to retry the startup if needed

## Recommended startup acknowledgment

Use a short acknowledgment like:

"本线程已进入 five-agent-governance 启动阶段。我现在真实拉起 Maxwell、Dirac、Parfit、Wegener、Popper，并在全部上线后才进入正式治理。"
