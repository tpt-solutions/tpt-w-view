// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 TPT Solutions

//! Pattern-based batch rename engine (Phase 5).
//!
//! Supported tokens (write literal braces as `{{` and `}}`):
//!
//! | Token | Meaning |
//! |-------|---------|
//! | `{counter}` / `{counter:N}` | running counter, optionally zero-padded to width N |
//! | `{name}` / `{name:lower}` / `{name:upper}` | original file stem, optional case transform |
//! | `{date}` | EXIF `DateTimeOriginal` as `YYYYMMDD_HHMMSS`, or `unknown` |
//! | `{camera}` | EXIF camera model (path-unsafe characters replaced), or `unknown` |

use crate::error::{Error, Result};

/// Values available to a pattern. `date` is the raw EXIF `DateTimeOriginal` string.
#[derive(Debug, Clone, Copy)]
pub struct RenameContext<'a> {
    pub stem: &'a str,
    pub counter: u32,
    pub date: Option<&'a str>,
    pub camera: Option<&'a str>,
}

/// Renders `pattern` for `ctx`. The result is a bare file stem: it may not be empty and may not
/// contain path separators.
pub fn render(pattern: &str, ctx: &RenameContext<'_>) -> Result<String> {
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '}' => return Err(Error::Invalid("unmatched '}' in rename pattern".into())),
            '{' => {
                let mut token = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(t) => token.push(t),
                        None => {
                            return Err(Error::Invalid("unclosed '{' in rename pattern".into()))
                        }
                    }
                }
                out.push_str(&expand(&token, ctx)?);
            }
            other => out.push(other),
        }
    }

    if out.is_empty() {
        return Err(Error::Invalid(
            "rename pattern produced an empty name".into(),
        ));
    }
    if out.contains(['/', '\\']) {
        return Err(Error::Invalid(
            "rename pattern produced a path separator".into(),
        ));
    }
    Ok(out)
}

fn expand(token: &str, ctx: &RenameContext<'_>) -> Result<String> {
    let (name, arg) = match token.split_once(':') {
        Some((n, a)) => (n, Some(a)),
        None => (token, None),
    };
    match name {
        "counter" => {
            let width = match arg {
                None | Some("") => 0usize,
                Some(a) => a.parse::<usize>().map_err(|_| {
                    Error::Invalid(format!("counter width must be a number, got '{a}'"))
                })?,
            };
            Ok(format!("{:0width$}", ctx.counter, width = width))
        }
        "name" => match arg {
            None | Some("") => Ok(ctx.stem.to_string()),
            Some("lower") => Ok(ctx.stem.to_lowercase()),
            Some("upper") => Ok(ctx.stem.to_uppercase()),
            Some(other) => Err(Error::Invalid(format!("unknown name transform '{other}'"))),
        },
        "date" => Ok(ctx
            .date
            .and_then(format_exif_date)
            .unwrap_or_else(|| "unknown".into())),
        "camera" => Ok(ctx
            .camera
            .map(sanitize)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into())),
        other => Err(Error::Invalid(format!(
            "unknown rename token '{{{other}}}'"
        ))),
    }
}

/// `"2024:01:02 03:04:05"` -> `"20240102_030405"`. Returns `None` for anything malformed.
fn format_exif_date(raw: &str) -> Option<String> {
    let (date, time) = raw.trim().split_once(' ')?;
    let d: Vec<&str> = date.split(':').collect();
    let t: Vec<&str> = time.split(':').collect();
    if d.len() != 3 || t.len() != 3 {
        return None;
    }
    let all: String = d.concat() + &t.concat();
    if all.len() != 14 || !all.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("{}_{}{}{}", d.concat(), t[0], t[1], t[2]))
}

fn sanitize(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(date: Option<&'a str>, camera: Option<&'a str>) -> RenameContext<'a> {
        RenameContext {
            stem: "IMG_0042",
            counter: 7,
            date,
            camera,
        }
    }

    #[test]
    fn counter_and_name_tokens() {
        let c = ctx(None, None);
        assert_eq!(render("{name}", &c).unwrap(), "IMG_0042");
        assert_eq!(render("photo_{counter:4}", &c).unwrap(), "photo_0007");
        assert_eq!(render("{counter}", &c).unwrap(), "7");
        assert_eq!(
            render("{name:lower}-{name:upper}", &c).unwrap(),
            "img_0042-IMG_0042"
        );
    }

    #[test]
    fn exif_date_and_camera_tokens() {
        let c = ctx(Some("2024:01:02 03:04:05"), Some("Canon EOS R5/Pro"));
        assert_eq!(render("{date}", &c).unwrap(), "20240102_030405");
        assert_eq!(render("{camera}", &c).unwrap(), "Canon EOS R5_Pro");
    }

    #[test]
    fn missing_metadata_falls_back_to_unknown() {
        let c = ctx(Some("garbage"), None);
        assert_eq!(render("{date}_{camera}", &c).unwrap(), "unknown_unknown");
    }

    #[test]
    fn escaped_braces_are_literal() {
        assert_eq!(
            render("{{x}}-{name}", &ctx(None, None)).unwrap(),
            "{x}-IMG_0042"
        );
    }

    #[test]
    fn invalid_patterns_are_rejected() {
        let c = ctx(None, None);
        assert!(render("{nope}", &c).is_err());
        assert!(render("{counter:x}", &c).is_err());
        assert!(render("{name", &c).is_err());
        assert!(render("name}", &c).is_err());
        assert!(render("", &c).is_err());
        assert!(render("a/b", &c).is_err());
        assert!(render("{name:weird}", &c).is_err());
    }
}
