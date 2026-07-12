use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tracing::warn;
use walkdir::WalkDir;

use crate::error::AppError;
use crate::models::schemas::RuleChunk;
use crate::rulesets::chunker::{split_candidate_by_tokens, split_markdown_sections};
use crate::rulesets::frontmatter::parse_ruleset_markdown;

pub fn load_category_map(map_file: &str) -> Result<HashMap<String, String>, AppError> {
    let content = std::fs::read_to_string(map_file)?;
    let parsed = serde_json::from_str::<HashMap<String, String>>(&content)?;
    if parsed.is_empty() {
        return Err(AppError::Validation(
            "rulesets category map is empty".to_string(),
        ));
    }

    let normalized = parsed
        .into_iter()
        .map(|(key, value)| {
            let trimmed = value.trim().trim_start_matches("./").to_string();
            (key, trimmed)
        })
        .collect();
    Ok(normalized)
}

pub fn load_all_rulesets(
    category_map: &HashMap<String, String>,
    rulesets_dir: &str,
    chunk_size: usize,
    chunk_overlap: usize,
) -> Result<(HashMap<String, Vec<RuleChunk>>, Vec<String>), AppError> {
    let mut chunks_by_category: HashMap<String, Vec<RuleChunk>> = HashMap::new();
    let mut empty_categories = Vec::new();

    let root = Path::new(rulesets_dir);

    for (category, relative_dir) in category_map {
        let category_dir = resolve_category_dir(root, relative_dir);
        let mut chunks = Vec::new();

        if !category_dir.exists() {
            warn!(category = %category, path = %category_dir.display(), "Ruleset category directory not found");
            empty_categories.push(category.clone());
            chunks_by_category.insert(category.clone(), chunks);
            continue;
        }

        for entry in WalkDir::new(&category_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }

            match load_file_chunks(
                category,
                &category_dir,
                path,
                chunk_size.max(1),
                chunk_overlap,
            ) {
                Ok(mut file_chunks) => chunks.append(&mut file_chunks),
                Err(error) => warn!(
                    category = %category,
                    file = %path.display(),
                    error = %error,
                    "Skipping invalid ruleset file"
                ),
            }
        }

        if chunks.is_empty() {
            empty_categories.push(category.clone());
        }

        chunks_by_category.insert(category.clone(), chunks);
    }

    Ok((chunks_by_category, empty_categories))
}

fn load_file_chunks(
    category: &str,
    category_dir: &Path,
    file_path: &Path,
    chunk_size: usize,
    chunk_overlap: usize,
) -> Result<Vec<RuleChunk>, AppError> {
    let bytes = std::fs::read(file_path)?;
    let file_hash = sha256_hex(&bytes);
    let content = String::from_utf8(bytes)
        .map_err(|error| AppError::Validation(format!("Invalid UTF-8 markdown: {error}")))?;
    let parsed = parse_ruleset_markdown(&content)?;

    let relative_path = file_path
        .strip_prefix(category_dir)
        .unwrap_or(file_path)
        .to_string_lossy()
        .replace('\\', "/");
    let document_id = format!("{category}::{relative_path}");

    let mut candidates = Vec::new();
    for section in split_markdown_sections(&parsed.body, &parsed.frontmatter.title) {
        let mut split = split_candidate_by_tokens(&section, chunk_size, chunk_overlap);
        candidates.append(&mut split);
    }

    let total_chunks = candidates.len().max(1);
    let chunks = candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| RuleChunk {
            id: format!("{document_id}::{index}"),
            document_id: document_id.clone(),
            chunk_index: index,
            chunk_title: candidate.title,
            total_chunks,
            content: candidate.content,
            source_file: relative_path.clone(),
            file_hash: file_hash.clone(),
            category: category.to_string(),
            frontmatter: parsed.frontmatter.clone(),
        })
        .collect();

    Ok(chunks)
}

fn resolve_category_dir(root: &Path, relative_dir: &str) -> PathBuf {
    let relative = relative_dir.trim_start_matches('/').trim_start_matches("./");
    root.join(relative)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
