#[derive(Debug, Clone)]
pub struct ChunkCandidate {
    pub title: String,
    pub content: String,
}

pub fn estimate_tokens(text: &str) -> usize {
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    let by_chars = chars.div_ceil(4);
    let by_words = ((words as f32) * 1.33).ceil() as usize;
    by_chars.max(by_words).max(1)
}

pub fn split_markdown_sections(body: &str, fallback_title: &str) -> Vec<ChunkCandidate> {
    let normalized = body.replace("\r\n", "\n");
    let mut sections: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_code_fence = false;

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
        }

        if !in_code_fence && trimmed == "---" {
            if !current.trim().is_empty() {
                sections.push(current.trim().to_string());
            }
            current.clear();
            continue;
        }

        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        sections.push(current.trim().to_string());
    }

    if sections.is_empty() {
        return vec![ChunkCandidate {
            title: fallback_title.to_string(),
            content: normalized.trim().to_string(),
        }];
    }

    sections
        .into_iter()
        .enumerate()
        .map(|(index, content)| ChunkCandidate {
            title: section_title(&content)
                .unwrap_or_else(|| format!("{fallback_title} (section {})", index + 1)),
            content,
        })
        .collect()
}

pub fn split_candidate_by_tokens(
    candidate: &ChunkCandidate,
    chunk_size: usize,
    chunk_overlap: usize,
) -> Vec<ChunkCandidate> {
    if estimate_tokens(&candidate.content) <= chunk_size.max(1) {
        return vec![candidate.clone()];
    }

    let paragraphs = split_paragraph_blocks(&candidate.content);
    let mut out = Vec::new();
    let mut buffer = String::new();

    for block in paragraphs {
        if buffer.is_empty() {
            buffer = block;
            continue;
        }

        let merged = format!("{}\n\n{}", buffer.trim_end(), block);
        if estimate_tokens(&merged) <= chunk_size.max(1) {
            buffer = merged;
            continue;
        }

        out.push(ChunkCandidate {
            title: candidate.title.clone(),
            content: buffer.trim().to_string(),
        });

        buffer = overlap_tail(&buffer, chunk_overlap);
        if !buffer.is_empty() {
            buffer = format!("{}\n\n{}", buffer.trim_end(), block);
        } else {
            buffer = block;
        }
    }

    if !buffer.trim().is_empty() {
        out.push(ChunkCandidate {
            title: candidate.title.clone(),
            content: buffer.trim().to_string(),
        });
    }

    out
}

fn section_title(content: &str) -> Option<String> {
    content
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .filter(|line| !line.is_empty())
}

fn split_paragraph_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut in_code_fence = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
        }

        if !in_code_fence && trimmed.is_empty() {
            if !current.trim().is_empty() {
                blocks.push(current.trim().to_string());
                current.clear();
            }
            continue;
        }

        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        blocks.push(current.trim().to_string());
    }

    if blocks.is_empty() {
        vec![content.trim().to_string()]
    } else {
        blocks
    }
}

fn overlap_tail(content: &str, overlap_tokens: usize) -> String {
    if overlap_tokens == 0 {
        return String::new();
    }

    let words: Vec<&str> = content.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }

    let keep = overlap_tokens.min(words.len());
    words[words.len() - keep..].join(" ")
}
