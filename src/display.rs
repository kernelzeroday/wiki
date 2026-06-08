use colored::Colorize;
use std::io::{self, IsTerminal, Write};
use std::process::{Command, Stdio};

pub fn term_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80)
}

pub fn render_html(html: &str, width: usize) -> String {
    use html2text::render::RichAnnotation;

    let lines = html2text::config::rich()
        .lines_from_read(html.as_bytes(), width)
        .unwrap_or_default();

    let mut text = String::new();
    for line in &lines {
        let mut current_url: Option<&str> = None;
        let mut link_buf = String::new();

        for ts in line.tagged_strings() {
            let url = ts.tag.iter().find_map(|a| match a {
                RichAnnotation::Link(u) => Some(u.as_str()),
                _ => None,
            });

            match (current_url, url) {
                (None, None) => text.push_str(&ts.s),
                (None, Some(u)) => {
                    current_url = Some(u);
                    link_buf.clear();
                    link_buf.push_str(&ts.s);
                }
                (Some(prev), Some(u)) if prev == u => {
                    link_buf.push_str(&ts.s);
                }
                (Some(prev), next) => {
                    emit_link_marker(&mut text, prev, &link_buf);
                    link_buf.clear();
                    match next {
                        Some(u) => {
                            current_url = Some(u);
                            link_buf.push_str(&ts.s);
                        }
                        None => {
                            current_url = None;
                            text.push_str(&ts.s);
                        }
                    }
                }
            }
        }
        if let Some(u) = current_url {
            emit_link_marker(&mut text, u, &link_buf);
        }
        text.push('\n');
    }

    postprocess(&text)
}

fn postprocess(text: &str) -> String {
    let text = strip_superscript_refs(text);
    let text = strip_ref_numbers(&text);
    let text = style_links(&text, &mut std::collections::HashSet::new());
    let text = flatten_tables(&text);
    text.lines()
        .filter(|line| !is_footnote_line(line))
        .map(|line| colorize_heading(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn emit_link_marker(text: &mut String, url: &str, display: &str) {
    text.push('[');
    text.push('\x01');
    text.push_str(url);
    text.push('\x01');
    text.push_str(display);
    text.push(']');
}

fn is_box_drawing(c: char) -> bool {
    ('\u{2500}'..='\u{257F}').contains(&c)
}

fn is_border_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|c| is_box_drawing(c) || c.is_whitespace())
}

fn flatten_tables(text: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut prev_was_table = false;

    for line in text.lines() {
        if is_border_line(line) {
            continue;
        }

        if line.contains('\u{2502}') {
            let parts: Vec<&str> = line.split('\u{2502}').map(|s| s.trim()).collect();
            let label = parts[0];
            let values: Vec<&str> =
                parts[1..].iter().filter(|s| !s.is_empty()).copied().collect();

            if !label.is_empty() && !values.is_empty() {
                result.push(format!("{}", label.dimmed()));
                result.push(format!("  {}", values.join("  ")));
            } else if !label.is_empty() {
                result.push(format!("{}", label.dimmed()));
            } else if !values.is_empty() {
                result.push(format!("  {}", values.join("  ")));
            }
            prev_was_table = true;
        } else {
            if prev_was_table {
                result.push(String::new());
            }
            result.push(line.trim_end().to_string());
            prev_was_table = false;
        }
    }
    result.join("\n")
}

fn style_links(text: &str, seen: &mut std::collections::HashSet<String>) -> String {
    let mut result = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let open = i;
            i += 1;
            let content_start = i;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'[' {
                    depth += 1;
                } else if bytes[i] == b']' {
                    depth -= 1;
                }
                if depth > 0 {
                    i += 1;
                }
            }
            if depth == 0 {
                let inner = &text[content_start..i];
                i += 1;
                if let Some(rest) = inner.strip_prefix('\x01') {
                    if let Some(sep) = rest.find('\x01') {
                        let url = &rest[..sep];
                        let display = &rest[sep + 1..];
                        let styled_display = style_links(display, seen);
                        result.push_str(&styled_display.underline().to_string());
                        if let Some(title) = wiki_title_from_url(url) {
                            if !titles_match(&title, display.trim()) && seen.insert(url.to_string()) {
                                result.push_str(&format!(" [{}]", title).dimmed().to_string());
                            }
                        }
                    } else {
                        result.push_str(&inner.underline().to_string());
                    }
                } else {
                    let styled = style_links(inner, seen);
                    result.push_str(&styled.underline().to_string());
                }
            } else {
                result.push_str(&text[open..i]);
            }
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b'[' {
                i += 1;
            }
            result.push_str(&text[start..i]);
        }
    }
    result
}

