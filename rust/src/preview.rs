use std::borrow::Cow;

pub const MAX_STORED_PREVIEW_RUNES: usize = 128;

pub fn build_preview_data(buf: &[u8], chars: usize) -> String {
    if buf.is_empty() {
        return String::new();
    }
    let limit = if chars == 0 || chars > MAX_STORED_PREVIEW_RUNES {
        MAX_STORED_PREVIEW_RUNES
    } else {
        chars
    };
    build_text_preview(buf, limit)
}

fn build_text_preview(buf: &[u8], chars: usize) -> String {
    let decoded: Cow<'_, str> = String::from_utf8_lossy(buf);
    let mut out = String::new();
    let mut last = None;
    let mut count = 0usize;

    for mut ch in decoded.chars() {
        if count >= chars {
            break;
        }
        ch = match ch {
            '\u{FFFD}' => '.',
            '\n' | '\r' | '\t' => ' ',
            c if !c.is_control() => c,
            _ => '.',
        };
        if last == Some(' ') && ch == ' ' {
            continue;
        }
        out.push(ch);
        last = Some(ch);
        count += 1;
    }

    out.trim().to_string()
}
