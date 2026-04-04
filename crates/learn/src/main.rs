use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};

use learn::commands::COMMANDS;
use learn::config::{load_vault_config, resolve_vault_path, write_config_pointer};
use learn::parse::concept::parse_concept;
use learn::parse::review::{parse_answered_reviews, parse_graded_items, resolve_concept_path};
use learn::schedule::{next_interval_days, update_mastery};
use learn::select::get_due_concepts;
use learn::types::ReviewItem;
use learn::write::frontmatter::{initialize_system_fields, write_system_frontmatter};
use learn::write::review_session::write_review_session;

#[derive(Parser)]
#[command(
    name = "learn",
    version,
    about = "Agent-assisted concept learning workflows for Obsidian vaults"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold vault structure and write config pointer
    Init {
        #[arg(long, env = "LEARN_VAULT")]
        vault: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Show due count and mastery by domain
    Status {
        #[arg(long, env = "LEARN_VAULT")]
        vault: Option<String>,
    },
    /// Manage concept notes
    Concept {
        #[command(subcommand)]
        command: ConceptCommands,
    },
    /// Review session commands
    Review {
        #[command(subcommand)]
        command: ReviewCommands,
    },
}

#[derive(Subcommand)]
enum ConceptCommands {
    /// Create a blank concept note
    New {
        name: String,
        #[arg(long, env = "LEARN_VAULT")]
        vault: Option<String>,
    },
    /// Propose term, tags, and links for unclassified notes
    Refine {
        #[arg(long, env = "LEARN_VAULT")]
        vault: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Subcommand)]
enum ReviewCommands {
    /// Generate today's review session
    Generate {
        #[arg(long, env = "LEARN_VAULT")]
        vault: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        count: Option<usize>,
        #[arg(long)]
        force: bool,
    },
    /// Grade answers in today's review file
    Grade {
        #[arg(long, env = "LEARN_VAULT")]
        vault: Option<String>,
        #[arg(long)]
        file: Option<String>,
    },
}

