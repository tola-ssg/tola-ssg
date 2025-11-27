//! RSS feed generation.
//!
//! Parses post metadata and generates RSS/Atom feeds.

use crate::{
    config::SiteConfig,
    log, run_command,
    utils::{build::collect_files, slug::slugify_path},
};
use anyhow::{Context, Ok, Result, anyhow, bail};
use rayon::prelude::*;
use regex::Regex;
use rss::{ChannelBuilder, GuidBuilder, ItemBuilder, validation::Validate};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

/// Tag name for querying typst metadata
const META_TAG_NAME: &str = "<tola-meta>";

#[derive(Debug)]
pub struct DateTimeUtc {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl DateTimeUtc {
    pub fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Self {
        Self { year, month, day, hour, minute, second }
    }

    pub fn from_ymd(year: u16, month: u8, day: u8) -> Self {
        Self::new(year, month, day, 0, 0, 0)
    }

    pub fn validate(&self) -> Result<()> {
        let Self { year, month, day, hour, minute, second } = *self;

        if !(1..=12).contains(&month) {
            bail!("month is invalid: {month}");
        }

        let is_leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let max_days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if is_leap => 29,
            2 => 28,
            _ => unreachable!(),
        };

        if day == 0 || day > max_days {
            bail!("day is invalid: {day}");
        }
        if hour > 23 {
            bail!("hour is invalid: {hour}");
        }
        if minute > 59 {
            bail!("minute is invalid: {minute}");
        }
        if second > 59 {
            bail!("second is invalid: {second}");
        }

        Ok(())
    }

    pub fn to_rfc2822(&self) -> String {
        const WEEKDAYS: [&str; 7] = ["Sat", "Sun", "Mon", "Tue", "Wed", "Thu", "Fri"];
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun",
            "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];

        // Zeller's congruence
        let (y, m) = if self.month < 3 {
            (self.year as i32 - 1, self.month as i32 + 12)
        } else {
            (self.year as i32, self.month as i32)
        };
        let d = self.day as i32;
        let weekday = ((d + (13 * (m + 1)) / 5 + y + y / 4 - y / 100 + y / 400) % 7) as usize;

        format!(
            "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
            WEEKDAYS[weekday],
            self.day,
            MONTHS[(self.month - 1) as usize],
            self.year,
            self.hour,
            self.minute,
            self.second
        )
    }
}

pub struct RSSFeed {
    title: String,
    description: String,
    base_url: String,
    language: String,
    generator: String,
    posts_meta: Vec<PostMeta>,
}

#[derive(Serialize, Deserialize, Debug)]
struct PostMeta {
    title: Option<String>,
    summary: Option<String>,
    date: Option<String>,
    update: Option<String>,

    #[serde(default)]
    link: Option<String>,
    author: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "func", rename_all = "lowercase")]
enum TypstElement {
    Space,
    Linebreak,
    Text { text: String },
    Strike { text: String },
    Link { dest: String, body: Box<TypstElement> },
    Sequence { children: Vec<TypstElement> },
    #[serde(other)]
    OtherIgnored,
}

pub fn build_rss(config: &'static SiteConfig) -> Result<()> {
    if config.build.rss.enable {
        let rss_xml = RSSFeed::new(config)?;
        rss_xml.write_to_file(config)?;
    }
    Ok(())
}

