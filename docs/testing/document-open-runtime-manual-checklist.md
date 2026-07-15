# Document Open Runtime Manual Checklist

Use this checklist after automated tests pass.

## Setup

- Start Iris with `npm run tauri dev`.
- Use a vault with at least 100 Markdown notes, including one note around 50KB and one classified note if the classified vault is configured.
- Open DevTools performance tools if tracing is enabled for local development.

## Checks

- Welcome recent note: hover a recent note, click it, and confirm the editor replaces Welcome without row-only `Opening...` or `正在打开` text.
- Quick Open: open Quick Open, type a query, wait for visible results, open the first result, and confirm no blank workspace frame appears.
- File tree: expand a folder, hover a note, click it, and confirm the same loading surface behavior as Quick Open.
- Hot tab: open two notes, switch between their tabs ten times, and confirm there is no visible loading surface and no repeated `fileRead` trace for the ready tab.
- Dirty tab: edit note A, switch to note B, switch back to note A, and confirm the latest complete Markdown snapshot remains present without a visual stall; returning to A must not silently mark it saved before a disk receipt.
- Reopen existing document: open a note that already has a tab from Quick Open and confirm focus moves to the existing tab instead of cold-loading a duplicate.
- Startup warmup: restart Iris, open a recently used note from Quick Open, and confirm the runtime trace reports a warm or cache-hit path when possible.
- Background index contention: trigger the background embedding rebuild, immediately open and edit a note, and confirm visible open feedback and ordinary Markdown saves complete quickly. The rebuild must pause at a batch boundary for foreground activity rather than holding a long-lived database connection or blocking the editor.

## Pass Criteria

- Hot tab activation feels immediate and never shows the loading surface.
- Warm prepared opens feel immediate or near-immediate after selection.
- Cold opens show one stable loading surface quickly and then the editor.
- No trace or log line contains note body, frontmatter, title, full path, prompts, credentials, or decrypted classified content.
