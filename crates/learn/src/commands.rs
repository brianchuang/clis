//! Claude Code slash-command templates scaffolded by `learn init`.
//! Users invoke these from their vault directory as `/project:review-generate`, etc.

pub const REVIEW_GENERATE: &str = r#"Generate today's review session and help me answer.

Steps:
1. Run `learn review generate --vault $ARGUMENTS` (use current directory if no argument given)
2. Read the generated review file (Reviews/YYYY-MM-DD.md)
3. For each concept in the session:
   - Read the concept note from Concepts/
   - Write a thoughtful answer in the "My answer:" section based on the concept's content
   - The answer should demonstrate understanding, not just repeat the note verbatim
4. Print a summary of how many concepts were reviewed and which prompt types were used
"#;

pub const REVIEW_GRADE: &str = r#"Grade today's review session answers.

Steps:
1. Run `learn review grade --vault $ARGUMENTS` to check for answered items (use current directory if no argument given)
2. If there are answered items awaiting grades, read the review file (Reviews/YYYY-MM-DD.md)
3. For each answered item:
   - Read the corresponding concept note from Concepts/
   - Compare the answer against the concept content
   - Assign a score (0-5):
     - 5: Perfect recall, demonstrates deep understanding
     - 4: Good recall with minor gaps
     - 3: Partial recall, key ideas present but incomplete
     - 2: Weak recall, significant gaps
     - 1: Barely relevant response
     - 0: No meaningful answer or completely wrong
   - Write concise feedback explaining the score
   - Write a hint for next time if score < 5
   - Fill in Score, Feedback, and Hint fields in the review file
4. Run `learn review grade --vault $ARGUMENTS` again to update concept frontmatter with new mastery scores
5. Print a summary of scores and any concepts that need extra attention (score < 3)
"#;

pub const CONCEPT_REFINE: &str = r#"Refine unclassified concept notes with term, domain, and tags.

Steps:
1. Run `learn concept refine --vault $ARGUMENTS` to list notes that need refinement (use current directory if no argument given)
2. For each unclassified note:
   - Read the note content
   - Propose appropriate YAML frontmatter fields:
     - `term`: A clear, canonical name for the concept
     - `domain`: The subject area (e.g., "Systems", "Databases", "Networking")
     - `tags`: 2-4 topic tags as a YAML list
   - Identify any [[wikilinks]] to other concepts that should be added to the note body
3. Show the proposed changes and ask for confirmation before writing
4. For confirmed changes, add the frontmatter fields to each note
   - Only add user-owned fields (term, domain, tags) — never modify _system fields
   - Preserve existing note body content exactly as-is
5. Print a summary of how many notes were refined
"#;

pub struct CommandTemplate {
    pub filename: &'static str,
    pub content: &'static str,
}

pub const COMMANDS: &[CommandTemplate] = &[
    CommandTemplate {
        filename: "review-generate.md",
        content: REVIEW_GENERATE,
    },
    CommandTemplate {
        filename: "review-grade.md",
        content: REVIEW_GRADE,
    },
    CommandTemplate {
        filename: "concept-refine.md",
        content: CONCEPT_REFINE,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_commands_have_non_empty_content() {
        for cmd in COMMANDS {
            assert!(!cmd.filename.is_empty());
            assert!(!cmd.content.is_empty());
            assert!(cmd.filename.ends_with(".md"));
        }
    }

    #[test]
    fn commands_reference_learn_cli() {
        assert!(REVIEW_GENERATE.contains("learn review generate"));
        assert!(REVIEW_GRADE.contains("learn review grade"));
        assert!(CONCEPT_REFINE.contains("learn concept refine"));
    }
}
