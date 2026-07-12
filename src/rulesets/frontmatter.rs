use crate::error::AppError;
use crate::models::schemas::RulesetFrontmatter;

#[derive(Debug, Clone)]
pub struct ParsedRuleset {
    pub frontmatter: RulesetFrontmatter,
    pub body: String,
}

pub fn parse_ruleset_markdown(content: &str) -> Result<ParsedRuleset, AppError> {
    let normalized = content.replace("\r\n", "\n");
    let trimmed = normalized.trim_start();
    if !trimmed.starts_with("---\n") {
        return Err(AppError::Validation(
            "Ruleset markdown must start with YAML frontmatter delimited by ---".into(),
        ));
    }

    let Some(frontmatter_end) = trimmed[4..].find("\n---\n") else {
        return Err(AppError::Validation(
            "Ruleset markdown has invalid YAML frontmatter delimiters".into(),
        ));
    };

    let yaml_start = 4;
    let yaml_end = yaml_start + frontmatter_end;
    let body_start = yaml_end + "\n---\n".len();

    let yaml = &trimmed[yaml_start..yaml_end];
    let body = trimmed[body_start..].trim().to_string();
    let frontmatter = serde_yaml::from_str::<RulesetFrontmatter>(yaml)?;

    Ok(ParsedRuleset { frontmatter, body })
}