impl TypstElement {
    fn into_html_tag(self, config: &'static SiteConfig) -> String {
        match self {
            Self::Space => " ".into(),
            Self::Linebreak => "<br/>".into(),
            Self::Text { text } => text,
            Self::Strike { text } => format!("<strike>{text}</strike>"),
            Self::Link { dest, body } => {
                let href = if dest.starts_with(['.', '/']) {
                    let path = dest.trim_start_matches(['.', '/']);
                    format!("{}/{}", config.base.url.as_deref().unwrap_or_default(), path)
                } else {
                    dest
                };
                format!("<a href=\"{href}\">{}</a>", body.into_html_tag(config))
            }
            Self::Sequence { children } => {
                children.into_iter().map(|c| c.into_html_tag(config)).collect()
            }
            Self::OtherIgnored => String::new(),
        }
    }
}
pub fn get_guid_from_content_output_path(
    content_path: &Path,
    config: &'static SiteConfig,
) -> Result<String> {
    // let root = config.get_root();
    let content = &config.build.content;
    let base_url = config.base.url.clone().unwrap_or_default();

    // println!("{:?}, {:?}, {:?}, {:?}", root, content, output, content_path);
    let relative_post_path = content_path
        .strip_prefix(content)?
        .to_str()
        .ok_or(anyhow!("Invalid path"))?
        .strip_suffix(".typ")
        .ok_or(anyhow!("Not a .typ file"))
        .with_context(|| format!("building rss: {:?}", content_path))?;

    let guid_path = if content_path.file_name().is_some_and(|p| p == "index.typ") {
        PathBuf::from("index.html")
    } else {
        PathBuf::from(relative_post_path).join("index.html")
    };

    let guid_path = slugify_path(&guid_path, config);
    let guid_path = guid_path.to_str().unwrap();
    let guid_path = urlencoding::encode(guid_path).into_owned();
    let guid_path = guid_path.replace("%2F", "/");
    // println!("{}", guid_path);
    let guid_path = format!("{}/{}", base_url.trim_end_matches("/"), guid_path);

    Ok(guid_path)
}

impl RSSFeed {
    pub fn new(config: &'static SiteConfig) -> Result<Self> {
        log!(true; "rss"; "generating rss feed started");
        let posts_path = collect_files(
            &crate::utils::build::CONTENT_CACHE,
            &config.build.content,
            &|path| path.extension().is_some_and(|ext| ext == "typ"),
        )?;
        let posts_meta = posts_path
            .par_iter()
            .map(|path| query_meta(path, config))
            .collect::<Result<Vec<_>>>()?;
        let rss = Self {
            title: config.base.title.clone(),
            description: config.base.description.clone(),
            base_url: config.base.url.clone().unwrap_or_default(),
            language: config.base.language.clone(),
            generator: "tola-ssg".to_string(),
            posts_meta,
        };

        Ok(rss)
    }

    fn into_rss_xml(self) -> Result<String> {
        let items: Vec<_> = self
            .posts_meta
            .into_iter()
            .filter_map(|meta| {
                let date_rfc2822 = parse_date(meta.date)?;
                Some(
                    ItemBuilder::default()
                        .title(meta.title?)
                        .link(meta.link.clone())
                        .guid(
                            GuidBuilder::default()
                                .permalink(true)
                                .value(meta.link?)
                                .build(),
                        )
                        .description(meta.summary)
                        .pub_date(date_rfc2822)
                        .author(meta.author)
                        .build(),
                )
            })
            .collect();

        let channel = ChannelBuilder::default()
            .title(self.title)
            .link(self.base_url)
            .description(self.description)
            .language(self.language)
            .generator(self.generator)
            .items(items)
            .build();

        channel
            .validate()
            .map_err(|e| anyhow!("rss validate: {e}"))?;

        Ok(channel.to_string())
    }

    pub fn write_to_file(self, config: &'static SiteConfig) -> Result<()> {
        let xml = self.into_rss_xml()?;
        let rss_path = config.build.rss.path.as_path();
        fs::create_dir_all(rss_path.parent().unwrap())?;
        std::fs::write(rss_path, xml)?;

        log!(true; "rss"; "rss feed written successfully");
        Ok(())
    }
}

/// Parse date string to RFC2822 format
fn parse_date(date: Option<String>) -> Option<String> {
    static RE_YYYY_MM_DD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})$").unwrap());
    static RE_RFC3339: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})T(?P<H>\d{2}):(?P<M>\d{2}):(?P<S>\d{2})Z$").unwrap()
    });

    let date_str = date?;

    let datetime = if let Some(caps) = RE_RFC3339.captures(&date_str) {
        DateTimeUtc::new(
            caps["y"].parse().ok()?,
            caps["m"].parse().ok()?,
            caps["d"].parse().ok()?,
            caps["H"].parse().ok()?,
            caps["M"].parse().ok()?,
            caps["S"].parse().ok()?,
        )
    } else if let Some(caps) = RE_YYYY_MM_DD.captures(&date_str) {
        DateTimeUtc::from_ymd(
            caps["y"].parse().ok()?,
            caps["m"].parse().ok()?,
            caps["d"].parse().ok()?,
        )
    } else {
        return None;
    };

    if let Err(e) = datetime.validate() {
        log!("date"; "{e}");
        return None;
    }

    Some(datetime.to_rfc2822())
}

