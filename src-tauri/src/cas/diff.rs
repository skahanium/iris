/// Simple line-based LCS diff for version delta storage.
/// Format: `@<old_start> <old_len> <new_len>\n<inserted_line>\n...`
use crate::error::{AppError, AppResult};

/// Compute a compact line-based diff between two texts.
/// Returns None if the diff is not smaller than storing the full text.
pub fn compute_diff(old: &str, new: &str) -> Option<String> {
    if old == new {
        return None;
    }

    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let lcs = longest_common_subsequence(&old_lines, &new_lines);
    if lcs.is_empty() && !old.is_empty() && !new.is_empty() {
        return None;
    }

    let hunks = build_hunks(&old_lines, &new_lines, &lcs);
    let diff = format_diff(&hunks);
    if diff.len() < new.len() || new.is_empty() {
        Some(diff)
    } else {
        None
    }
}

/// Apply a diff to reconstruct the new text from the old text.
pub fn apply_diff(old: &str, diff: &str) -> AppResult<String> {
    let old_lines: Vec<&str> = old.lines().collect();
    let mut result: Vec<String> = Vec::new();
    let mut old_idx: usize = 0;
    let mut lines = diff.lines();

    while let Some(line) = lines.next() {
        let header = line
            .strip_prefix('@')
            .ok_or_else(|| AppError::msg(format!("invalid diff line: {}", line)))?;
        let mut parts = header.split_whitespace();
        let old_start = parse_hunk_usize(parts.next(), "old_start")?;
        let old_len = parse_hunk_usize(parts.next(), "old_len")?;
        let new_len = parse_hunk_usize(parts.next(), "new_len")?;
        if parts.next().is_some() {
            return Err(AppError::msg(format!("invalid diff header: {}", line)));
        }

        if old_start < old_idx || old_start > old_lines.len() {
            return Err(AppError::msg(format!(
                "invalid diff old_start: {}",
                old_start
            )));
        }

        while old_idx < old_start {
            result.push(old_lines[old_idx].to_string());
            old_idx += 1;
        }

        if old_idx + old_len > old_lines.len() {
            return Err(AppError::msg(format!("invalid diff old_len: {}", old_len)));
        }
        old_idx += old_len;

        for _ in 0..new_len {
            let inserted = lines
                .next()
                .ok_or_else(|| AppError::msg("diff ended before inserted lines"))?;
            result.push(inserted.to_string());
        }
    }

    while old_idx < old_lines.len() {
        result.push(old_lines[old_idx].to_string());
        old_idx += 1;
    }

    Ok(result.join("\n"))
}

#[derive(Debug, Clone)]
struct Hunk {
    old_start: usize,
    old_len: usize,
    inserted: Vec<String>,
}

fn longest_common_subsequence<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<(usize, usize)> {
    let m = a.len();
    let n = b.len();
    if m == 0 || n == 0 {
        return vec![];
    }

    let max_size = m.min(n).min(500);
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
    let mut oi = 0usize;
    let mut ni = 0usize;

    for &(om, nm) in lcs {
        if oi < om || ni < nm {
            hunks.push(Hunk {
                old_start: oi,
                old_len: om - oi,
                inserted: new[ni..nm].iter().map(|line| (*line).to_string()).collect(),
            });
        }

        oi = om + 1;
        ni = nm + 1;
    }

    if oi < old.len() || ni < new.len() {
        hunks.push(Hunk {
            old_start: oi,
            old_len: old.len() - oi,
            inserted: new[ni..].iter().map(|line| (*line).to_string()).collect(),
        });
    }

    hunks
}

fn format_diff(hunks: &[Hunk]) -> String {
    let mut out = String::new();
    for h in hunks {
        out.push_str(&format!(
            "@{} {} {}\n",
            h.old_start,
            h.old_len,
            h.inserted.len()
        ));
        for line in &h.inserted {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn parse_hunk_usize(value: Option<&str>, name: &str) -> AppResult<usize> {
    value
        .ok_or_else(|| AppError::msg(format!("missing diff header field: {}", name)))?
        .parse::<usize>()
        .map_err(|_| AppError::msg(format!("invalid diff header field: {}", name)))
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
