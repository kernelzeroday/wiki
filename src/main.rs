mod api;
mod display;

use clap::{CommandFactory, Parser, Subcommand};
use std::ffi::OsString;

#[derive(Parser)]
#[command(name = "wiki", about = "Search and read Wikipedia from the terminal", version)]
struct Cli {
    /// Wikipedia language code (e.g. en, de, ja, fr)
    #[arg(short, long, default_value = "en", global = true)]
    lang: String,

    /// Wikimedia project (wikipedia, wikiquote, wikinews, wiktionary) or base URL
    #[arg(short, long, default_value = "wikipedia", global = true)]
    site: String,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Search Wikipedia articles
    Search {
        /// Search query
        query: Vec<String>,
        /// Maximum results
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: u32,
    },
    /// Get a short summary of an article
    Summary {
        /// Article title
        title: Vec<String>,
    },
    /// Read the full article
    Page {
        /// Article title
        title: Vec<String>,
        /// Terminal width for wrapping (default: auto-detect)
        #[arg(short, long)]
        width: Option<usize>,
    },
    /// Show a random article summary
    Random,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.json {
        colored::control::set_override(false);
    }

    let client = api::Client::new(&cli.lang, &cli.site);

    let result = match cli.command {
        Some(Commands::Search { query, limit }) => {
            cmd_search(&client, &query.join(" "), limit, cli.json).await
        }
        Some(Commands::Summary { title }) => {
            cmd_summary(&client, &title.join(" "), cli.json).await
        }
        Some(Commands::Page { title, width }) => {
            cmd_page(&client, &title.join(" "), width, cli.json).await
        }
        Some(Commands::Random) => cmd_random(&client, cli.json).await,
        Some(Commands::Completions { shell }) => {
            clap_complete::generate(shell, &mut Cli::command(), "wiki", &mut std::io::stdout());
            Ok(())
        }
        Some(Commands::External(args)) => {
            let query: String = args
                .iter()
                .map(|s| s.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            cmd_default(&client, &query, cli.json).await
        }
        None => {
            let _ = Cli::command().print_help();
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        let code = if e
            .downcast_ref::<api::ApiError>()
            .map_or(false, |e| matches!(e, api::ApiError::NotFound(_)))
        {
            1
        } else {
            2
        };
        std::process::exit(code);
    }
}

fn summary_url(s: &api::Summary) -> Option<&str> {
    s.content_urls
        .as_ref()
        .and_then(|u| u.desktop.as_ref())
        .and_then(|d| d.page.as_deref())
}

async fn cmd_search(
    client: &api::Client,
    query: &str,
    limit: u32,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = client.search(query, limit).await?;

    if json {
        let items: Vec<serde_json::Value> = result
            .pages
            .iter()
            .map(|p| {
                serde_json::json!({
                    "title": p.title,
                    "description": p.description,
                    "excerpt": p.excerpt.as_ref().map(|e| display::strip_search_highlight(e)),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if result.pages.is_empty() {
        println!("No results for \"{query}\"");
        return Ok(());
    }

    println!("Results for \"{query}\":\n");
    for (i, page) in result.pages.iter().enumerate() {
        display::print_search_result(
            i,
            &page.title,
            page.description.as_deref(),
            page.excerpt.as_deref(),
        );
    }
    Ok(())
}

async fn cmd_summary(
    client: &api::Client,
    title: &str,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let s = client.summary(title).await?;

    if json {
        let out = serde_json::json!({
            "title": s.title,
            "description": s.description,
            "extract": s.extract,
            "url": summary_url(&s),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    display::print_summary(
        &s.title,
        s.description.as_deref(),
        s.extract.as_deref(),
        summary_url(&s),
    );
    Ok(())
}

async fn cmd_default(
    client: &api::Client,
    query: &str,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match client.summary_direct(query).await? {
        Some(s) => show_page(client, &s, json).await,
        None => cmd_search(client, query, 10, json).await,
    }
}

async fn cmd_page(
    client: &api::Client,
    title: &str,
    _width: Option<usize>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let s = client.summary(title).await?;
    show_page(client, &s, json).await
}

async fn show_page(
    client: &api::Client,
    summary: &api::Summary,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let width = display::term_width();
    let html = client.page_html(&summary.title).await?;
    let rendered = display::render_html(&html, width);

    if json {
        let out = serde_json::json!({
            "title": summary.title,
            "description": summary.description,
            "url": summary_url(summary),
            "content": rendered,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    use colored::Colorize;
    let mut output = format!("{}\n", summary.title.bold().blue());
    if let Some(desc) = &summary.description {
        output.push_str(&format!("{}\n", desc.dimmed()));
    }
    output.push('\n');
    output.push_str(&rendered);

    if let Some(u) = summary_url(summary) {
        output.push_str(&format!("\n{}", u.dimmed()));
    }

    display::paged_print(&output);
    Ok(())
}

async fn cmd_random(
    client: &api::Client,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let s = client.random_summary().await?;

    if json {
        let out = serde_json::json!({
            "title": s.title,
            "description": s.description,
            "extract": s.extract,
            "url": summary_url(&s),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    display::print_summary(
        &s.title,
        s.description.as_deref(),
        s.extract.as_deref(),
        summary_url(&s),
    );
    Ok(())
}
