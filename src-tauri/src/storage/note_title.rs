use std::path::Path;

/// Vault-relative path → display stem（不含 `.md`）。
pub(crate) fn title_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or(path)
        .to_string()
}

/// 是否为系统生成的占位标题（与路径同步/恢复共用）。
pub(crate) fn is_placeholder_title(title: &str) -> bool {
    let title = title.trim();
    if title.is_empty() {
        return true;
    }
    title.starts_with("未命名文档")
        || title.starts_with("新建文档")
        || title.starts_with("无标题")
        || title.to_lowercase().starts_with("untitled")
}

#[cfg(test)]
mod tests {
    use super::{is_placeholder_title, title_from_path};

    #[test]
    fn title_from_path_uses_file_stem() {
        assert_eq!(title_from_path("notes/foo.md"), "foo");
        assert_eq!(title_from_path("foo.md"), "foo");
        assert_eq!(title_from_path("nested/path/bar"), "bar");
    }

    #[test]
    fn title_from_path_falls_back_when_stem_empty() {
        assert_eq!(title_from_path(".md"), ".md");
        assert_eq!(title_from_path(""), "");
    }

    #[test]
    fn placeholder_matches_chinese_markers() {
        assert!(is_placeholder_title("未命名文档"));
        assert!(is_placeholder_title("未命名文档-2"));
        assert!(is_placeholder_title("新建文档"));
        assert!(is_placeholder_title("无标题"));
    }

    #[test]
    fn placeholder_matches_untitled_variants() {
        assert!(is_placeholder_title("untitled"));
        assert!(is_placeholder_title("Untitled"));
        assert!(is_placeholder_title("untitled-3"));
        assert!(is_placeholder_title("UNTITLED-N"));
    }

    #[test]
    fn placeholder_treats_empty_as_placeholder() {
        assert!(is_placeholder_title(""));
        assert!(is_placeholder_title("   "));
    }

    #[test]
    fn placeholder_rejects_real_titles() {
        assert!(!is_placeholder_title("民法总则笔记"));
        assert!(!is_placeholder_title("My Notes"));
    }
}