fn get_vault(flag: Option<&str>) -> PathBuf {
    match resolve_vault_path(flag) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { vault, force } => {
            let vault_path = vault
                .map(|v| {
                    PathBuf::from(&v)
                        .canonicalize()
                        .unwrap_or_else(|_| PathBuf::from(&v))
                })
                .unwrap_or_else(|| std::env::current_dir().unwrap());

            let dirs = [
                "Concepts",
                "Reviews",
                "Templates",
                ".learning-system",
                ".learning-system/runs",
            ];
            for dir in &dirs {
                fs::create_dir_all(vault_path.join(dir)).unwrap();
            }

            let config_path = vault_path.join(".learning-system").join("config.json");
            if force || !config_path.exists() {
                let config = serde_json::json!({
                    "defaultReviewCount": 5,
                    "defaultDomain": null,
                });
                fs::write(
                    &config_path,
                    serde_json::to_string_pretty(&config).unwrap() + "\n",
                )
                .unwrap();
            } else {
                println!(
                    "Skipping .learning-system/config.json (exists, use --force to overwrite)"
                );
            }

            let template_path = vault_path.join("Templates").join("concept.md");
            if force || !template_path.exists() {
                fs::write(
                    &template_path,
                    "Write anything here. Rough notes, examples, a diagram in words,\na definition you want to understand. The system will figure out\nhow to test you on it.\n",
                )
                .unwrap();
            } else {
                println!("Skipping Templates/concept.md (exists, use --force to overwrite)");
            }

            let commands_dir = vault_path.join(".claude").join("commands");
            fs::create_dir_all(&commands_dir).unwrap();
            for cmd in COMMANDS {
                let cmd_path = commands_dir.join(cmd.filename);
                if force || !cmd_path.exists() {
                    fs::write(&cmd_path, cmd.content).unwrap();
                }
            }

            if let Err(e) = write_config_pointer(&vault_path) {
                eprintln!("Warning: could not write config pointer: {e}");
            }

            println!("Vault initialized at {}", vault_path.display());
            println!(
                "Created: Concepts/, Reviews/, Templates/, .learning-system/, .claude/commands/"
            );
        }

        Commands::Status { vault } => {
            let vault_root = get_vault(vault.as_deref());
            let _config = load_vault_config(&vault_root);
            let today = today();
            let due = get_due_concepts(&vault_root, None, None, Some(&today)).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });

            println!("\nLearning Status — {today}");
            println!("Due for review: {} concept(s)\n", due.len());

            let mut by_domain: std::collections::BTreeMap<String, (usize, f64)> =
                std::collections::BTreeMap::new();
            for c in &due {
                let domain = c.domain.as_deref().unwrap_or("(unclassified)").to_string();
                let entry = by_domain.entry(domain).or_insert((0, 0.0));
                entry.0 += 1;
                entry.1 += c.mastery;
            }

            if !by_domain.is_empty() {
                println!("Due by domain:");
                for (domain, (count, mastery_sum)) in &by_domain {
                    let avg = mastery_sum / *count as f64;
                    println!("  {domain}: {count} due, avg mastery {avg:.2}");
                }
            }
        }

        Commands::Concept { command } => match command {
            ConceptCommands::New { name, vault } => {
                let vault_root = get_vault(vault.as_deref());
                let concepts_dir = vault_root.join("Concepts");
                fs::create_dir_all(&concepts_dir).unwrap();

                let file_path = concepts_dir.join(format!("{name}.md"));
                if file_path.exists() {
                    eprintln!("Concept already exists: {}", file_path.display());
                    process::exit(1);
                }

                let template_path = vault_root.join("Templates").join("concept.md");
                let content = if template_path.exists() {
                    fs::read_to_string(&template_path).unwrap_or_default()
                } else {
                    String::new()
                };

                fs::write(&file_path, content).unwrap();
                println!("Created: {}", file_path.display());
            }

            ConceptCommands::Refine {
                vault,
                file,
                domain,
                apply,
            } => {
                let vault_root = get_vault(vault.as_deref());
                let concepts_dir = vault_root.join("Concepts");

                let files: Vec<PathBuf> = if let Some(f) = file {
                    let resolved = PathBuf::from(&f)
                        .canonicalize()
                        .unwrap_or_else(|_| PathBuf::from(&f));
                    if !resolved.exists() {
                        eprintln!("File not found: {}", resolved.display());
                        process::exit(1);
                    }
                    vec![resolved]
                } else {
                    let pattern = concepts_dir.join("**/*.md").to_string_lossy().to_string();
                    glob::glob(&pattern)
                        .unwrap()
                        .flatten()
                        .filter(|f| {
                            let c = parse_concept(f);
                            if let Some(ref d) = domain {
                                if c.domain.as_deref() != Some(d.as_str()) {
                                    return false;
                                }
                            } else {
                                let has_user_fields =
                                    c.term.is_some() || c.domain.is_some() || c.tags.is_some();
                                if has_user_fields && !c.wikilinks.is_empty() {
                                    return false;
                                }
                            }
                            true
                        })
                        .collect()
                };

                if files.is_empty() {
                    println!("No notes to refine.");
                    return;
                }

                println!("Found {} note(s) to refine.", files.len());
                println!(
                    "Run via Claude Code for AI-powered suggestions, or review notes manually:"
                );
                for f in &files {
                    if let Ok(rel) = f.strip_prefix(&vault_root) {
                        println!("  {}", rel.display());
                    } else {
                        println!("  {}", f.display());
                    }
                }
                if apply {
                    println!("\n--apply requires AI-powered suggestions. Run via Claude Code.");
                }
            }
        },

        Commands::Review { command } => match command {
            ReviewCommands::Generate {
                vault,
                domain,
                count,
                force,
            } => {
                let vault_root = get_vault(vault.as_deref());
                let config = load_vault_config(&vault_root);
                let today = today();

                let count = count.unwrap_or(config.default_review_count);
                let domain_filter = domain.as_deref().or(config.default_domain.as_deref());

                // Initialize system fields before querying due concepts
                let concepts_dir = vault_root.join("Concepts");
                let init_pattern = concepts_dir.join("**/*.md").to_string_lossy().to_string();
                for file in glob::glob(&init_pattern).unwrap().flatten() {
                    if let Err(e) = initialize_system_fields(&file, &today) {
                        eprintln!("Warning: failed to initialize {}: {e}", file.display());
                    }
                }

                let due = get_due_concepts(&vault_root, domain_filter, Some(count), Some(&today))
                    .unwrap_or_else(|e| {
                        eprintln!("Error: {e}");
                        process::exit(1);
                    });

                if due.is_empty() {
                    println!("No concepts due for review today.");
                    return;
                }

                let prompt_types = [
                    "definition",
                    "decision",
                    "contrast",
                    "context",
                    "consequence",
                ];
                let items: Vec<ReviewItem> = due
                    .iter()
                    .map(|concept| {
                        let available: Vec<&&str> = prompt_types
                            .iter()
                            .filter(|t| {
                                concept
                                    .last_prompt_type
                                    .as_deref()
                                    .map(|lpt| lpt != **t)
                                    .unwrap_or(true)
                            })
                            .collect();
                        let prompt_type = available[rand::random::<usize>() % available.len()];
                        let term = concept.term.as_deref().unwrap_or(&concept.filename);

                        ReviewItem {
                            concept_path: concept.path.clone(),
                            concept_term: term.to_string(),
                            prompt_type: prompt_type.to_string(),
                            prompt: format!("Explain your understanding of {term}."),
                        }
                    })
                    .collect();

                let file_path = write_review_session(&vault_root, &items, &today, force)
                    .unwrap_or_else(|e| {
                        eprintln!("Error: {e}");
                        process::exit(1);
                    });

                // Update _last_prompt_type for each concept
                for item in &items {
                    let _ = write_system_frontmatter(
                        Path::new(&item.concept_path),
                        &[(
                            "_last_prompt_type",
                            serde_yaml::Value::String(item.prompt_type.clone()),
                        )],
                    );
                }

                println!("{} concept(s) selected", items.len());
                println!("Review file: {file_path}");
            }

            ReviewCommands::Grade { vault, file } => {
                let vault_root = get_vault(vault.as_deref());
                let today = today();
                let review_path = file
                    .map(PathBuf::from)
                    .unwrap_or_else(|| vault_root.join("Reviews").join(format!("{today}.md")));

                if !review_path.exists() {
                    eprintln!("Review file not found: {}", review_path.display());
                    process::exit(1);
                }

                let graded = parse_graded_items(&review_path);

                if graded.is_empty() {
                    let answered = parse_answered_reviews(&review_path);
                    if answered.is_empty() {
                        println!("No answered items to grade.");
                    } else {
                        println!("Found {} answered item(s) awaiting grades.", answered.len());
                        println!(
                            "Fill in Score fields manually, or run via Claude Code for AI grading."
                        );
                    }
                    return;
                }

                let mut processed = 0u32;
                let mut failed = 0u32;

                for item in &graded {
                    let concept_path = match resolve_concept_path(&vault_root, &item.term) {
                        Some(p) => p,
                        None => {
                            eprintln!("  Could not find concept note for: {}", item.term);
                            failed += 1;
                            continue;
                        }
                    };

                    let concept_file = Path::new(&concept_path);
                    let concept = parse_concept(concept_file);
                    let interval = next_interval_days(item.score, concept.current_interval);
                    let new_mastery = update_mastery(concept.mastery, item.score);

                    let next_date = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d").unwrap()
                        + chrono::Duration::days(interval as i64);
                    let next_review = next_date.format("%Y-%m-%d").to_string();

                    match write_system_frontmatter(
                        concept_file,
                        &[
                            ("_last_reviewed", serde_yaml::Value::String(today.clone())),
                            ("_next_review", serde_yaml::Value::String(next_review)),
                            (
                                "_review_count",
                                serde_yaml::Value::Number(serde_yaml::Number::from(
                                    concept.review_count + 1,
                                )),
                            ),
                            ("_mastery", serde_yaml::to_value(new_mastery).unwrap()),
                        ],
                    ) {
                        Ok(_) => processed += 1,
                        Err(e) => {
                            eprintln!("  Failed to update {}: {e}", item.term);
                            failed += 1;
                        }
                    }
                }

                println!("Graded: {processed}, failed: {failed}");
            }
        },
    }
}
