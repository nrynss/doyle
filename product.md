# Doyle

*Truth maintenance for agent memory.*

Entry for the Meta Global AI Developer Hackathon (Cerebral Valley), Indie track, Build for Your World theme. Build 26 Oct 21:30 IST → submit by 5 Nov, 07:30 IST.

## The product

Coding agents remember what they learn about the code and the APIs they work with. Those memories rest on things that change. When an API contract changes, every memory about the old behaviour is now wrong, and nothing tells the agent. An agent's built-in memory records what it learned but not why it holds, so it cannot know when what it remembers stops being true. Neither can an external memory such as Lambo.

Doyle can. It:

1. **Records what each memory rests on.** A citation index from memories to the API elements they describe, keyed by Vimanam's stable IDs.
2. **Watches the sources.** `doyle check OLD NEW` diffs two contract versions through Vimanam. Additive changes flag nothing.
3. **Flags the stale.** Every memory citing a changed element is flagged.
4. **Proposes the fix.** Muse Spark re-checks each flagged memory against the new contract and proposes a correction, retirement, or keep.
5. **Gets a person's sign-off.** `doyle serve` is the review page: what changed, what went stale, which agents relied on it, the proposal, and approve / retire / keep.
6. **Guards the agent.** A Muse Code `PreToolUse` hook asks Doyle whether an action rests on a stale memory and blocks it with the reason.

**The moment the demo is built around:** an agent confidently ships a backup that returns 200 and silently loses records, because its memory says the endpoint returns everything. With Doyle, the same agent is stopped, the memory is corrected in both the agent's built-in memory and Lambo, and the backup is right.

## Memory systems are clients

Doyle holds its own state (the citation index). Memory systems plug in through adapters, and Doyle corrects every one that holds the stale memory.

**Pluggable by design.** Doyle's core (citations, the diff, flagging, proposals, sign-off) knows only the adapter contract, never a particular memory system. Lambo is the adapter we know best, not a special case. Adding a memory system means writing one adapter, with no change to the core. The two adapters below are the ones built for the event. Claude Code and Codex memory, and hosted memories such as mem0, Letta or Zep, are the next candidates.

**Adapter contract.** Required: list or read memories (id, text, last updated) and write one. Optional: annotate in place, so the warning shows on recall; mark superseded; notify on new memories.

| Adapter | Why |
|---|---|
| **Muse Code built-in memory**, the lead | The memory Muse Wonder writes and recalls in Muse Code; Meta-native and entirely new code. Markdown with a `MEMORY.md` index (short notes inline, or one file per topic), each note with `type` and `description` frontmatter. The default scope is per project and private, at `~/.local/share/muse/memory/projects/<path-slug>-<hash>/`; the shared scope is `<repo>/.agents/memory/`. Doyle annotates the stale note in place. Muse Code's `/migrate` imports Claude Code and Codex memory into the same format |
| **Lambo**, the second | An existing open-source dependency, reached as a client. Doyle corrects it too: it records the correction `against` the stale memory, so Lambo's recall warns. Citations are explicit (`depends_on`), contradictions are first-class (`against`). The demo shows the correction landing, not Lambo's other features |

Where a memory system cannot annotate, the agent learns of staleness through Doyle's own tool or the Muse Code hook.

**Verified on Muse Code 1.4.14 with Muse Wonder (`muse-wonder-1.0`), 28 Oct.** Wonder recalls a fact in a new session only once it has written it to its memory file: a fact it did not write came back unknown, in the same project and in another. Once written, the note recorded the fact and nothing about where it came from. After Doyle-style annotation of that note (`[STALE … now X]`), a fresh session answered with the corrected value.

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

- Not a memory store. The agent's built-in memory and Lambo are the memory.
- Not a coding agent. Muse Wonder is the agent.
- Not a dashboard for its own sake. The page exists to settle corrections and show the evidence.

## Cautions

- Muse Code loads committed `MEMORY.md` even in untrusted workspaces; Meta's docs call it a prompt-injection surface. Whatever Doyle writes there is short, plain, and quotes nothing from a contract that could read as an instruction.
- Muse Code hooks run outside its sandbox, and project hooks load only in a trusted workspace.

## Open questions

- The second real upgrade for the evaluation, ideally with visible stakes (payments, auth, personal data).
- The problem statements, on the event details page (sign-in only). The Official Rules are read: original work, no new-code rule, open source allowed if licences are followed, no licence requirement.
- The review page's stack.
