pub struct Concept {
    pub path: String,
    pub filename: String,

    // user-owned (may be absent)
    pub term: Option<String>,
    pub domain: Option<String>,
    pub tags: Option<Vec<String>>,

    // system-owned (always present after first encounter)
    pub mastery: f64,
    pub review_count: u32,
    pub current_interval: u32,
    pub last_reviewed: Option<String>,
    pub next_review: Option<String>,
    pub last_prompt_type: Option<String>,

    // parsed from note body
    pub body: String,
    pub wikilinks: Vec<String>,
}

pub struct ReviewItem {
    pub concept_path: String,
    pub concept_term: String,
    pub prompt_type: String,
    pub prompt: String,
}

pub struct VaultConfig {
    pub default_review_count: usize,
    pub default_domain: Option<String>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            default_review_count: 5,
            default_domain: None,
        }
    }
}
