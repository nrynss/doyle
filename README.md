# Doyle

**Truth maintenance for agent memory.**

Every memory an agent keeps rests on something outside it: an API contract, a schema, a dependency version, a config. When that changes, the memory goes stale, and the agent keeps acting on it. Doyle records what each memory rests on. When a source changes, Doyle flags every memory resting on it, has a model propose a correction, sends the correction to a person for sign-off, and keeps agents from acting on the stale version until it is settled.

API contracts are the first source, through [Vimanam](https://github.com/noemaforge/vimanam): OpenAPI, protobuf and GraphQL. Doyle plugs into the memory your agents already use, through adapters.

> **Status:** entry for the Meta Global AI Developer Hackathon (Indie track), in development. Nothing below is implemented yet; see [`product.md`](product.md).

## Usage (planned)

```
doyle check OLD NEW   # flag the memories a contract change has made stale
doyle serve           # the review page: what went stale, the proposed fix, approve / retire / keep
```

## The name

Doyle is named after Jon Doyle, whose *A Truth Maintenance System* (Artificial Intelligence 12(3), 1979) set out the idea this tool applies to LLM agent memory: every belief keeps its justification, and when a justification fails, the beliefs resting on it are withdrawn. The name is a tribute; this project is not affiliated with or endorsed by him.

## Licence

Apache-2.0. See [`LICENSE`](LICENSE).
