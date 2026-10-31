# Doyle

*Truth maintenance for agent memory.*

**Doyle keeps what Muse Wonder remembers true, from API specs to legacy COBOL to the screens it watched.** A careful agent re-checks its memory before it acts, but it never fixes that memory, so the next session inherits the wrong belief and every session pays to re-check it. Doyle fixes the memory once, with a person's sign-off, and guards the one step where a stale belief would do damage. Wonder does the migration. Doyle makes its memory worth trusting, so Wonder's safety rests on certainty rather than on luck.

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

**The moment the demo is built around: the memory nobody fixed.** A veterinary clinic chain moves its patient records to a new platform, and Muse Wonder runs the migration as its ops agent. An earlier session saved a memory: `GET /patients` returns every patient in one response. The vendor's API has since moved to v2, which returns at most 100 patients and a cursor. Trusting that memory, the plan would export one page, verify it against the same call, import it, and decommission the old system, with every check passing and 4,712 of 4,812 records gone.

We ran it without Doyle, twice: once against a v2 that changed its response shape, once against a quiet v2 that keeps the same shape and pages through a `Link` header. Both times Wonder did not lose a record. It read its stale memory, probed the endpoint anyway, paged all 49 pages and verified them. What the runs show instead:

- **The belief survives the agent that caught it.** After both runs, Wonder's memory still says one response returns everyone. The next session, a cheaper model, or another agent reading the shared scope starts from the wrong belief.
- **Distrust has a cost.** With a memory it could not trust, Wonder re-derived the API from scratch: in the quiet run, about 6 minutes and 300 tool approvals to page, verify and import (part of that count is a flaky sandbox proxy). The run with Doyle gives the comparison.
- **Muse Code's own reviewer guards the last step, on the plan's evidence.** In both runs its automated safety review escalated the decommission for a person's approval until the counts were proven. If Wonder had trusted the memory, that evidence would have agreed with itself (export, repeat and import all 100). Doyle checks what the reviewer cannot: whether a belief behind the plan is still true. That a reviewer would pass a 100 = 100 decommission is our inference, not something a run has shown.
- **Its safety was a choice it happened to make.** Nothing made Wonder probe. Another run, model or plan might not.

With Doyle, the contract change flags that memory before the run. The guard holds any action on `GET /patients` until a person signs the correction and sees which agent sessions relied on it. The correction lands in the agent's built-in memory and in Lambo, and Wonder starts from a true memory and pages straight through. Both runs without Doyle are published as they happened.

## Beyond the spec: legacy code and screens

A real migration rarely has a clean API spec on both sides. The agent learns the old system from its code, from its screens, and from the training videos people made about it. Those memories go stale too, or were never right, and no spec diff catches them.

So the Quiet Migration goes further. Fernwhistle's old records system is a COBOL system: a billing batch, a copybook for the patient record, and a green-screen operator console with a training video. Wonder's job grows from moving the records to moving them **and rewriting the billing batch in Rust**, then retiring the console. Wonder does all of it. Doyle keeps what Wonder believes true, with three kinds of evidence:

| Evidence | What a memory cites | What makes it stale |
|---|---|---|
| **API spec** (Vimanam) | `GET /patients`, its 200 response | v2 pages at 100 |
| **Legacy source** | a COBOL paragraph or copybook field: "the late fee is 1.5% of the balance", resting on `CALC-LATE-FEE` | the code says otherwise: the memory was never right, or the code changed |
| **Screens** | a span of the console's training video: "decommission asks for the clinic name", 01:42–01:51 | the console redesign dropped the confirmation |

Two things make the new evidence real:

- **Doyle's eyes are Wonder's.** Doyle sends the frames and recordings to `muse-wonder-1.0` and records what Wonder sees as evidence with stable IDs, the way Vimanam gives API elements stable IDs. Doyle reads no pixels itself. Probed on the Meta Model API: Wonder listed a screen's elements, caught every change between two versions of it with none invented, and gave exact step timestamps on a short recording sent as video (one run each; T9.2 measures it properly).
- **Running both systems is evidence.** Doyle feeds the same inputs to the COBOL batch and to Wonder's Rust rewrite. Where the outputs differ, it traces the difference to the paragraphs that ran and flags the memories that cite them. The flag carries proof, not a model's opinion. In the extended story the wrong rule in the rewrite is planted in the fixtures, and the runs say so. What they show is how the mistake is caught: by execution, traced to the code, and the same way every time.

The new sources are proposed in [PHASE-9](dev-diary/PHASE-9-beyond-the-spec.md), each behind a grounding probe, so a source that does not hold up is a finding, not a broken demo.

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
| Muse Wonder (in Muse Code) | The coding agent in the demo: it runs the migration and rewrites the COBOL batch, is held at the stale step, then works from the corrected memory |
| Muse Wonder (Meta Model API) | Doyle's eyes: reads screens and recordings into evidence with stable IDs |

## Shape

One Rust binary, `doyle`, two modes: `check` (CLI and CI step) and `serve` (review page and the judges' page). Vimanam is a library dependency (crates.io `vimanam` 2.x). Lambo is reached as a client of a running Lambo server, never embedded.

## For judges (to build)

A live page with a "For judges: start here" table: the recorded runs with and without Doyle, the measured numbers (memories flagged, stale memories left behind without Doyle, the cost of re-checking, false alarms), the Muse Code session logs, and the review page itself.

## Hackathon rules that shape the build

- **New work only.** Everything presented must be built during the event (27 Oct – 4 Nov). The demo and README must make clear what is new (Doyle) and what is an existing dependency (Vimanam, Lambo). Never vendor or copy code from earlier projects.
- **The repository must be public** by submission, on 4 Nov at 21:00 ET.
- **A three-minute demo video** of what was built during the hackathon.
- **Bonus fit:** Best Build on Meta Open Weight Models (the curator on Muse Spark Open via Baseten, doing real work) and Best Agentic Use-Case.

## Not this

- Not a memory store. The agent's built-in memory and Lambo are the memory.
- Not a coding agent. Muse Wonder is the agent.
- Not a migration tool. Muse Wonder migrates, COBOL rewrite included. Doyle keeps what it believes true.
- Not a dashboard for its own sake. The page exists to settle corrections and show the evidence.

## Cautions

- Muse Code loads committed `MEMORY.md` even in untrusted workspaces; Meta's docs call it a prompt-injection surface. Whatever Doyle writes there is short, plain, and quotes nothing from a contract that could read as an instruction.
- Muse Code hooks run outside its sandbox, and project hooks load only in a trusted workspace.

## Open questions

- Whether the legacy and screen evidence (PHASE-9) goes into the recorded runs and the video, once its probes report.
- Whether to add a second, less exploratory agent reading the same shared memory: the case where the belief left behind does the damage. Optional; its result could go either way.
- The problem statements, on the event details page (sign-in only). The Official Rules are read: original work, no new-code rule, open source allowed if licences are followed, no licence requirement.
- The review page's stack.
