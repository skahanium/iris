# v1.2.6 RAG broker evaluation

The default quality gate is intentionally model-free and deterministic. It
indexes `fixtures/rag-v2-vault/` and calls the public broker rather than an
embedding helper:

```text
fixture Markdown -> index_vault_incremental(Skip) -> hybrid_retrieve_with_diagnostics
-> hard scope -> rank -> ContextPacket
```

Run it with:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test rag_broker_eval -- --nocapture
```

The fixture has exactly 48 synthetic Markdown notes and 60 labelled queries:
50 answerable queries plus 10 no-answer queries. Ten answerable link queries
require two labelled source notes; the remaining 40 require one source. Four
labels exercise hard scope boundaries: path prefix, exact path, one required
tag, and two required tags with AND semantics. The fixture contract test
asserts all of these counts directly from `labels.json`.

## Default gates

- any-source Recall@5 >= 0.80; any-source Recall@30 >= 0.95
- nDCG@10 >= 0.85; MRR@10 is reported for trend comparison
- no-answer false-positive rate <= 0.10
- scope leaks == 0
- at least six queries must be served by metadata FTS
- warm p95 is reported; it is not compared across different machines

The two recall families have deliberately different semantics:

- **any-source recall** passes a query when at least one labelled path appears
  by the cutoff. This preserves the historical release gate and is the basis
  of MRR/nDCG, which rank the first labelled path.
- **all-required-source recall** passes only when every labelled path appears
  by the cutoff. It is especially important for the ten two-source link
  queries and is reported separately; it must never be presented as the
  historical `Recall@K` value.

The deterministic v1.2.15 run measured any-source Recall@5/30 =
0.960/0.960 and all-required-source Recall@5/30 = 0.900/0.900, with 10
metadata matches, no-answer false-positive rate 0, and zero scope leaks. The
test computes each metadata-match query once and has a focused contract test
showing that one of two required paths is insufficient for all-required-source
recall.

The test disables vector retrieval deliberately. It therefore verifies the
actual hybrid broker, FTS, metadata, scope, rank and ContextPacket route
without downloading a model. A separately provisioned release environment
must run the same corpus with BGE v2 available before using vector-quality
claims.

## Citation-integrity release gate

`rag_v2_every_returned_packet_has_a_valid_source_span_and_hash` is strict:
every returned packet must have a non-empty source path and content hash, a
non-empty excerpt, and `SourceSpan { start, end }` with `end > start`. It is
not acceptable to weaken this gate merely because a retrieval layer currently
emits descriptive text.

Each packet producer has a concrete remediation route:

| Producer                   | Required evidence source                                                                                                                                 |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| FTS                        | Resolve the matching `files` row to the selected `chunks` row; use its `content_hash`, `source_start`, `source_end`, heading and source excerpt.         |
| Metadata aliases/tags      | Resolve the matching file to a concrete chunk before constructing the packet; aliases and tags are a retrieval reason, never an evidence excerpt.        |
| Graph                      | Expand a linked file, then retrieve a real target chunk from that file; do not use a `linked via` sentence as evidence.                                  |
| Vector chunks              | Preserve the chunk's stored `content_hash` and source offsets when turning the vector hit into a packet.                                                 |
| Vector anchors/regulations | Store/recover the canonical source text and stable offsets (or resolve the cited backing note); do not synthesize a citation from vector metadata alone. |
| Exact regulation           | Attach the canonical regulation source hash and article span that produced the exact match.                                                              |
| Templates                  | Hash the immutable template text and return its actual source span.                                                                                      |
| Runtime overlay            | Hash the current in-memory document content and calculate UTF-8-safe offsets into that snapshot; mark it transient but still cite it.                    |

## Baseline policy

A historical comparison must be captured from the immutable `v1.2.5` tag
against this exact fixture and recorded in
`docs/eval/results/v1.2.5-hybrid.json` with the command, revision, platform,
fixture hash and raw metrics. Do not manufacture baseline values from the
current broker. The present deterministic absolute gates prevent regressions
until that reproducible historical capture is checked in; the release gate
then additionally requires nDCG@10 and MRR@10 to improve by at least 0.05,
and no labelled subset to regress by more than 0.02.
