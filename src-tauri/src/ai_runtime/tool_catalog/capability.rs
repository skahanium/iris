use crate::ai_runtime::{ToolAccessLevel, ToolCapabilityAffinity};

use super::ToolCatalogEntry;

impl ToolCatalogEntry {
    /// Capability affinity for task-policy driven tool exposure.
    pub fn capability_affinity(&self) -> Vec<ToolCapabilityAffinity> {
        capability_affinity(self)
    }
}

fn capability_affinity(entry: &ToolCatalogEntry) -> Vec<ToolCapabilityAffinity> {
    use ToolCapabilityAffinity::*;

    let mut capabilities = match entry.access_level {
        ToolAccessLevel::ReadIndex => vec![SearchNotes],
        ToolAccessLevel::ReadNoteSpan | ToolAccessLevel::ReadProfile => vec![ReadNotes],
        ToolAccessLevel::Network => vec![WebFetch],
        ToolAccessLevel::WriteMarkdown => vec![WriteNotes, PatchDocument],
        ToolAccessLevel::WriteCache | ToolAccessLevel::WriteSettings => vec![WriteNotes],
        ToolAccessLevel::ManageSkills => vec![SkillManagement],
    };

    match entry.name {
        "conclude_reasoning" | "spawn_subagent" | "get_context_packets" => {
            push_unique(&mut capabilities, ResearchSynthesis);
        }
        "get_regulation" => {
            push_unique(&mut capabilities, ResearchSynthesis);
        }
        name if name.starts_with("skills_") => {
            push_unique(&mut capabilities, SkillManagement);
        }
        name if name.starts_with("vault_") => {
            push_unique(&mut capabilities, VaultOrganize);
        }
        "add_tags" | "confirm_block_link" | "create_note_from_deposit" => {
            push_unique(&mut capabilities, VaultOrganize);
        }
        _ => {}
    }

    capabilities
}

fn push_unique(capabilities: &mut Vec<ToolCapabilityAffinity>, capability: ToolCapabilityAffinity) {
    if !capabilities.contains(&capability) {
        capabilities.push(capability);
    }
}
