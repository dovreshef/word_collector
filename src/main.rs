use color_eyre::eyre::Context;
use color_eyre::{eyre::eyre, Report};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use reqwest::Client;
use select::document::Document;
use select::predicate::Class;
use std::fs::File;
use std::io::{BufWriter, Write};
use tokio::task;
use tracing::info;
use tracing_subscriber::EnvFilter;
use urlencoding::decode;

/// This macro is useful to avoid the "compile regex on every loop iteration" problem
macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: once_cell::sync::OnceCell<regex::Regex> = once_cell::sync::OnceCell::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

const BASE_URL: &str = "https://he.wiktionary.org";

#[tokio::main]
async fn main() -> Result<(), Report> {
    setup()?;
    info!("Fetching all words from wiktionary");
    let client = Client::new();
    let base_path = String::from("/wiki/מיוחד:כל_הדפים/א");
    let mut pages = FuturesUnordered::new();
    let jh = tokio::spawn(load_page(client.clone(), base_path));
    pages.push(jh);
    let mut parsed_pages = FuturesUnordered::new();
    let mut words = Vec::new();

    loop {
        tokio::select! {
            Some(page) = pages.next() => {
                let page = page??;
                let jh = task::spawn_blocking(move || parse_page(page));
                parsed_pages.push(jh);
            },
            Some(parsed_page) = parsed_pages.next() => {
                let parsed_page = parsed_page??;
                words.push(parsed_page.words);
                if let Some(url) = parsed_page.next_page {
                    let jh = tokio::spawn(load_page(client.clone(), url));
                    pages.push(jh);
                }
            }
            else => break,
        }
    }

    write_file("words.txt", words)?;
    Ok(())
}

fn setup() -> Result<(), Report> {
    if std::env::var("RUST_LIB_BACKTRACE").is_err() {
        std::env::set_var("RUST_LIB_BACKTRACE", "1")
    }
    color_eyre::install()?;
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info")
    }
    tracing_subscriber::fmt::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    Ok(())
}

fn write_file(file_path: &str, words: Vec<Vec<String>>) -> Result<(), Report> {
    let file = File::create(file_path).wrap_err("Failed to create words file")?;
    let mut file = BufWriter::new(file);
    let mut word_count: u32 = 0;
    for mut word in words.into_iter().flatten() {
        word_count += 1;
        word.push('\n');
        file.write(word.as_bytes())
            .wrap_err("Failed to write word")?;
    }
    file.flush().wrap_err("Failed to flush file")?;
    info!("Wrote {} words to {}", word_count, file_path);
    Ok(())
}

struct Page {
    path: String,
    content: String,
}

async fn load_page(client: Client, path: String) -> Result<Page, Report> {
    let url = format!("{}/{}", BASE_URL, path);
    info!("Loading page {}", url);
    let res = client.get(&url).send().await?.error_for_status()?;
    let content = res.text().await?;
    Ok(Page { path, content })
}

struct ParsedPage {
    words: Vec<String>,
    next_page: Option<String>,
}

fn parse_page(page: Page) -> Result<ParsedPage, Report> {
    let alphabet = regex!("^[א-ת]+$");
    let mut words = Vec::new();
    let path = page.path;
    let document = Document::from(page.content.as_str());
    // find all words
    let ul = document
        .find(Class("mw-allpages-chunk"))
        .next()
        .ok_or_else(|| eyre!("[{}] failed to find words table in page", &path))?;
    for li in ul.children() {
        if let Some(a) = li.first_child() {
            let word = a.text();
            if alphabet.is_match(&word) && word.chars().count() > 1 {
                words.push(word);
            }
        }
    }
    // find next page
    let div = document
        .find(Class("mw-allpages-nav"))
        .next()
        .ok_or_else(|| eyre!("[{}] failed to find nav div in page", &path))?;
    let a = div
        .last_child()
        .ok_or_else(|| eyre!("[{}] failed to get a element to next link", &path))?;
    let next_page = if a.text().contains("הדף הבא") {
        let href = a
            .attr("href")
            .ok_or_else(|| eyre!("[{}] failed to get href to next link", &path))
            .and_then(|href| decode(href).wrap_err("Failed to decode href"))?;
        Some(href.to_string())
    } else {
        // we have reached the last page
        None
    };
    Ok(ParsedPage { words, next_page })
}
