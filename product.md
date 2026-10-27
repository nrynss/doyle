# Doyle

*Truth maintenance for agent memory.*

Entry for the Meta Global AI Developer Hackathon (Cerebral Valley), Indie track, Build for Your World theme. Build 26 Oct 21:30 IST → submit by 5 Nov, 07:30 IST.

## The product

Coding agents remember what they learn about the code and the APIs they work with. Those memories rest on things that change. When an API contract changes, every memory about the old behaviour is now wrong, and nothing tells the agent. Muse Wonder's Active Memory remembers; it cannot know when what it remembers stops being true.

Doyle can. It:

1. **Records what each memory rests on.** A citation index from memories to the API elements they describe, keyed by Vimanam's stable IDs.
2. **Watches the sources.** `doyle check OLD NEW` diffs two contract versions through Vimanam. Additive changes flag nothing.
3. **Flags the stale.** Every memory citing a changed element is flagged.
4. **Proposes the fix.** Muse Spark re-checks each flagged memory against the new contract and proposes a correction, retirement, or keep.
5. **Gets a person's sign-off.** `doyle serve` is the review page: what changed, what went stale, which agents relied on it, the proposal, and approve / retire / keep.
6. **Guards the agent.** A Muse Code `PreToolUse` hook asks Doyle whether an action rests on a stale memory and blocks it with the reason.

**The moment the demo is built around:** an agent confidently ships a backup that returns 200 and silently loses records, because its memory says the endpoint returns everything. With Doyle, the same agent is stopped, the memory is corrected, and the backup is right.

## Memory systems are clients

Doyle holds its own state (the citation index). Memory systems plug in through adapters.

**Adapter contract.** Required: list or read memories (id, text, last updated) and write one. Optional: annotate in place, so the warning shows on recall; mark superseded; notify on new memories.

| Adapter | Why |
|---|---|
| **Muse Code memory**, the lead | Meta-native and entirely new code. Markdown under `<repo>/.agents/memory/` (a `MEMORY.md` index, one file per topic) plus personal scopes. Muse Code's `/migrate` imports Claude Code and Codex memory into the same format |
| **Lambo**, the second | An existing open-source dependency, reached as a client. Citations are explicit (`depends_on`), contradictions are first-class (`against`). The demo does not showcase Lambo's own features |

Where a memory system cannot annotate, the agent learns of staleness through Doyle's own tool or the Muse Code hook.

## Meta models, one job each

| Model | Job |
|---|---|
| Muse Glimmer, or Muse Spark Open on Baseten | Curator: reads memories and transcripts, records which API elements each rests on |
| Muse Spark | Re-check: flagged memory + new contract → correct, retire, or keep |
| Muse Wonder (in Muse Code) | The coding agent in the demo: warned, then repairs its plan |

## Shape

One Rust binary, `doyle`, two modes: `check` (CLI and CI step) and `serve` (review page and the judges' page). Vimanam is a library dependency (crates.io `vimanam` 2.x). Lambo is reached as a client of a running Lambo server, never embedded.

## For judges (to build)

A live page with a "For judges: start here" table: the recorded runs with and without Doyle, the measured numbers (memories flagged, wrong actions prevented, false alarms), the Muse Code session logs, and the review page itself.

## Hackathon rules that shape the build

- **New work only.** Everything presented must be built during the event (27 Oct – 4 Nov). The demo and README must make clear what is new (Doyle) and what is an existing dependency (Vimanam, Lambo). Never vendor or copy code from earlier projects.
- **The repository must be public** by submission, on 4 Nov at 21:00 ET.
- **A three-minute demo video** of what was built during the hackathon.
- **Bonus fit:** Best Build on Meta Open Weight Models (the curator on Muse Spark Open via Baseten, doing real work) and Best Agentic Use-Case.

## Not this

- Not a memory store. Lambo and Muse Code memory are the memory.
- Not a coding agent. Muse Wonder is the agent.
- Not a dashboard for its own sake. The page exists to settle corrections and show the evidence.

## Cautions

- Muse Code loads committed `MEMORY.md` even in untrusted workspaces; Meta's docs call it a prompt-injection surface. Whatever Doyle writes there is short, plain, and quotes nothing from a contract that could read as an instruction.
- Muse Code hooks run outside its sandbox, and project hooks load only in a trusted workspace.

## Open questions

- The second real upgrade for the evaluation, ideally with visible stakes (payments, auth, personal data).
- The problem statements, on the event details page (sign-in only). The Official Rules are read: original work, no new-code rule, open source allowed if licences are followed, no licence requirement.
- The review page's stack.
