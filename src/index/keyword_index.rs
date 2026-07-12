use std::collections::HashSet;

use crate::models::schemas::RuleChunk;

/// In-memory keyword vocabulary built from all rule chunks' applies_to and tags fields.
/// No hardcoded language/framework lists.
#[derive(Debug, Clone, Default)]
pub struct KeywordIndex {
    vocabulary: HashSet<String>,
}

impl KeywordIndex {
    pub fn new() -> Self {
        Self {
            vocabulary: HashSet::new(),
        }
    }

    /// Build vocabulary from all chunks
    pub fn build_from_chunks(&mut self, chunks: &[RuleChunk]) {
        self.vocabulary.clear();
        for chunk in chunks {
            for tag in &chunk.frontmatter.applies_to {
                self.vocabulary.insert(tag.to_lowercase());
            }
            for tag in &chunk.frontmatter.tags {
                self.vocabulary.insert(tag.to_lowercase());
            }
        }
    }

    /// Extract keywords from text that match the vocabulary (case-insensitive substring)
    pub fn extract_keywords(&self, text: &str) -> Vec<String> {
        let text_lower = text.to_lowercase();
        self.vocabulary
            .iter()
            .filter(|kw| text_lower.contains(kw.as_str()))
            .cloned()
            .collect()
    }

    /// Get all vocabulary terms
    pub fn vocabulary(&self) -> &HashSet<String> {
        &self.vocabulary
    }
}