fn wiki_title_from_url(url: &str) -> Option<String> {
    let path = url.strip_prefix("./").or_else(|| {
        url.find("/wiki/").map(|pos| &url[pos + 6..])
    })?;
    let path = path.split('#').next().unwrap_or(path);
    if path.is_empty() {
        return None;
    }
    let decoded = urlencoding::decode(path).unwrap_or_else(|_| path.into());
    let title = decoded.replace('_', " ");
    let dominated_by_namespace = title.starts_with("File:")
        || title.starts_with("Category:")
        || title.starts_with("Special:")
        || title.starts_with("Wikipedia:")
        || title.starts_with("Help:")
        || title.starts_with("Template:")
        || title.starts_with("Template talk:")
        || title.starts_with("Talk:")
        || title.starts_with("Portal:");
    if dominated_by_namespace {
        return None;
    }
    Some(title)
}

fn titles_match(link_title: &str, display: &str) -> bool {
    let norm = |s: &str| {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(|c| c.to_lowercase())
            .collect::<String>()
    };
    norm(link_title) == norm(display)
}

fn strip_superscript_refs(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("^{") {
        result.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        let mut depth = 1;
        let mut end = None;
        for (i, b) in after.bytes().enumerate() {
            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
            }
            if depth == 0 {
                end = Some(i);
                break;
            }
        }
        if let Some(e) = end {
            rest = &after[e + 1..];
        } else {
            result.push_str("^{");
            rest = after;
        }
    }
    result.push_str(rest);
    result
}

fn is_footnote_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('[') {
        return false;
    }
    let rest = &trimmed[1..];
    if let Some(pos) = rest.find(']') {
        let inside = &rest[..pos];
        !inside.is_empty()
            && inside.bytes().all(|b| b.is_ascii_digit())
            && rest[pos + 1..].starts_with(": ")
    } else {
        false
    }
}

fn strip_ref_numbers(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let start = i;
            i += 1;
            let digit_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > digit_start && i < bytes.len() && bytes[i] == b']' {
                i += 1;
                continue;
            }
            result.push_str(&line[start..i]);
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b'[' {
                i += 1;
            }
            result.push_str(&line[start..i]);
        }
    }
    result
}

fn colorize_heading(line: &str) -> String {
    let trimmed = line.trim_start();
    if trimmed.starts_with("### ") {
        line.bold().to_string()
    } else if trimmed.starts_with("## ") {
        line.bold().cyan().to_string()
    } else if trimmed.starts_with("# ") {
        line.bold().blue().to_string()
    } else {
        line.to_string()
    }
}

pub fn paged_print(content: &str) {
    if !io::stdout().is_terminal() {
        print!("{content}");
        return;
    }

    let pager_env = std::env::var("PAGER").unwrap_or_else(|_| "less".to_string());
    let mut parts = pager_env.split_whitespace();
    let pager = parts.next().unwrap_or("less");
    let mut args: Vec<&str> = parts.collect();

    if pager.ends_with("less") && args.is_empty() {
        args = vec!["-RFX"];
    }

    match Command::new(pager)
        .args(&args)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(content.as_bytes());
                drop(stdin);
            }
            let _ = child.wait();
        }
        Err(_) => {
            print!("{content}");
        }
    }
}

pub fn clean_excerpt(text: &str) -> String {
    let text = text
        .replace("<span class=\"searchmatch\">", "")
        .replace("</span>", "");
    decode_html_entities(&text)
}

fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

pub fn print_summary(title: &str, description: Option<&str>, extract: Option<&str>, url: Option<&str>) {
    println!("{}", title.bold().blue());
    if let Some(desc) = description {
        println!("{}", desc.dimmed());
    }
    println!();
    if let Some(text) = extract {
        let width = term_width().min(100);
        let wrapped = textwrap::fill(text, width);
        println!("{wrapped}");
    }
    if let Some(u) = url {
        println!("\n{}", u.dimmed());
    }
}

pub fn print_search_result(idx: usize, title: &str, description: Option<&str>, excerpt: Option<&str>) {
    println!("  {}. {}", idx + 1, title.bold().cyan());
    if let Some(desc) = description {
        println!("     {}", desc.dimmed());
    }
    if let Some(exc) = excerpt {
        let clean = clean_excerpt(exc);
        let oneline = clean.replace('\n', " ");
        let trimmed: String = oneline.chars().take(120).collect();
        println!("     {}", trimmed.dimmed());
    }
    println!();
}
