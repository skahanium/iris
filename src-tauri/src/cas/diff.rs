/// Simple line-based LCS diff for version delta storage.
/// Format: `<hunk count>\n<old_start> <old_len> <new_start> <new_len>\n-old_line\n+new_line\n context\n`
use crate::error::{AppError, AppResult};

/// Compute a unified-style diff between two texts.
/// Returns None if the diff is not smaller than storing the full text.
pub fn compute_diff(old: &str, new: &str) -> Option<String> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    // Simple Myers-like diff using LCS
    let lcs = longest_common_subsequence(&old_lines, &new_lines);
    if lcs.is_empty() && !old.is_empty() && !new.is_empty() {
        // No common lines — just store full content
        return None;
    }

    let hunks = build_hunks(&old_lines, &new_lines, &lcs);
    let diff = format_diff(&hunks);

    // Only use diff if it saves space (at least 30% smaller)
    if diff.len() < new.len() * 7 / 10 {
        Some(diff)
    } else {
        None
    }
}

/// Apply a diff to reconstruct the new text from the old text.
pub fn apply_diff(old: &str, diff: &str) -> AppResult<String> {
    let old_lines: Vec<&str> = old.lines().collect();
    let mut result: Vec<&str> = Vec::new();
    let mut old_idx: usize = 0;
    let lines = diff.lines().peekable();

    for line in lines {
        if line.starts_with(' ') {
            // Context line — must match old
            if old_idx < old_lines.len() && &line[1..] == old_lines[old_idx] {
                result.push(old_lines[old_idx]);
                old_idx += 1;
            }
        } else if line.starts_with('-') {
            // Deleted line — advance old pointer
            old_idx += 1;
        } else if line.starts_with('+') {
            result.push(line.strip_prefix('+').unwrap_or(line));
        } else if !line.is_empty() {
            return Err(AppError::msg(format!("invalid diff line: {}", line)));
        }
    }

    // Append remaining old lines
    while old_idx < old_lines.len() {
        result.push(old_lines[old_idx]);
        old_idx += 1;
    }

    Ok(result.join("\n"))
}

#[derive(Debug, Clone)]
struct Hunk {
    _old_start: usize,
    _old_len: usize,
    _new_start: usize,
    _new_len: usize,
    edits: Vec<Edit>,
}

#[derive(Debug, Clone, PartialEq)]
enum Edit {
    Context(String),
    Delete(String),
    Insert(String),
}

fn longest_common_subsequence<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<(usize, usize)> {
    let m = a.len();
    let n = b.len();
    if m == 0 || n == 0 {
        return vec![];
    }
    // Use simple dynamic programming for small diffs
    let max_size = m.min(n).min(500); // Cap to avoid O(n²) explosion
    let mut dp = vec![vec![0u16; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }
    // Backtrack
    let mut result = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            result.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    result.reverse();
    if result.len() > max_size {
        result.truncate(max_size);
    }
    result
}

fn build_hunks(old: &[&str], new: &[&str], lcs: &[(usize, usize)]) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut li = 0usize; // lcs index
    let mut oi = 0usize; // old index
    let mut ni = 0usize; // new index

    while oi < old.len() || ni < new.len() {
        // Find next match
        let next_match = if li < lcs.len() { Some(lcs[li]) } else { None };

        let (om, nm) = next_match.unwrap_or((old.len(), new.len()));

        if oi < om || ni < nm {
            let mut edits = Vec::new();
            // Context before the change
            let ctx_start = oi.min(ni);
            let ctx_end = ctx_start + (om - oi).min(nm - ni);
            for k in ctx_start..ctx_end {
                if k < old.len() && k < new.len() && old[k] == new[k] {
                    edits.push(Edit::Context(old[k].to_string()));
                    oi = k + 1;
                    ni = k + 1;
                }
            }
            // Deletions
            while oi < om {
                edits.push(Edit::Delete(old[oi].to_string()));
                oi += 1;
            }
            // Insertions
            while ni < nm {
                edits.push(Edit::Insert(new[ni].to_string()));
                ni += 1;
            }
            if !edits.is_empty() {
                let (old_len, new_len) = count_edit_lines(&edits);
                hunks.push(Hunk {
                    _old_start: oi.saturating_sub(count_deletes(&edits)),
                    _old_len: old_len,
                    _new_start: ni.saturating_sub(count_inserts(&edits)),
                    _new_len: new_len,
                    edits,
                });
            }
        }

        // Add the matching line
        if next_match.is_some() {
            if oi < old.len() {
                if let Some(h) = hunks.last_mut() { h.edits.push(Edit::Context(old[oi].to_string())); }
            }
            oi = om + 1;
            ni = nm + 1;
            li += 1;
        } else {
            break;
        }
    }

    hunks
}

fn count_deletes(edits: &[Edit]) -> usize {
    edits.iter().filter(|e| matches!(e, Edit::Delete(_))).count()
}

fn count_inserts(edits: &[Edit]) -> usize {
    edits.iter().filter(|e| matches!(e, Edit::Insert(_))).count()
}

fn count_edit_lines(edits: &[Edit]) -> (usize, usize) {
    let dels = edits.iter().filter(|e| matches!(e, Edit::Delete(_))).count();
    let ins = edits.iter().filter(|e| matches!(e, Edit::Insert(_))).count();
    let ctx = edits.iter().filter(|e| matches!(e, Edit::Context(_))).count();
    (dels + ctx, ins + ctx)
}

fn format_diff(hunks: &[Hunk]) -> String {
    let mut out = String::new();
    for h in hunks {
        for edit in &h.edits {
            match edit {
                Edit::Context(s) => {
                    out.push(' ');
                    out.push_str(s);
                    out.push('\n');
                }
                Edit::Delete(s) => {
                    out.push('-');
                    out.push_str(s);
                    out.push('\n');
                }
                Edit::Insert(s) => {
                    out.push('+');
                    out.push_str(s);
                    out.push('\n');
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_identical_texts() {
        let text = "line 1\nline 2\nline 3";
        assert!(compute_diff(text, text).is_none());
    }

    #[test]
    fn diff_single_line_change() {
        let old = "line 1\nline 2\nline 3";
        let new = "line 1\nline 2 modified\nline 3";
        let diff = compute_diff(old, new);
        assert!(diff.is_some(), "should produce a diff");
    }

    #[test]
    fn apply_diff_roundtrip() {
        let old = "the quick\nbrown fox\njumps over\nthe lazy dog";
        let new = "the quick\nred fox\njumps over\nthe sleepy dog";
        let diff = compute_diff(old, new).expect("diff");
        let reconstructed = apply_diff(old, &diff).expect("apply");
        assert_eq!(reconstructed, new);
    }

    #[test]
    fn diff_appends_lines() {
        let old = "header\ncontent";
        let new = "header\ncontent\nnew line 1\nnew line 2";
        let diff = compute_diff(old, new).expect("diff");
        let reconstructed = apply_diff(old, &diff).expect("apply");
        assert_eq!(reconstructed, new);
        assert!(diff.len() < new.len());
    }
}
