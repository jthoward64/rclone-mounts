// SPDX-License-Identifier: GPL-2.0-or-later

//! rclone.conf format I/O.
//!
//! rclone.conf is INI-shaped: `[section]` headers and `key = value` lines, with
//! `#` or `;` line comments and blank lines for grouping. There is no formal
//! spec; rclone itself uses Go's `ini` package permissively. We aim for
//! **round-trip preservation**: parse → render returns the original bytes when
//! no mutation occurs, and mutations leave unrelated comments and ordering
//! intact. The user's hand-edits must not be clobbered.
//!
//! Parsing is line-oriented via nom. Each line becomes a typed `Line` carrying
//! its original raw text; render writes the raw text back verbatim. When a
//! caller mutates via `set` / `remove_*`, the affected lines are replaced with
//! canonical forms and the rest is untouched.

use nom::{
    branch::alt,
    bytes::complete::{take_till, take_while1},
    character::complete::{char, line_ending, not_line_ending, space0},
    combinator::{eof, opt, recognize},
    sequence::{delimited, tuple},
    IResult,
};

use crate::Error;

/// A parsed rclone.conf document. Round-trip preserving.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    Section { name: String, raw: String },
    KeyValue { key: String, value: String, raw: String },
    Comment(String),
    Blank,
}

impl Document {
    pub fn parse(input: &str) -> Result<Self, Error> {
        let mut lines = Vec::new();
        let mut remaining = input;
        while !remaining.is_empty() {
            let (rest, line) = parse_line(remaining)
                .map_err(|e| Error::ConfigParse(format!("near {:?}: {e}", &remaining[..40.min(remaining.len())])))?;
            lines.push(line);
            remaining = rest;
        }
        Ok(Self { lines })
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Section { raw, .. } | Line::KeyValue { raw, .. } | Line::Comment(raw) => {
                    out.push_str(raw);
                }
                Line::Blank => out.push('\n'),
            }
        }
        out
    }

    pub fn sections(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                Line::Section { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Every `key = value` pair in the named section, in file order. Empty if
    /// the section doesn't exist. Lets callers read all of a section's options
    /// without knowing the keys in advance.
    pub fn section_entries(&self, section: &str) -> Vec<(&str, &str)> {
        let Some((start, end)) = self.section_range(section) else {
            return Vec::new();
        };
        self.lines[start..end]
            .iter()
            .filter_map(|l| match l {
                Line::KeyValue { key, value, .. } => Some((key.as_str(), value.as_str())),
                _ => None,
            })
            .collect()
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let (start, end) = self.section_range(section)?;
        self.lines[start..end].iter().find_map(|l| match l {
            Line::KeyValue { key: k, value, .. } if k == key => Some(value.as_str()),
            _ => None,
        })
    }

    /// Insert or update a key in the named section. Creates the section at the
    /// end of the document if it doesn't exist.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let canonical = format!("{key} = {value}\n");
        match self.section_range(section) {
            Some((start, end)) => {
                for i in start..end {
                    if let Line::KeyValue { key: k, .. } = &self.lines[i] {
                        if k == key {
                            self.lines[i] = Line::KeyValue {
                                key: key.to_string(),
                                value: value.to_string(),
                                raw: canonical,
                            };
                            return;
                        }
                    }
                }
                // Key not found in section — insert at end of section, before trailing blanks.
                let insert_at = (start..end)
                    .rev()
                    .find(|&i| !matches!(self.lines[i], Line::Blank))
                    .map(|i| i + 1)
                    .unwrap_or(end);
                self.lines.insert(
                    insert_at,
                    Line::KeyValue {
                        key: key.to_string(),
                        value: value.to_string(),
                        raw: canonical,
                    },
                );
            }
            None => {
                if !matches!(self.lines.last(), Some(Line::Blank) | None) {
                    self.lines.push(Line::Blank);
                }
                self.lines.push(Line::Section {
                    name: section.to_string(),
                    raw: format!("[{section}]\n"),
                });
                self.lines.push(Line::KeyValue {
                    key: key.to_string(),
                    value: value.to_string(),
                    raw: canonical,
                });
            }
        }
    }

    /// Remove a section and all its keys (but leave preceding comments alone).
    pub fn remove_section(&mut self, section: &str) {
        if let Some((header_idx, end)) = self.section_range_with_header(section) {
            self.lines.drain(header_idx..end);
            // Collapse multiple trailing blanks down to one.
            while matches!(self.lines.last(), Some(Line::Blank))
                && self.lines.len() >= 2
                && matches!(self.lines[self.lines.len() - 2], Line::Blank)
            {
                self.lines.pop();
            }
        }
    }

    /// Returns the half-open range of line indices *inside* the named section
    /// (i.e. excluding the `[section]` header itself).
    fn section_range(&self, section: &str) -> Option<(usize, usize)> {
        let header_idx = self.lines.iter().position(|l| matches!(l, Line::Section { name, .. } if name == section))?;
        let start = header_idx + 1;
        let end = self.lines[start..]
            .iter()
            .position(|l| matches!(l, Line::Section { .. }))
            .map(|p| start + p)
            .unwrap_or(self.lines.len());
        Some((start, end))
    }

    fn section_range_with_header(&self, section: &str) -> Option<(usize, usize)> {
        let (start, end) = self.section_range(section)?;
        Some((start - 1, end))
    }
}

