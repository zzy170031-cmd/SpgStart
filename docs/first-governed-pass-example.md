# First Governed Pass Example

This document records the first canonical repository example of a real `five-agent-governance` pass that ended in implementation.

## Startup

```text
Enable five-agent-governance real five-subagent mode in this project thread. Launch Maxwell, Dirac, Parfit, Wegener, and Popper as real subagents. Current stage: feature implementation. Current checkpoint: before implementation. Current goal: prove the first minimal offline golden path from provider unlock to persisted workbench overview. Current mode: Full Council.
```

## Council Summary

- `Maxwell`
  Narrow the work to one measurable slice and avoid another governance-only cleanup cycle.
- `Dirac`
  Prove the existing system with one deterministic offline path: fixture input, fake gateway, `run_now()`, SQLite persistence, and overview assertions.
- `Parfit`
  Make the first governed pass reusable by recording a concrete example and keeping scope tight.
- `Wegener`
  Integrated verdict: choose a minimal system-contract-first slice with a small visible workbench result.
- `Popper`
  Final verdict: `PASS`, with a hard boundary against UI polish, new providers, broad refactors, or live-network work.

## Approved Slice

The approved implementation slice was:

1. Use the existing fake gateway and test fixture path.
2. Prove `ProviderStore` gate and unlock state before `run_now()`.
3. Run one offline `WorkbenchService::run_now()`.
4. Promote one ranked content item into `topic_pool`.
5. Assert persisted candidate rows, current ranking snapshots, topic-pool rows, and key overview fields.
6. Keep the scope fully offline and deterministic.

## Proof Artifact

The canonical proof for this pass is the Rust test:

- [run_now_offline_golden_path_persists_candidate_rankings_topic_pool_and_overview](../crates/spg-web/src/workbench.rs)

This test checks:

- provider gate confirmed
- provider unlock succeeded
- one successful offline run
- persisted content rows exist
- current content ranking snapshots exist
- topic-pool curation works through `add_topic_pool`
- topic-pool rows exist after the explicit curation step
- `overview()` returns the latest run and expected key fields

## Explicitly Out Of Scope

The governed pass explicitly did **not** include:

- adding a new provider
- adding a new page or UI polish
- broad refactors
- live network behavior
- multi-fixture expansion
- second product-line work

## Why This Example Matters

This is the first repository example where the real five-agent chain moved from governance into a concrete, testable implementation slice. Future project threads can reuse this document as the reference pattern for:

- how to start a governed implementation thread
- how to keep scope tight after `Popper = PASS`
- what a minimal proof artifact should look like
