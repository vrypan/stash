# Refactoring & Performance Plan

Issues are grouped by priority. Each item includes the specific change needed and affected locations.

---

## Priority 1 — High Impact

### P1. `attrs_command` N+1 cache reads
**File:** `src/cli.rs:881-891`, `src/store.rs:275-291`

`attrs_command` calls `all_attr_keys()` to get the list of keys, then calls
`attr_count_for_key(&key)` for each one — each of which re-deserializes the
bincode cache file from disk.

**Fix:** Change `all_attr_keys()` to return `Vec<(String, usize)>` (key + count),
reading the cache exactly once. Remove `attr_count_for_key` (no other callers).
Update `attrs_command` to destructure the tuple instead of calling the removed function.

```rust
// store.rs: change signature
pub fn all_attr_keys() -> io::Result<Vec<(String, usize)>> {
    if let Ok(cache) = read_list_cache_file() {
        return Ok(cache.attr_keys.into_iter().collect());
    }
    let items = list()?;
    Ok(build_attr_key_index(&items).into_iter().collect())
}

// cli.rs: updated loop
for (key, count) in store::all_attr_keys()? {
    if args.count { println!("{key}\t{count}"); }
    else          { println!("{key}"); }
}
```

---

### P2. Full list load for single-entry operations *(abandoned)*
**File:** `src/store.rs:293-297`

`newest()` calls `list()` (loads + deserializes all entries) then takes only the
first. Since entries are ULID-sorted and stored by filename, the newest entry is
simply the last filename in `data_dir`.

**Fix:** Add a `newest_id()` fast path that reads only `data_dir` entries without
touching the cache or attr files:

```rust
pub fn newest() -> io::Result<Meta> {
    // Fast path: last entry in the sorted data dir
    let dir = data_dir()?;
    let mut entries: Vec<_> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    entries.sort_unstable();
    let id = entries.into_iter().next_back()
        .ok_or_else(|| io::Error::other("stash is empty"))?;
    get_meta(&id.to_ascii_uppercase())
}
```

`nth_newest` can stay as-is (it needs the full list anyway for arbitrary N).

---

## Priority 2 — Medium Impact

### P3. Eliminate `CachedMeta` / double allocation on every cache read and write
**File:** `src/store.rs:48-99, 228-263`

`Meta` and `CachedMeta` are structurally identical structs. Every cache read
does `.map(CachedMeta -> Meta)` (cloning all strings); every write does
`.iter().cloned().map(Meta -> CachedMeta)` (another full deep copy).

**Fix:**
1. Add `#[derive(Serialize, Deserialize)]` to `Meta`.
2. Change `ListCacheFile.items` from `Vec<CachedMeta>` to `Vec<Meta>`.
3. Delete `CachedMeta` and both `From` impls.
4. Update `read_list_cache` and `write_list_cache` to use `Meta` directly.

```rust
// store.rs: before Meta definition
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta { ... }

// ListCacheFile
struct ListCacheFile {
    ...
    items: Vec<Meta>,          // was Vec<CachedMeta>
    ...
}

// read_list_cache: drop the .map(Into::into)
fn read_list_cache() -> io::Result<Vec<Meta>> {
    Ok(read_list_cache_file()?.items)
}

// write_list_cache: drop .iter().cloned().map(Into::into)
items: items.to_vec(),
```

---

### P4. Deduplicate `push_from_reader` / `tee_from_reader_partial` read loops
**File:** `src/store.rs:436-627`

Both functions share ~70 lines of identical logic: signal setup, buffer loop,
sample accumulation, partial-save handling. The only difference is that `tee`
also writes each buffer chunk to `stdout`.

**Fix:** Extract an inner `read_loop` function with an `Option<&mut dyn Write>`
for the optional tee output:

```rust
fn read_loop<R: Read>(
    reader: &mut R,
    tee_out: Option<&mut dyn Write>,
    interrupted: &Arc<AtomicBool>,
    signal: &Arc<AtomicI32>,
    id: &str,
    data_path: &Path,
    attrs: BTreeMap<String, String>,
    opts: PartialSaveOptions,
) -> io::Result<(i64, Vec<u8>)> { ... }
```

`push_from_reader` passes `None`; `tee_from_reader_partial` passes `Some(stdout)`.

---

### P5. Deduplicate `unix_to_utc` / `civil_from_days`
**File:** `src/store.rs:746-768`, `src/cli.rs:1583-1617`

Identical copies exist in both files.

**Fix:** Keep the implementation in `store.rs` and make both functions `pub`.
In `cli.rs` remove the local copies and call `store::unix_to_utc` / `store::civil_from_days`
(or `use store::{unix_to_utc, civil_from_days}`). Optionally introduce a `UtcDateTime`
struct to replace the opaque 6-tuple return type (see P8 below).

---

## Priority 3 — Low Impact / Code Quality

### P6. `preview_snippet` double `chars()` scan
**File:** `src/cli.rs:1428-1444`

`preview.chars().count()` does a full scan just to know if `total > chars`.
The same information can be obtained during the existing enumerate loop.