fn parse_line(input: &str) -> IResult<&str, Line> {
    alt((parse_blank, parse_comment, parse_section, parse_key_value))(input)
}

fn parse_blank(input: &str) -> IResult<&str, Line> {
    let (rest, _) = recognize(tuple((space0, alt((line_ending, eof)))))(input)?;
    // Only treat as blank if the whole line was empty/whitespace; otherwise let other parsers run.
    let consumed_len = input.len() - rest.len();
    let consumed = &input[..consumed_len];
    if consumed.trim().is_empty() {
        Ok((rest, Line::Blank))
    } else {
        // Should not happen given recognize semantics, but be safe.
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Verify,
        )))
    }
}

fn parse_comment(input: &str) -> IResult<&str, Line> {
    let (rest, raw) = recognize(tuple((
        space0,
        alt((char(';'), char('#'))),
        not_line_ending,
        opt(line_ending),
    )))(input)?;
    Ok((rest, Line::Comment(raw.to_string())))
}

fn parse_section(input: &str) -> IResult<&str, Line> {
    let (rest, raw) = recognize(tuple((
        space0,
        delimited(char('['), take_till(|c| c == ']' || c == '\n'), char(']')),
        not_line_ending,
        opt(line_ending),
    )))(input)?;
    let name_start = raw.find('[').unwrap() + 1;
    let name_end = raw.find(']').unwrap();
    let name = raw[name_start..name_end].trim().to_string();
    Ok((rest, Line::Section { name, raw: raw.to_string() }))
}

fn parse_key_value(input: &str) -> IResult<&str, Line> {
    let (rest, raw) = recognize(tuple((
        space0,
        take_while1(is_key_char),
        space0,
        char('='),
        not_line_ending,
        opt(line_ending),
    )))(input)?;
    let eq_idx = raw.find('=').unwrap();
    let key = raw[..eq_idx].trim().to_string();
    // Value runs from after = up to (but not including) any trailing newline.
    let after_eq = &raw[eq_idx + 1..];
    let trimmed_eol = after_eq.trim_end_matches(['\n', '\r']);
    let value = trimmed_eol.trim().to_string();
    Ok((
        rest,
        Line::KeyValue {
            key,
            value,
            raw: raw.to_string(),
        },
    ))
}

