use std::collections::HashSet;

use crate::index::keyword_index::KeywordIndex;
use crate::models::schemas::{ChatMessage, QueryAnalysis};

const LANGUAGES: &[&str] = &[
    "rust", "go", "python", "javascript", "typescript", "java", "kotlin", "swift",
    "ruby", "php", "c", "c++", "c#", "sql", "bash",
];
const FRAMEWORKS: &[&str] = &[
    "axum", "tokio", "tower", "react", "vue", "svelte", "django", "flask", "fastapi",
    "spring", "laravel", "rails", "next.js", "nuxt", "express",
];
const TOPIC_HINTS: &[&str] = &[
    "auth", "authentication", "security", "routing", "logging", "embedding", "vector",
    "retrieval", "prompt", "ruleset", "streaming", "testing", "performance",
];

pub fn analyze_query(messages: &[ChatMessage], keyword_index: &KeywordIndex) -> QueryAnalysis {
    let user_text = messages
        .iter()
        .filter(|message| message.role.eq_ignore_ascii_case("user"))
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let text = user_text.to_lowercase();

    let keywords = dedupe(keyword_index.extract_keywords(&text));
    let languages = detect_terms(&text, LANGUAGES, &keywords);
    let frameworks = detect_terms(&text, FRAMEWORKS, &keywords);
    let topics = detect_terms(&text, TOPIC_HINTS, &keywords);
    let intent = detect_intent(&text);
    let is_agent_mode = ["agent", "autonomous", "background job", "tool", "workflow"]
        .iter()
        .any(|term| text.contains(term));

    QueryAnalysis {
        keywords,
        languages,
        frameworks,
        topics,
        intent,
        is_agent_mode,
    }
}

fn detect_terms(text: &str, known_terms: &[&str], keywords: &[String]) -> Vec<String> {
    let mut matches = Vec::new();

    for term in known_terms {
        if text.contains(term) {
            matches.push((*term).to_string());
        }
    }

    for keyword in keywords {
        if known_terms.iter().any(|term| keyword == term) {
            matches.push(keyword.clone());
        }
    }

    dedupe(matches)
}

fn detect_intent(text: &str) -> Option<String> {
    let intent = if ["debug", "fix", "error", "broken", "panic"].iter().any(|term| text.contains(term)) {
        Some("debugging")
    } else if ["build", "create", "implement", "write", "add"].iter().any(|term| text.contains(term)) {
        Some("implementation")
    } else if ["test", "verify", "assert"].iter().any(|term| text.contains(term)) {
        Some("testing")
    } else if ["refactor", "cleanup", "simplify"].iter().any(|term| text.contains(term)) {
        Some("refactor")
    } else if ["explain", "why", "how"].iter().any(|term| text.contains(term)) {
        Some("explanation")
    } else {
        None
    };

    intent.map(str::to_string)
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for value in values {
        if seen.insert(value.clone()) {
            deduped.push(value);
        }
    }

    deduped
}
