use super::{read_impl, root_impl, skills_impl, web_impl, write_impl, ToolCatalogEntry};

pub(super) fn collect_tool_catalog() -> Vec<ToolCatalogEntry> {
    let mut tools = Vec::new();
    tools.extend(read_impl::tools());
    tools.extend(web_impl::tools());
    tools.extend(write_impl::tools());
    tools.extend(root_impl::tools());
    tools.extend(skills_impl::tools());
    tools
}