fn is_key_char(c: char) -> bool {
    !c.is_whitespace() && c != '=' && c != '[' && c != ']' && c != ';' && c != '#'
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const SAMPLE: &str = "\
; top of file comment
[work]
type = smb
host = files.example.com
user = alice

# personal stuff
[home]
type = webdav
url = https://dav.example.org

[empty]
";

    #[test]
    fn round_trip_preserves_bytes() {
        let doc = Document::parse(SAMPLE).unwrap();
        assert_eq!(doc.render(), SAMPLE);
    }

    #[test]
    fn sections_listed_in_order() {
        let doc = Document::parse(SAMPLE).unwrap();
        assert_eq!(doc.sections(), vec!["work", "home", "empty"]);
    }

    #[test]
    fn get_existing_key() {
        let doc = Document::parse(SAMPLE).unwrap();
        assert_eq!(doc.get("work", "host"), Some("files.example.com"));
        assert_eq!(doc.get("home", "type"), Some("webdav"));
    }

    #[test]
    fn section_entries_returns_all_keys_in_order() {
        let doc = Document::parse(SAMPLE).unwrap();
        assert_eq!(
            doc.section_entries("work"),
            vec![("type", "smb"), ("host", "files.example.com"), ("user", "alice")]
        );
        // A section with only a header yields nothing.
        assert!(doc.section_entries("empty").is_empty());
        // An unknown section yields nothing, not a panic.
        assert!(doc.section_entries("nope").is_empty());
    }

    #[test]
    fn get_missing_returns_none() {
        let doc = Document::parse(SAMPLE).unwrap();
        assert_eq!(doc.get("work", "nonexistent"), None);
        assert_eq!(doc.get("nosuchsection", "anything"), None);
    }

    #[test]
    fn set_updates_existing_key_in_place() {
        let mut doc = Document::parse(SAMPLE).unwrap();
        doc.set("work", "host", "new.example.com");
        assert_eq!(doc.get("work", "host"), Some("new.example.com"));
        // The comment above [work] survives.
        assert!(doc.render().contains("; top of file comment"));
        // The original key for [home] is untouched.
        assert_eq!(doc.get("home", "url"), Some("https://dav.example.org"));
    }

    #[test]
    fn set_inserts_new_key_into_existing_section() {
        let mut doc = Document::parse(SAMPLE).unwrap();
        doc.set("work", "domain", "EXAMPLE");
        let out = doc.render();
        // New key is inside [work] section, not at the top.
        let work_idx = out.find("[work]").unwrap();
        let home_idx = out.find("[home]").unwrap();
        let domain_idx = out.find("domain = EXAMPLE").unwrap();
        assert!(work_idx < domain_idx && domain_idx < home_idx);
    }

    #[test]
    fn set_creates_new_section_at_end() {
        let mut doc = Document::parse(SAMPLE).unwrap();
        doc.set("backup", "type", "drive");
        let out = doc.render();
        assert!(out.ends_with("[backup]\ntype = drive\n"));
    }

    #[test]
    fn remove_section_drops_keys_but_leaves_neighbors() {
        let mut doc = Document::parse(SAMPLE).unwrap();
        doc.remove_section("work");
        let out = doc.render();
        assert!(!out.contains("[work]"));
        assert!(!out.contains("files.example.com"));
        assert!(out.contains("[home]"));
        assert!(out.contains("[empty]"));
    }

    #[test]
    fn parse_handles_no_trailing_newline() {
        let input = "[only]\nkey = value";
        let doc = Document::parse(input).unwrap();
        assert_eq!(doc.get("only", "key"), Some("value"));
    }

    #[test]
    fn parse_handles_spaces_around_equals() {
        let doc = Document::parse("[s]\nk1=v1\nk2 =v2\nk3= v3\nk4 = v4\n").unwrap();
        assert_eq!(doc.get("s", "k1"), Some("v1"));
        assert_eq!(doc.get("s", "k2"), Some("v2"));
        assert_eq!(doc.get("s", "k3"), Some("v3"));
        assert_eq!(doc.get("s", "k4"), Some("v4"));
    }

    #[test]
    fn parse_handles_both_comment_styles() {
        let input = "; semicolon\n# hash\n[s]\nk = v\n";
        let doc = Document::parse(input).unwrap();
        assert_eq!(doc.render(), input);
    }
}
