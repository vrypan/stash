use std::collections::BTreeMap;
use std::fmt::Write;

use super::Meta;

pub(super) fn encode_attr(meta: &Meta) -> String {
    let mut out = String::new();
    write_attr_line(&mut out, "id", &meta.id);
    write_attr_line(&mut out, "ts", &meta.ts);
    let _ = write!(out, "size={}\n", meta.size);
    if !meta.preview.trim().is_empty() {
        write_attr_line(&mut out, "preview", &meta.preview);
    }
    for (k, v) in &meta.attrs {
        write_attr_line(&mut out, k, v);
    }
    out
}

#[inline]
fn write_attr_line(out: &mut String, key: &str, value: &str) {
    escape_attr_into(out, key);
    out.push('=');
    escape_attr_into(out, value);
    out.push('\n');
}

// Escapes a value for storage in the attr file format.
// '=' must be escaped here because it is the key=value delimiter.
// See also: escape_attr_output in display.rs, which intentionally omits '='.
#[inline]
fn escape_attr_into(out: &mut String, value: &str) {
    // Fast path: if no special chars, just append directly
    if !value
        .bytes()
        .any(|b| matches!(b, b'\\' | b'\n' | b'\r' | b'\t' | b'='))
    {
        out.push_str(value);
        return;
    }
    out.reserve(value.len());
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
        // Fast path: if key has no backslash, avoid allocation from unescape
        let key_owned;
        let key_str = if key.contains('\\') {
            key_owned = unescape_attr(key)?;
            key_owned.as_str()
        } else {
            key
        };
        match key_str {
            "id" => meta.id = unescape_attr_or_clone(value)?,
            "ts" => meta.ts = unescape_attr_or_clone(value)?,
            "size" => {
                // Size values should never contain escapes; parse directly
                meta.size = value
                    .parse::<i64>()
                    .map_err(|_| format!("invalid size {value:?}"))?
            }
            "preview" => meta.preview = unescape_attr_or_clone(value)?,
            _ => {
                meta.attrs
                    .insert(key_str.to_owned(), unescape_attr_or_clone(value)?);
            }
        }
    }
    Ok(meta)
}

/// Unescapes a value, but avoids allocation if no escape sequences are present.
#[inline]
fn unescape_attr_or_clone(input: &str) -> Result<String, String> {
    if input.contains('\\') {
        unescape_attr(input)
    } else {
        Ok(input.to_owned())
    }
}

fn split_attr_line(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut escaped = false;
    for (idx, &b) in bytes.iter().enumerate() {
        match b {
            b'\\' => escaped = !escaped,
            b'=' if !escaped => return Some((&line[..idx], &line[idx + 1..])),
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
