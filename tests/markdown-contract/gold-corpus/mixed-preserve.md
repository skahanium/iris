# Mixed Content Document

## Native Headings and Paragraphs

This paragraph contains native **bold**, *italic*, ~~strikethrough~~, and `code`.

## Callouts Alongside Native GFM

> [!note] Note with GFM Elements
> This note callout contains:
> - A **bullet list** inside
> - With `inline code`
> - And a [link](https://example.com)

Above callout is followed by a native paragraph.

Here's a native GFM table:

| Priority | Feature | Status |
|----------|---------|--------|
| P0 | Markdown contract | In Progress |
| P1 | Editor refactor | Planned |

## Footnotes with GFM

This paragraph has a footnote[^gfm-fn] and also **bold text**, *italics*, and `code`.

Here is a task list after footnotes:

- [x] Implement contract kernel
- [ ] Write tests
- [ ] Verify rendering consistency

More text with [links](https://example.com) and another footnote[^second-fn].

[^gfm-fn]: Footnote content with **bold**, *italic*, and `code` inside.

[^second-fn]: Second footnote with a [reference link](https://example.com/reference).

## Callout with Footnotes and GFM

> [!info] Mixed Callout
> This callout has a footnote[^callout-fn] and native GFM.
>
> | Key | Value |
> |-----|-------|
> | Type | Mixed |
> | Purpose | Testing |
>
> - Task list in callout:
> - [x] Done
> - [ ] Pending

[^callout-fn]: Footnote inside a callout context.

## Native GFM After Advanced Syntax

After all the advanced syntax, here is regular GFM content:

### Sub-section

- Plain unordered list
  1. With nested ordered list
  2. And **bold** text

> A blockquote after everything.

```typescript
// Code block after mixed content
const result = "all good";
```

---

Final paragraph with [one more link](https://example.com/final) and [[Wiki Link To Note]].

## Raw HTML (preserve_only)

<div class="note-box">
  Some raw HTML content preserved as-is.
</div>

<kbd>Ctrl</kbd> + <kbd>S</kbd> shortcut preserved.
