extern crate hyper;
extern crate select;
extern crate regex;

mod wiktionary;

use wiktionary::*;
use std::error::Error;
use std::path::Path;
use std::fs::File;
use std::io::prelude::*;

fn main() {
    let mut scraper = Scraper::new();
    scraper.scrape();
    if let ScrapeStatus::Failed(error) = scraper.status() {
        println!("ERROR: loading of all words failed. {}", error);
    }
    if ! scraper.words().is_empty() {
        let path = Path::new("words.txt");
        let mut file = match File::create(&path) {
            Err(why) => panic!("couldn't create {}: {}", path.display(), why.description()),
            Ok(file) => file
        };
        let content = scraper.words().join("\n");
        if let Err(why) = file.write_all(content.as_bytes()) {
            panic!("couldn't write to {}: {}", path.display(), why.description());
        }
    }
}
