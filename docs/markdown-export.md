# Markdown Export Semantics

This document defines how Iris moves content between Markdown source files,
the Markdown contract layer, and the TipTap/ProseMirror editor.

## Hot Path

1. `serializeOpenNote` calls `editorDocToMarkdown`.
2. `editorDocToMarkdown` uses the ProseMirror Markdown serializer.
3. If the PM serializer cannot handle a document, Iris reports a recoverable
   error and keeps the last committed Markdown; it never falls back to
   HTML/Turndown for a write.

The user `.md` file remains the source of truth. Editor-only state must either
round-trip to a documented Markdown representation or remain transient.

## Contract & Production Alignment

The Markdown contract layer now uses the same production paths as the editor:

- `renderMarkdownWithProfile("editor_ingest")` delegates to
  `ingestMarkdownForEditorSafely`.
- `renderMarkdownWithProfile("editor_export")` delegates to
  `markdownToMarkdownViaProductionEditor`, which creates a real TipTap editor
  and serializes with `editorDocToMarkdown`.

This removes the previous divergence where contract tests exercised a different
Turndown pipeline than the production save path.

Current ingress is Preserve-aware `editor-ingest`. It still uses an isolated
Marked renderer to prepare TipTap HTML for the current custom schema; this is
an import implementation detail, not a write fallback. A direct ProseMirror
MarkdownParser migration is not complete until it supports the same Preserve,
callout, footnote, media, table, and wiki-link corpus.

## Block Separation

Blank lines between ordinary Markdown blocks are structural separators, not
editable spacer paragraphs.

| Stage  | Behavior                                                                                        |
| ------ | ----------------------------------------------------------------------------------------------- |
| Parse  | `space` fragments are ignored for editor content.                                               |
| Ingest | `ingestMarkdownForEditor` does not create `data-iris-spacer` paragraphs.                        |
| Schema | `IrisParagraphExtension` does not carry spacer attributes.                                      |
| Export | The PM serializer emits one Markdown block separator between blocks and skips empty paragraphs. |

## Iris Block Indent Extension

Iris treats paragraph and heading indentation as block-level editor state, not
as text content. Standard Markdown has no safe native syntax for visually
indenting an ordinary paragraph:

- Four leading spaces create an indented code block.
- `>` changes the meaning to a blockquote.
- Literal tabs or full-width spaces pollute the user's note text.

Editable indented paragraphs and headings are exported as Iris private HTML
blocks:

```html
<p data-iris-indent="1">Indented paragraph</p>
<h2 data-iris-indent="1">Indented heading</h2>
```

Only `p` and `h1`..`h6` with the `data-iris-indent` attribute are reopened as
editable Iris blocks. Other raw HTML remains preserve-only and is written back
from `originalRaw`.

## Contract Categories

| Category                 | Examples                                                   | Editing Mode         |
| ------------------------ | ---------------------------------------------------------- | -------------------- |
| Standard GFM             | paragraphs, headings, lists, task lists, tables, images    | editable             |
| Obsidian-like extensions | wiki links, callouts                                       | editable             |
| Iris private extensions  | `data-iris-indent` block HTML                              | editable             |
| Preserve-only raw syntax | unsupported raw HTML, footnote definitions, unknown blocks | write back unchanged |

## Lists

Bullet, ordered, and task lists must remain structural ProseMirror list nodes.
Tab and Shift+Tab operate on list structure through ProseMirror list commands;
they must not insert literal tabs, full-width spaces, or delete list item
content. Ordered lists are serialized to standard Markdown numbering.

## Callouts

Obsidian-style callouts such as `> [!note] Title` are parsed into editable
blockquote nodes with Iris callout attributes and are serialized back to the
callout Markdown form. Plain blockquotes remain CommonMark blockquotes.

## Preserve-Only Content

`preserveBlock` writes `originalRaw` back exactly for unsupported block-level
syntax. Inline raw HTML elements (including `span`, `kbd`, `mark`, `sub`,
`sup`, etc.) are represented by `preserveInline` and written back from
`originalRaw` as a single byte-for-byte fragment. Block-level elements such as
`div`, `section`, and `pre` remain `preserveBlock`.

## Stylesheet Loading

`markdown-prose.css` is loaded after `globals.css` in `src/main.tsx`. It is
plain CSS (no `@layer components` wrapper) so Tailwind preflight cannot reset
editor heading sizes. The build contract test in `tests/prose-tokens.test.ts`
verifies this import order.

## Related Tests

- `tests/editor-pm-serialize.test.ts`
- `tests/markdown-contract/editor-export-consistency.test.ts`
- `tests/markdown-contract/editor-roundtrip-advanced.test.ts`
- `tests/markdown-contract/serialization-boundaries.test.ts`
- `tests/markdown-list-bold.test.ts`
- `tests/prose-tokens.test.ts`
