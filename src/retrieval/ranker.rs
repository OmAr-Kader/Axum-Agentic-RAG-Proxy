use std::collections::HashMap;

use crate::models::schemas::{QueryAnalysis, RetrievedChunk, RuleChunk};

pub fn keyword_boost(chunk: &RuleChunk, analysis: &QueryAnalysis) -> f32 {
    let applies_to: Vec<String> = chunk
        .frontmatter
        .applies_to
        .iter()
        .map(|value| value.to_lowercase())
        .collect();
    let tags: Vec<String> = chunk
        .frontmatter
        .tags
        .iter()
        .map(|value| value.to_lowercase())
        .collect();
    let content = format!(
        "{}\n{}\n{}",
        chunk.chunk_title.to_lowercase(),
        chunk.content.to_lowercase(),
        chunk.category.to_lowercase()
    );

    let mut score: f32 = 0.0;
    for keyword in &analysis.keywords {
        if applies_to.iter().any(|value| value == keyword) {
            score += 0.35;
        } else if tags.iter().any(|value| value == keyword) {
            score += 0.25;
        } else if content.contains(keyword) {
            score += 0.1;
        }
    }

    if chunk.frontmatter.always_include {
        score += 0.4;
    }

    score.min(2.0)
}

pub fn metadata_score(chunk: &RuleChunk, analysis: &QueryAnalysis) -> f32 {
    let mut score = chunk.frontmatter.priority as f32 * 0.03;
    let applies_to: Vec<String> = chunk
        .frontmatter
        .applies_to
        .iter()
        .map(|value| value.to_lowercase())
        .collect();
    let tags: Vec<String> = chunk
        .frontmatter
        .tags
        .iter()
        .map(|value| value.to_lowercase())
        .collect();

    for value in analysis
        .languages
        .iter()
        .chain(analysis.frameworks.iter())
        .chain(analysis.topics.iter())
    {
        if applies_to.iter().any(|item| item == value) || tags.iter().any(|item| item == value) {
            score += 0.15;
        }
    }

    if let Some(intent) = &analysis.intent {
        if tags.iter().any(|item| item == intent) || chunk.chunk_title.to_lowercase().contains(intent) {
            score += 0.2;
        }
    }

    if analysis.is_agent_mode && chunk.frontmatter.agent_only {
        score += 0.25;
    }

    score
}

pub fn rank_chunks(chunks: Vec<(RuleChunk, f32)>, analysis: &QueryAnalysis) -> Vec<RetrievedChunk> {
    let mut best_by_id: HashMap<String, (RuleChunk, f32)> = HashMap::new();

    for (chunk, similarity_score) in chunks {
        tracing::debug!(
            "Evaluating chunk {} with similarity score {}",
            chunk.id, similarity_score
        );
        best_by_id
            .entry(chunk.id.clone())
            .and_modify(|existing| {
                if similarity_score > existing.1 {
                    *existing = (chunk.clone(), similarity_score);
                }
            })
            .or_insert((chunk, similarity_score));
    }

    let mut ranked: Vec<RetrievedChunk> = best_by_id
        .into_values()
        .map(|(chunk, similarity_score)| {
            let keyword_boost_value = keyword_boost(&chunk, analysis);
            let metadata_score_value = metadata_score(&chunk, analysis);
            let final_score = similarity_score + keyword_boost_value + metadata_score_value;
            RetrievedChunk {
                chunk,
                similarity_score,
                keyword_boost: keyword_boost_value,
                final_score,
            }
        })
        .collect();

    ranked.sort_by(|left, right| {
        right
            .final_score
            .partial_cmp(&left.final_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.chunk.frontmatter.priority.cmp(&left.chunk.frontmatter.priority))
    });
    tracing::info!(
        "Ranked {} chunks for query analysis: {:?}",
        ranked.len(),
        analysis
    );
    ranked
}