fn query_meta(post_path: &Path, config: &'static SiteConfig) -> Result<PostMeta> {
    let root = config.get_root();
    let guid = get_guid_from_content_output_path(post_path, config)?;

    let output = run_command!(
        &config.build.typst.command;
        "query", "--features", "html", "--format", "json",
        "--font-path", root, "--root", root,
        post_path,
        META_TAG_NAME, "--field", "value", "--one"
    )
    .with_context(|| {
        format!(
            "Failed to query metadata for rss in post path: {}\nMake sure your tag name is correct(\"{}\")",
            post_path.display(),
            META_TAG_NAME
        )
    })?;

    let queried_meta = std::str::from_utf8(&output.stdout)?;
    extract_metadata(guid, queried_meta, config)
}

// Helper function used for extracting metadata from typst post
// e.g.:
// -----------------------------------
// author: "John Doe"
// title: "My Post"
// summary: [This post is translated from #link("https://example.com")[original post]]
// date: "2023-01-01"
// -----------------------------------
// The `summary` here is a `content`, which we wanted to eval it into html string but not possible
// `typst query` command will get `{"children":[{"func":"text", "text": "This post is translated from "},{"func":"link","dest":"https://example.com","text":"original post"}]}`
fn extract_metadata(
    guid: String,
    queried_meta: &str,
    config: &'static SiteConfig,
) -> Result<PostMeta> {
    let json: serde_json::Value = serde_json::from_str(queried_meta).with_context(|| {
        format!("Failed to extract post meta. It may be a inner bug: \n {queried_meta}",)
    })?;

    let get_elem = |json: &serde_json::Value, key: &str| json.get(key).map(|v| v.as_str().unwrap_or_default().to_string());

    let summary = get_elem(&json, "summary")
        .context("Failed to get summary metadata")
        .and_then(|summary| {
            let summary = parse_element_from_typst_sequence(&summary)?.into_html_tag(config);
            Ok(summary)
        })
        .ok();
    let author = get_elem(&json, "author");
    let author = correct_rss_author(author.as_ref(), config);

    let meta = PostMeta {
        summary,
        author,
        title: get_elem(&json, "title"),
        date: get_elem(&json, "date"),
        update: get_elem(&json, "update"),
        link: Some(guid),
    };

    Ok(meta)
}

// Example for valid author(for rss): "bob@xxx.com (Bob)"
// Priority for looking up author and email:
// 1. `author` in user's post meta (`<tola-ssg-meta>`)
// 2. `author` in user's site config (`tola.toml`)
// 3. Try to combine `author` and `email` (`tola.toml`)
fn correct_rss_author(author: Option<&String>, config: &'static SiteConfig) -> Option<String> {
    static RE_VALID_AUTHOR: LazyLock<Regex> = LazyLock::new(||
        Regex::new(r"^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\s*\([^)]+\)$").unwrap()
    );

    let author = author?;
    let author = match RE_VALID_AUTHOR.is_match(author) {
        true => author.to_owned(),
        false => config.base.author.clone(),
    };
    let author = match RE_VALID_AUTHOR.is_match(&author) {
        true => author.to_owned(),
        false => format!("{} ({})", config.base.email.clone(), author),
    };
    Some(author)    
} 

fn parse_element_from_typst_sequence(content: &str) -> Result<TypstElement> {
    let parsed_element: TypstElement = serde_json::from_str(content)?;
    Ok(parsed_element)
}

#[test]
fn test_parse_element_from_typst_sequence() {
    let json_str = r#"
    {
        "func": "sequence",
        "children": [
            { "func": "space" },
            { "func": "text", "text": "小鹤双拼是一个简洁, 流畅, 自由的双拼输入法方案" },
            { "func": "space" },
            { "func": "linebreak" },
            { "func": "space" },
            { "func": "link", "dest": "https://example.com", "body": { "func": "text", "text": "小鹤双拼" } },
            { "func": "text", "text": "适合想提高打字速度, 但又不想投入巨量精力进行记忆, 追求高性价比的同学" },
            { "func": "space" },
            { "func": "unknown_func" }
        ]
    }
    "#;

    let result = parse_element_from_typst_sequence(json_str).unwrap();
    assert_eq!(
        result,
        TypstElement::Sequence {
            children: vec![
                TypstElement::Space,
                TypstElement::Text {
                    text: "小鹤双拼是一个简洁, 流畅, 自由的双拼输入法方案".to_string()
                },
                TypstElement::Space,
                TypstElement::Linebreak,
                TypstElement::Space,
                TypstElement::Link {
                    dest: "https://example.com".to_string(),
                    body: Box::new(TypstElement::Text {
                        text: "小鹤双拼".to_string()
                    }),
                },
                TypstElement::Text {
                    text: "适合想提高打字速度, 但又不想投入巨量精力进行记忆, 追求高性价比的同学"
                        .to_string()
                },
                TypstElement::Space,
                TypstElement::OtherIgnored,
            ]
        }
    );
}

