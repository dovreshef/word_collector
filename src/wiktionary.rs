use std::error::Error;
use hyper_native_tls::NativeTlsClient;
use hyper::net::HttpsConnector;
use hyper::client::Client;
use std::io::Read;
use select::document::Document;
use select::predicate::Class;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum ScrapeStatus {
    NotStarted,
    Success,
    Failed(String)
}

impl<T> From<T> for ScrapeStatus where T: Error {
    fn from(err: T) -> ScrapeStatus {
        ScrapeStatus::Failed(err.description().to_string())
    }
}

#[derive(Debug)]
pub struct Scraper {
    domain: String,
    path: String,
    client: Client,
    words: Vec<String>,
    status: ScrapeStatus
}

impl Scraper {
    pub fn new() -> Scraper {    
        let ssl = NativeTlsClient::new().unwrap();
        let connector = HttpsConnector::new(ssl);
        let client = Client::with_connector(connector);
        Scraper { 
            domain: String::from("he.wiktionary.org"),
            path: String::from("/wiki/מיוחד:כל_הדפים/א"),
            client: client,
            words: Vec::new(),
            status: ScrapeStatus::NotStarted
        }
    }

    pub fn scrape(&mut self) {
        // if we're done -- return
        if self.status != ScrapeStatus::NotStarted {
            return;
        }
        // load each words page and parse it
        loop {
            match self.load_page()
                .and_then(|b| self.parse_page(&b)) {
                Ok(next_path) => {
                    self.path = next_path;
                },
                Err(status) => {
                    self.status = status;
                    break;
                }
            }
        }
    }

    fn load_page(&mut self) -> Result<String, ScrapeStatus> {
        let url = format!("https://{}{}", self.domain, self.path);
        println!("loading {}", url);
        let mut response = self.client.get(&url).send()?;
        let mut buffer = String::new();
        response.read_to_string(&mut buffer)?;
        Ok(buffer)
    }

    fn parse_page(&mut self, buffer: &str) -> Result<String, ScrapeStatus> {
        let error = |e, path| {
            let msg = format!("{} at {}", e, path);
            ScrapeStatus::Failed(msg)
        };
        let re = Regex::new("^[א-ת]+$").unwrap();
        let document = Document::from(buffer);
        // find all words
        let ul = document.find(Class("mw-allpages-chunk")).next()
            .ok_or(error("failed to find words table in page", &self.path))?;
        for li in ul.children() {
            if let Some(a) = li.first_child() {
                let word = a.text();
                if re.is_match(&word) && word.chars().count() > 1 {
                    self.words.push(word);
                }
            }
        }
        // find next page
        let div = document.find(Class("mw-allpages-nav")).next()
            .ok_or(error("failed to find nav div in page", &self.path))?;
        let a = div.last_child()
            .ok_or(error("failed to get a element to next link", &self.path))?;
        if a.text().contains("הדף הבא") {
            let href = a.attr("href")
                .ok_or(error("failed to get href to next link", &self.path))?;
            return Ok(href.to_string());
        }
        // we have reached the last page
        Err(ScrapeStatus::Success)
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    pub fn status(&self) -> ScrapeStatus {
        self.status.clone()
    }
}
