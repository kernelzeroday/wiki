mod api;
mod display;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wiki", about = "Search and read Wikipedia from the terminal")]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    /// Wikipedia language code (e.g. en, de, ja, fr)
    #[arg(short, long, default_value = "en", global = true)]
    lang: String,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Article title or search query (tries page first, falls back to search)
    query: Vec<String>,
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
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let client = api::Client::new(&cli.lang);

    let result = match cli.command {
        Some(Commands::Search { query, limit }) => cmd_search(&client, &query.join(" "), limit).await,
        Some(Commands::Summary { title }) => cmd_summary(&client, &title.join(" ")).await,
        Some(Commands::Page { title, width }) => cmd_page(&client, &title.join(" "), width).await,
        Some(Commands::Random) => cmd_random(&client).await,
        None if !cli.query.is_empty() => cmd_default(&client, &cli.query.join(" ")).await,
        None => {
            use clap::CommandFactory;
            let _ = Cli::command().print_help();
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn cmd_search(client: &api::Client, query: &str, limit: u32) -> Result<(), Box<dyn std::error::Error>> {
    let result = client.search(query, limit).await?;

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

async fn cmd_summary(client: &api::Client, title: &str) -> Result<(), Box<dyn std::error::Error>> {
    let s = client.summary(title).await?;

    let url = s.content_urls
        .as_ref()
        .and_then(|u| u.desktop.as_ref())
        .and_then(|d| d.page.as_deref());

    display::print_summary(
        &s.title,
        s.description.as_deref(),
        s.extract.as_deref(),
        url,
    );
    Ok(())
}

async fn cmd_default(client: &api::Client, query: &str) -> Result<(), Box<dyn std::error::Error>> {
    match client.summary_direct(query).await? {
        Some(s) => show_page(client, &s, None).await,
        None => cmd_search(client, query, 10).await,
    }
}

async fn cmd_page(client: &api::Client, title: &str, width: Option<usize>) -> Result<(), Box<dyn std::error::Error>> {
    let s = client.summary(title).await?;
    show_page(client, &s, width).await
}

async fn show_page(client: &api::Client, summary: &api::Summary, width: Option<usize>) -> Result<(), Box<dyn std::error::Error>> {
    let width = width.unwrap_or_else(display::term_width);
    let html = client.page_html(&summary.title).await?;
    let rendered = display::render_html(&html, width);

    use colored::Colorize;
    let mut output = format!("{}\n", summary.title.bold().blue());
    if let Some(desc) = &summary.description {
        output.push_str(&format!("{}\n", desc.dimmed()));
    }
    output.push('\n');
    output.push_str(&rendered);

    let url = summary.content_urls
        .as_ref()
        .and_then(|u| u.desktop.as_ref())
        .and_then(|d| d.page.as_deref());
    if let Some(u) = url {
        output.push_str(&format!("\n{}", u.dimmed()));
    }

    display::paged_print(&output);
    Ok(())
}

async fn cmd_random(client: &api::Client) -> Result<(), Box<dyn std::error::Error>> {
    let s = client.random_summary().await?;

    let url = s.content_urls
        .as_ref()
        .and_then(|u| u.desktop.as_ref())
        .and_then(|d| d.page.as_deref());

    display::print_summary(
        &s.title,
        s.description.as_deref(),
        s.extract.as_deref(),
        url,
    );
    Ok(())
}
