//! Rustfmt and conservative Markdown prose wrapping; code and tables stay intact.
use super::Result;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

const PROSE_WIDTH: usize = 100;

pub(super) fn width(line: &str) -> usize {
    line.chars().map(|ch| if ch == '\t' { 4 } else { 1 }).sum()
}

pub(super) fn format(root: &Path, file: &Path, source: &str) -> Result<String> {
    match file.extension().and_then(|extension| extension.to_str()) {
        Some("md") => Ok(markdown(source)),
        Some("rs") => {
            let mut child = Command::new("rustfmt")
                .current_dir(root)
                .args([
                    "--edition",
                    "2021",
                    "--config",
                    "skip_children=true",
                    "--emit",
                    "stdout",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            child
                .stdin
                .take()
                .ok_or("missing rustfmt stdin")?
                .write_all(source.as_bytes())?;
            let output = child.wait_with_output()?;
            if !output.status.success() {
                return Err(format!(
                    "{}: rustfmt failed:\n{}",
                    file.display(),
                    String::from_utf8_lossy(&output.stderr)
                )
                .into());
            }
            Ok(String::from_utf8(output.stdout)?)
        }
        _ => Ok(source.to_owned()),
    }
}

fn fence(line: &str) -> Option<(char, usize, bool)> {
    let (_, _, body) = prefixes(line);
    let start = body.trim_start();
    let ch = start.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let count = start.chars().take_while(|c| *c == ch).count();
    (count >= 3).then(|| (ch, count, start[count..].trim().is_empty()))
}

fn markdown(source: &str) -> String {
    let mut result = String::new();
    let mut open_fence = None;
    let mut frontmatter = None;
    let mut raw_html = None;
    for (index, raw) in source.split_inclusive('\n').enumerate() {
        let (line, newline) = if let Some(line) = raw.strip_suffix("\r\n") {
            (line, "\r\n")
        } else if let Some(line) = raw.strip_suffix('\n') {
            (line, "\n")
        } else {
            (raw, "")
        };
        if index == 0 && matches!(line, "---" | "+++") {
            frontmatter = Some(line);
            result.push_str(raw);
            continue;
        }
        if let Some(delimiter) = frontmatter {
            if line == delimiter {
                frontmatter = None;
            }
            result.push_str(raw);
            continue;
        }
        let trimmed = line.trim_start();
        if raw_html.is_none() && open_fence.is_none() {
            raw_html = ["pre", "script", "style", "textarea"]
                .into_iter()
                .find(|tag| trimmed.starts_with(&format!("<{tag}")));
        }
        if let Some(tag) = raw_html {
            if line.contains(&format!("</{tag}>")) {
                raw_html = None;
            }
            result.push_str(raw);
            continue;
        }
        let delimiter = fence(line);
        let in_fence = open_fence.is_some();
        if let Some((ch, count, closes)) = delimiter {
            match open_fence {
                Some((open, size)) if ch == open && count >= size && closes => {
                    open_fence = None;
                }
                None => open_fence = Some((ch, count)),
                _ => {}
            }
        }
        let indented = width(&line[..line.len() - trimmed.len()]) >= 4;
        let protected = in_fence
            || delimiter.is_some()
            || indented
            || trimmed.starts_with(['#', '<'])
            || line.contains('|')
            || (trimmed.starts_with('[') && trimmed.contains("]:"));
        if protected || width(line) <= PROSE_WIDTH {
            result.push_str(line);
        } else {
            result.push_str(&wrap_line(
                line,
                if newline.is_empty() { "\n" } else { newline },
            ));
        }
        result.push_str(newline);
    }
    result
}

fn prefixes(line: &str) -> (&str, String, &str) {
    let mut offset = line.len() - line.trim_start().len();
    while line[offset..].starts_with("> ") {
        offset += 2;
    }
    let quote_end = offset;
    let body = &line[offset..];
    let marker = if body.starts_with("- ") || body.starts_with("* ") || body.starts_with("+ ") {
        2
    } else {
        let digits = body.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && (body[digits..].starts_with(". ") || body[digits..].starts_with(") ")) {
            digits + 2
        } else {
            0
        }
    };
    offset += marker;
    let continuation = format!("{}{}", &line[..quote_end], " ".repeat(marker));
    (&line[..offset], continuation, &line[offset..])
}

fn prose_words(source: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut ticks = 0;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '`' && !current.ends_with('\\') {
            let mut count = 1;
            while chars.peek() == Some(&'`') {
                chars.next();
                count += 1;
            }
            current.push_str(&"`".repeat(count));
            if ticks == 0 {
                ticks = count;
            } else if ticks == count {
                ticks = 0;
            }
        } else if ch.is_whitespace() && ticks == 0 && !current.ends_with('\\') {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn wrap_line(line: &str, newline: &str) -> String {
    let (line, hard_break) = if let Some(line) = line.strip_suffix("  ") {
        (line, "  ")
    } else if let Some(line) = line.strip_suffix('\\') {
        (line, "\\")
    } else {
        (line, "")
    };
    let (first, continuation, body) = prefixes(line);
    let mut output = String::from(first);
    let mut column = width(first);
    let mut has_word = false;
    let maximum = PROSE_WIDTH - width(hard_break);
    for word in prose_words(body) {
        let word_width = width(&word);
        if has_word && column + 1 + word_width > maximum {
            output.push_str(newline);
            output.push_str(&continuation);
            column = width(&continuation);
            has_word = false;
        }
        if has_word {
            output.push(' ');
            column += 1;
        }
        output.push_str(&word);
        column += word_width;
        has_word = true;
    }
    output.push_str(hard_break);
    output
}