#[test]
fn test_datetime_utc_new() {
    let dt = DateTimeUtc::new(2024, 6, 15, 14, 30, 45);
    assert_eq!(dt.year, 2024);
    assert_eq!(dt.month, 6);
    assert_eq!(dt.day, 15);
    assert_eq!(dt.hour, 14);
    assert_eq!(dt.minute, 30);
    assert_eq!(dt.second, 45);
}

#[test]
fn test_datetime_utc_from_ymd() {
    let dt = DateTimeUtc::from_ymd(2024, 12, 25);
    assert_eq!(dt.year, 2024);
    assert_eq!(dt.month, 12);
    assert_eq!(dt.day, 25);
    assert_eq!(dt.hour, 0);
    assert_eq!(dt.minute, 0);
    assert_eq!(dt.second, 0);
}

#[test]
fn test_datetime_utc_validate_valid() {
    // Valid date
    assert!(DateTimeUtc::new(2024, 6, 15, 14, 30, 45).validate().is_ok());

    // Edge cases - start of day
    assert!(DateTimeUtc::new(2024, 1, 1, 0, 0, 0).validate().is_ok());

    // Edge cases - end of day
    assert!(DateTimeUtc::new(2024, 12, 31, 23, 59, 59).validate().is_ok());
}

#[test]
fn test_datetime_utc_validate_invalid_month() {
    // Month 0
    assert!(DateTimeUtc::new(2024, 0, 15, 12, 0, 0).validate().is_err());

    // Month 13
    assert!(DateTimeUtc::new(2024, 13, 15, 12, 0, 0).validate().is_err());
}

#[test]
fn test_datetime_utc_validate_invalid_day() {
    // Day 0
    assert!(DateTimeUtc::new(2024, 6, 0, 12, 0, 0).validate().is_err());

    // Day 32 in a 31-day month
    assert!(DateTimeUtc::new(2024, 1, 32, 12, 0, 0).validate().is_err());

    // Day 31 in a 30-day month
    assert!(DateTimeUtc::new(2024, 4, 31, 12, 0, 0).validate().is_err());

    // Day 30 in February (leap year)
    assert!(DateTimeUtc::new(2024, 2, 30, 12, 0, 0).validate().is_err());

    // Day 29 in February (non-leap year)
    assert!(DateTimeUtc::new(2023, 2, 29, 12, 0, 0).validate().is_err());
}

#[test]
fn test_datetime_utc_validate_leap_year() {
    // Leap year - Feb 29 is valid
    assert!(DateTimeUtc::new(2024, 2, 29, 12, 0, 0).validate().is_ok());
    assert!(DateTimeUtc::new(2000, 2, 29, 12, 0, 0).validate().is_ok()); // divisible by 400

    // Non-leap year - Feb 29 is invalid
    assert!(DateTimeUtc::new(2023, 2, 29, 12, 0, 0).validate().is_err());
    assert!(DateTimeUtc::new(1900, 2, 29, 12, 0, 0).validate().is_err()); // divisible by 100 but not 400
}

#[test]
fn test_datetime_utc_validate_invalid_hour() {
    // Hour 24
    assert!(DateTimeUtc::new(2024, 6, 15, 24, 0, 0).validate().is_err());
}

#[test]
fn test_datetime_utc_validate_invalid_minute() {
    // Minute 60
    assert!(DateTimeUtc::new(2024, 6, 15, 12, 60, 0).validate().is_err());
}

#[test]
fn test_datetime_utc_validate_invalid_second() {
    // Second 60
    assert!(DateTimeUtc::new(2024, 6, 15, 12, 30, 60).validate().is_err());
}

