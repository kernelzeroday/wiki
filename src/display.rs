use colored::Colorize;
use std::io::{self, IsTerminal, Write};
use std::process::{Command, Stdio};

pub fn term_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80)
}

pub fn render_html(html: &str, width: usize) -> String {
    let text = html2text::config::plain()
        .no_table_borders()
        .string_from_read(html.as_bytes(), width)
        .unwrap_or_default();
    postprocess(&text)
}

fn postprocess(text: &str) -> String {
    let text = strip_superscript_refs(text);
    let text = strip_ref_numbers(&text);
    let text = strip_link_brackets(&text);
    let text = strip_link_brackets(&text);
    text.lines()
        .filter(|line| !is_footnote_line(line))
        .map(|line| colorize_heading(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_link_brackets(text: &str) -> String {
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
                result.push_str(&text[content_start..i]);
                i += 1;
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

pub fn strip_search_highlight(text: &str) -> String {
    text.replace("<span class=\"searchmatch\">", "")
        .replace("</span>", "")
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
        let clean = strip_search_highlight(exc);
        let oneline = clean.replace('\n', " ");
        let trimmed: String = oneline.chars().take(120).collect();
        println!("     {}", trimmed.dimmed());
    }
    println!();
}
