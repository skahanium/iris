# Advanced Syntax Document

## Callout / Admonition Blocks

> [!note] Information
> This is a note callout with **bold** content.

> [!warning] Warning
> This is a warning callout with *italic* text and `code`.

> [!tip] Pro Tip
> - List item in callout
> - Another item

> [!danger] Critical
> Do not ignore this important message.

> [!example] Example
> ```python
> print("Hello from callout")
> ```

## Footnotes

Here is a sentence with a footnote reference[^1].

Another paragraph with a different footnote[^note-label].

Here is a reference to the first footnote again[^1].

[^1]: This is the first footnote content with **bold** and `code`.

[^note-label]: This is the second footnote with a [link](https://example.com).

## Raw HTML (preserve_only)

<div class="custom-container">
  <p>This is raw HTML that should be preserved as-is.</p>
</div>

<details>
  <summary>Click to expand</summary>
  <p>Hidden content here.</p>
</details>

## Mixed Content

> [!info] Info Callout
> Content with a footnote[^mixed-fn] inside.

[^mixed-fn]: Mixed footnote content.

<div class="warning-box">
  **Bold text** inside raw HTML with a footnote[^html-fn].
</div>

[^html-fn]: Footnote referenced from HTML block.

## Directive-like Syntax

::info[Information directive content here]

::warning[Warning directive content here]

:::details Toggle
Content inside details directive.
:::

## Preserve-only Edge Cases

<!-- HTML comment should be preserve_only -->

<kbd>Ctrl</kbd> + <kbd>C</kbd>

<mark>Highlighted text</mark> that should be preserved.

<style>
  .custom { color: red; }
</style>
