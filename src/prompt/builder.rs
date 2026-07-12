use std::collections::HashSet;

use crate::config::Config;
use crate::models::schemas::RetrievedChunk;

pub fn build_system_prompt(
    chunks: &[RetrievedChunk],
    original_system: Option<&str>,
    config: &Config,
) -> String {
    let mut lines = vec![format!(
        "Use the retrieved rules below when they are relevant. Context budget: {} tokens.",
        config.max_injected_context_tokens
    )];
    let mut seen = HashSet::new();

    let mut global_always = Vec::new();
    let mut high_priority = Vec::new();
    let mut retrieved = Vec::new();

    for chunk in chunks {
        if !seen.insert(chunk.chunk.id.clone()) {
            continue;
        }

        if chunk.chunk.frontmatter.always_include && chunk.chunk.category.eq_ignore_ascii_case("global") {
            global_always.push(chunk);
        } else if chunk.chunk.frontmatter.priority > 0 {
            high_priority.push(chunk);
        } else {
            retrieved.push(chunk);
        }
    }

    high_priority.sort_by(|left, right| right.chunk.frontmatter.priority.cmp(&left.chunk.frontmatter.priority));

    append_chunks(&mut lines, &global_always);
    append_chunks(&mut lines, &high_priority);
    append_chunks(&mut lines, &retrieved);

    if let Some(original_system) = original_system.filter(|value| !value.trim().is_empty()) {
        lines.push("## original-system".to_string());
        lines.push(original_system.trim().to_string());
    }

    lines.join("\n\n")
}

fn append_chunks(lines: &mut Vec<String>, chunks: &[&RetrievedChunk]) {
    for chunk in chunks {
        lines.push(format!(
            "## {} › {} [score: {:.2}]\n{}",
            chunk.chunk.category,
            chunk.chunk.chunk_title,
            chunk.final_score,
            chunk.chunk.content.trim()
        ));
    }
}
