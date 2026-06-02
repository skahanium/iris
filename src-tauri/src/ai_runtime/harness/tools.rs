//! Tool output → evidence packet merging.

use crate::ai_runtime::{AiScene, ContextPacket};

pub(crate) fn max_fetch_per_round(scene: AiScene) -> u32 {
    match scene {
        AiScene::ResearchSynthesis => 2,
        _ => 1,
    }
}

pub(crate) fn merge_tool_packets_into(
    tool_name: &str,
    output: &serde_json::Value,
    acc: &mut Vec<ContextPacket>,
) {
    if !matches!(
        tool_name,
        "search_hybrid"
            | "search_semantic"
            | "search_keyword"
            | "get_regulation"
            | "web_search"
            | "fetch_web_page"
    ) {
        return;
    }
    let Some(results) = output.get("results").and_then(|v| v.as_array()) else {
        if tool_name == "get_regulation" {
            if let Ok(packet) = serde_json::from_value::<ContextPacket>(output.clone()) {
                push_packet_dedup(acc, packet);
            } else if let Some(reg) = output.get("regulation") {
                if let Ok(packet) = serde_json::from_value::<ContextPacket>(reg.clone()) {
                    push_packet_dedup(acc, packet);
                }
            }
        }
        return;
    };
    for value in results {
        if let Ok(packet) = serde_json::from_value::<ContextPacket>(value.clone()) {
            push_packet_dedup(acc, packet);
        }
    }
}

fn push_packet_dedup(acc: &mut Vec<ContextPacket>, packet: ContextPacket) {
    if acc.iter().any(|p| p.id == packet.id) {
        return;
    }
    acc.push(packet);
}