**Fix:**
```rust
fn preview_snippet(preview: &str, chars: usize) -> String {
    if chars == 0 { return String::new(); }
    let mut out = String::new();
    let mut count = 0usize;
    for ch in preview.chars() {
        if count >= chars { break; }
        out.push(ch);
        count += 1;
    }
    let truncated = preview.chars().nth(chars).is_some();
    if truncated && chars > 3 { out.push_str("..."); }
    out
}
```

---

### P7. `auto_ls_preview_chars` allocates strings just to measure width
**File:** `src/cli.rs:1270-1295`

`format_size(...)` and `format_date(...)` are called per-item only to measure
`.len()`, then the results are discarded and the same calls are made again in
`decorate_entries`.

**Fix:** Add `measure_size_width(size: i64, mode: &str) -> usize` and
`measure_date_width(ts: &str, mode: &str) -> usize` that compute the length
without allocating a `String` (e.g., using integer arithmetic for size, or
returning a known-constant width for each date mode).

---

### P8. `unix_to_utc` returns an opaque 6-tuple
**File:** `src/store.rs:746`, `src/cli.rs:1595`

The return type `(i32, u32, u32, u32, u32, u32)` is undocumented and
error-prone at call sites (several callers discard 5 of 6 fields).

**Fix:** Introduce a small struct (done alongside P5):
```rust
pub struct UtcDateTime { pub year: i32, pub month: u32, pub day: u32,
                         pub hour: u32, pub min: u32,  pub sec: u32 }
```
Update all call sites to use field access instead of positional destructuring.

---

### P9. `trim_ansi_to_width` re-validates UTF-8 per byte
**File:** `src/cli.rs:1237`

`s[i..].chars().next().unwrap()` starts a new UTF-8 scan from position `i` on
every iteration.

**Fix:** Use `s.char_indices()` as the outer iterator and detect ANSI escapes
by peeking at the raw byte slice, avoiding repeated UTF-8 validation:

```rust
fn trim_ansi_to_width(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut visible = 0usize;
    let bytes = s.as_bytes();
    let mut chars = s.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '\x1b' && bytes.get(i + 1) == Some(&b'[') {
            // consume until final byte of CSI sequence
            let start = i;
            let end = bytes[i + 2..].iter().position(|&b| (0x40..=0x7e).contains(&b))
                .map(|p| i + 2 + p + 1).unwrap_or(bytes.len());
            out.push_str(&s[start..end]);
            // advance chars iterator past the escape bytes
            while chars.peek().map(|(j, _)| *j < end).unwrap_or(false) { chars.next(); }
            continue;
        }
        if visible >= width { break; }
        out.push(ch);
        visible += 1;
    }
    if visible >= width { out.push_str("\x1b[0m"); }
    out
}
```

---

### P10. `parse_attr_file` wraps `&str` in `io::Cursor`
**File:** `src/store.rs:848`

`io::Cursor::new(input).lines()` allocates a `String` per line via `BufRead`.
The input is already a `&str`.

**Fix:** Replace with `input.lines()` which yields `&str` slices directly,
then adjust downstream code that expected `io::Result<String>` to handle `&str`.

---

### P11. `parse_meta_selection` dedup is O(n²)
**File:** `src/cli.rs:407`

`Vec::contains` is called for each new tag, making dedup O(n²) in the number
of `--attr` flags. Fine for typical usage but not idiomatic.

**Fix:**
```rust
let mut seen = std::collections::HashSet::new();
for value in values {
    ...
    if seen.insert(value.as_str()) {
        out.tags.push(value.clone());
    }
}
```

---

### P12. `encode_ulid` uses `from_utf8_lossy().into_owned()`
**File:** `src/store.rs:795`

The output buffer is guaranteed ASCII-only (Crockford alphabet). `from_utf8_lossy`
still allocates via `.into_owned()`.

**Fix:**
```rust
// SAFETY: ALPHABET contains only ASCII bytes
unsafe { String::from_utf8_unchecked(out.to_vec()) }
```
Or use `String::from_utf8(out.to_vec()).expect("ASCII alphabet")` for a safe
but still infallible version.

---

### P13. `escape_attr` / `escape_attr_output` near-duplication
**File:** `src/store.rs:825`, `src/cli.rs:894`

Two nearly identical escape functions differ only in whether `=` is escaped.
The distinction is meaningful (storage format vs display) but the duplication
is fragile.

**Fix:** Merge into one function with a parameter, or keep both but add a doc
comment explaining the intentional difference to prevent accidental convergence.

---

## Suggested Implementation Order

1. **P1** — `attrs_command` N+1 reads (isolated, high payoff, small change)
2. **P3** — Eliminate `CachedMeta` (touches data layer, run benchmarks before/after)
3. **P2** — `newest()` fast path (isolated, measurable for `cat`/`pop` commands)
4. **P5 + P8** — Consolidate `unix_to_utc` + introduce `UtcDateTime` (do together)
5. **P4** — Factor out shared read loop in push/tee
6. **P6, P7, P9, P10, P11, P12, P13** — remaining cleanups in any order

Run `cargo bench` before and after P2 and P3 to verify improvement.
