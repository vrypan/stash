use std::collections::BTreeMap;

use super::Meta;

pub(super) fn encode_attr(meta: &Meta) -> String {
    let mut out = String::new();
    write_attr_line(&mut out, "id", &meta.id);
    write_attr_line(&mut out, "ts", &meta.ts);
    write_attr_line(&mut out, "size", &meta.size.to_string());
    if !meta.preview.trim().is_empty() {
        write_attr_line(&mut out, "preview", &meta.preview);
    }
    for (k, v) in &meta.attrs {
        write_attr_line(&mut out, k, v);
    }
    out
}

fn write_attr_line(out: &mut String, key: &str, value: &str) {
    out.push_str(&escape_attr(key));
    out.push('=');
    out.push_str(&escape_attr(value));
    out.push('\n');
}

// Escapes a value for storage in the attr file format.
// '=' must be escaped here because it is the key=value delimiter.
// See also: escape_attr_output in display.rs, which intentionally omits '='.
fn escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '=' => out.push_str("\\="),
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn parse_attr_file(input: &str) -> Result<Meta, String> {
    let mut meta = Meta {
        id: String::new(),
        ts: String::new(),
        size: 0,
        preview: String::new(),
        attrs: BTreeMap::new(),
    };
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = split_attr_line(line) else {
            return Err(format!("invalid attr line {line:?}"));
        };
        let key = unescape_attr(key)?;
        let value = unescape_attr(value)?;
        match key.as_str() {
            "id" => meta.id = value,
            "ts" => meta.ts = value,
            "size" => {
                meta.size = value
                    .parse::<i64>()
                    .map_err(|_| format!("invalid size {value:?}"))?
            }
            "preview" => meta.preview = value,
            _ => {
                meta.attrs.insert(key, value);
            }
        }
    }
    Ok(meta)
}

fn split_attr_line(line: &str) -> Option<(&str, &str)> {
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '\\' => escaped = !escaped,
            '=' if !escaped => return Some((&line[..idx], &line[idx + 1..])),
            _ => escaped = false,
        }
    }
    None
}

fn unescape_attr(input: &str) -> Result<String, String> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            return Err("unterminated attr escape".into());
        };
        match next {
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '=' => out.push('='),
            other => return Err(format!("invalid attr escape \\{other}")),
        }
    }
    Ok(out)
}