#[test]
fn test_datetime_utc_to_rfc2822() {
    // Test a known date
    let dt = DateTimeUtc::new(2024, 1, 15, 10, 30, 45);
    let rfc2822 = dt.to_rfc2822();

    // Should contain date parts
    assert!(rfc2822.contains("15"));
    assert!(rfc2822.contains("Jan"));
    assert!(rfc2822.contains("2024"));
    assert!(rfc2822.contains("10:30:45"));
    assert!(rfc2822.contains("GMT"));
}

#[test]
fn test_datetime_utc_to_rfc2822_format() {
    let dt = DateTimeUtc::new(2024, 6, 15, 14, 30, 45);
    let rfc2822 = dt.to_rfc2822();

    // Check the general format: "Day, DD Mon YYYY HH:MM:SS GMT"
    let parts: Vec<&str> = rfc2822.split(' ').collect();
    assert_eq!(parts.len(), 6);
    assert!(parts[0].ends_with(','));
    assert_eq!(parts[5], "GMT");
}

#[test]
fn test_datetime_utc_all_months() {
    let months = [
        (1, "Jan"),
        (2, "Feb"),
        (3, "Mar"),
        (4, "Apr"),
        (5, "May"),
        (6, "Jun"),
        (7, "Jul"),
        (8, "Aug"),
        (9, "Sep"),
        (10, "Oct"),
        (11, "Nov"),
        (12, "Dec"),
    ];

    for (month_num, month_name) in months {
        let dt = DateTimeUtc::new(2024, month_num, 15, 12, 0, 0);
        assert!(dt.validate().is_ok());
        let rfc2822 = dt.to_rfc2822();
        assert!(rfc2822.contains(month_name), "Month {} should contain {}", month_num, month_name);
    }
}

#[test]
fn test_typst_element_text() {
    let json = r#"{ "func": "text", "text": "Hello World" }"#;
    let elem: TypstElement = serde_json::from_str(json).unwrap();
    assert!(matches!(elem, TypstElement::Text { text } if text == "Hello World"));
}

#[test]
fn test_typst_element_space() {
    let json = r#"{ "func": "space" }"#;
    let elem: TypstElement = serde_json::from_str(json).unwrap();
    assert!(matches!(elem, TypstElement::Space));
}

#[test]
fn test_typst_element_linebreak() {
    let json = r#"{ "func": "linebreak" }"#;
    let elem: TypstElement = serde_json::from_str(json).unwrap();
    assert!(matches!(elem, TypstElement::Linebreak));
}

#[test]
fn test_typst_element_strike() {
    let json = r#"{ "func": "strike", "text": "strikethrough" }"#;
    let elem: TypstElement = serde_json::from_str(json).unwrap();
    assert!(matches!(elem, TypstElement::Strike { text } if text == "strikethrough"));
}

#[test]
fn test_typst_element_link() {
    let json = r#"{ "func": "link", "dest": "https://example.com", "body": { "func": "text", "text": "link text" } }"#;
    let elem: TypstElement = serde_json::from_str(json).unwrap();

    if let TypstElement::Link { dest, body } = elem {
        assert_eq!(dest, "https://example.com");
        assert!(matches!(*body, TypstElement::Text { text } if text == "link text"));
    } else {
        panic!("Expected Link element");
    }
}

#[test]
fn test_typst_element_unknown_ignored() {
    let json = r#"{ "func": "custom_unknown_func" }"#;
    let elem: TypstElement = serde_json::from_str(json).unwrap();
    assert!(matches!(elem, TypstElement::OtherIgnored));
}

#[test]
fn test_typst_element_sequence() {
    let json = r#"{
        "func": "sequence",
        "children": [
            { "func": "text", "text": "Hello" },
            { "func": "space" },
            { "func": "text", "text": "World" }
        ]
    }"#;
    let elem: TypstElement = serde_json::from_str(json).unwrap();

    if let TypstElement::Sequence { children } = elem {
        assert_eq!(children.len(), 3);
        assert!(matches!(&children[0], TypstElement::Text { text } if text == "Hello"));
        assert!(matches!(&children[1], TypstElement::Space));
        assert!(matches!(&children[2], TypstElement::Text { text } if text == "World"));
    } else {
        panic!("Expected Sequence element");
    }
}
