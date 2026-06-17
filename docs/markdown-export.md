# Markdown Export Semantics

This document defines how Iris moves content between Markdown source files,
the Markdown contract layer, and the TipTap/ProseMirror editor.

## Hot Path

1. `serializeOpenNote` calls `editorDocToMarkdown`.
2. `editorDocToMarkdown` uses the ProseMirror Markdown serializer.
3. If the PM serializer cannot handle a document, Iris falls back to
   `editorBodyHtmlToMarkdown` as a compatibility path.

The user `.md` file remains the source of truth. Editor-only state must either
round-trip to a documented Markdown representation or remain transient.

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
syntax. Safe inline raw HTML such as `<kbd>Ctrl</kbd>` is represented by
`preserveInline` and written back from `originalRaw`.

## Related Tests

- `tests/editor-pm-serialize.test.ts`
- `tests/editor-list-indent-keymap.test.ts`
- `tests/markdown-spacing.test.ts`
- `tests/markdown-wiki-link-roundtrip.test.ts`
- `tests/prose-tokens.test.ts`
