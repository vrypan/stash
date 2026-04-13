use clap::{ArgAction, Args};
use std::collections::{BTreeMap, HashSet};
use std::io::{self, Write};

use crate::store;
use crate::store::Meta;

#[derive(Args, Debug, Clone)]
pub(crate) struct RmArgs {
    #[arg(help = "Entry references to remove")]
    refs: Vec<String>,

    #[arg(long, help = "Remove entries older than the referenced entry")]
    before: Option<String>,

    #[arg(long, help = "Remove entries newer than the referenced entry")]
    after: Option<String>,

    #[arg(short = 'a', long = "attr", value_name = "name|name=value", action = ArgAction::Append, help = "Remove entries where an attribute is set, or equals a value (repeatable)")]
    attr: Vec<String>,

    #[arg(short = 'f', long = "force", help = "Do not prompt for confirmation")]
    force: bool,
}

#[derive(Clone, Debug)]
struct RmAttrFilter {
    key: String,
    value: Option<String>,
}

fn parse_rm_attr_filters(values: &[String]) -> io::Result<Vec<RmAttrFilter>> {
    let mut filters = Vec::new();
    for value in values {
        if value.trim().is_empty() || value.contains(',') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--attr accepts name or name=value and is repeatable",
            ));
        }
        if let Some((key, attr_value)) = value.split_once('=') {
            filters.push(RmAttrFilter {
                key: key.to_string(),
                value: Some(attr_value.to_string()),
            });
        } else {
            filters.push(RmAttrFilter {
                key: value.to_string(),
                value: None,
            });
        }
    }
    Ok(filters)
}

fn matches_rm_attr_filters(attrs: &BTreeMap<String, String>, filters: &[RmAttrFilter]) -> bool {
    filters.iter().all(|filter| match &filter.value {
        Some(value) => attrs.get(&filter.key) == Some(value),
        None => attrs.contains_key(&filter.key),
    })
}

fn confirm_rm_before(reference: &str, count: usize) -> io::Result<bool> {
    if count == 1 {
        eprint!("Remove 1 entry older than {}? [y/N] ", reference);
    } else {
        eprint!("Remove {} entries older than {}? [y/N] ", count, reference);
    }
    io::stderr().flush()?;
    let mut reply = String::new();
    io::stdin().read_line(&mut reply)?;
    let reply = reply.trim().to_ascii_lowercase();
    Ok(reply == "y" || reply == "yes")
}

fn confirm_rm_after(reference: &str, count: usize) -> io::Result<bool> {
    if count == 1 {
        eprint!("Remove 1 entry newer than {}? [y/N] ", reference);
    } else {
        eprint!("Remove {} entries newer than {}? [y/N] ", count, reference);
    }
    io::stderr().flush()?;
    let mut reply = String::new();
    io::stdin().read_line(&mut reply)?;
    let reply = reply.trim().to_ascii_lowercase();
    Ok(reply == "y" || reply == "yes")
}

fn confirm_rm_entries(reason: &str, entries: &[Meta]) -> io::Result<bool> {
    eprintln!(
        "Remove {} entr{} {}:",
        entries.len(),
        if entries.len() == 1 { "y" } else { "ies" },
        reason
    );
    for entry in entries {
        if let Some(name) = entry.attrs.get("filename") {
            eprintln!("  {}  {}  {}", entry.short_id(), entry.ts, name);
        } else {
            eprintln!("  {}  {}", entry.short_id(), entry.ts);
        }
    }
    eprint!("Continue? [y/N] ");
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

pub(super) fn rm_command(args: RmArgs) -> io::Result<()> {
    if args.before.is_some() && args.after.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "rm accepts at most one of --before or --after",
        ));
    }

    if !args.attr.is_empty() {
        if !args.refs.is_empty() || args.before.is_some() || args.after.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "rm accepts either <ref>..., --before, --after, or --attr",
            ));
        }
        let filters = parse_rm_attr_filters(&args.attr)?;
        let matches: Vec<Meta> = store::list()?
            .into_iter()
            .filter(|meta| matches_rm_attr_filters(&meta.attrs, &filters))
            .collect();
        if matches.is_empty() {
            return Ok(());
        }
        if !args.force && !confirm_rm_entries("matching attributes", &matches)? {
            return Ok(());
        }
        for meta in matches {
            store::remove(&meta.id)?;
        }
        return Ok(());
    }

    if let Some(before_ref) = args.before.as_deref() {
        if !args.refs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "rm accepts either <ref>..., --before, or --after",
            ));
        }
        let id = store::resolve(before_ref)?;
        let ids = store::older_than_ids(&id)?;
        if ids.is_empty() {
            return Ok(());
        }
        if !args.force && !confirm_rm_before(before_ref, ids.len())? {
            return Ok(());
        }
        for id in ids {
            store::remove(&id)?;
        }
        return Ok(());
    }

    if let Some(after_ref) = args.after.as_deref() {
        if !args.refs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "rm accepts either <ref>..., --before, or --after",
            ));
        }
        let id = store::resolve(after_ref)?;
        let ids = store::newer_than_ids(&id)?;
        if ids.is_empty() {
            return Ok(());
        }
        if !args.force && !confirm_rm_after(after_ref, ids.len())? {
            return Ok(());
        }
        for id in ids {
            store::remove(&id)?;
        }
        return Ok(());
    }

    if args.refs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "rm requires at least one ref",
        ));
    }

    let mut seen = HashSet::new();
    let mut ids: Vec<String> = Vec::new();
    for reference in &args.refs {
        let id = store::resolve(reference)?;
        if seen.insert(id.clone()) {
            ids.push(id);
        }
    }
    if ids.len() == 1 {
        return store::remove(&ids[0]);
    }

    let mut entries = Vec::new();
    for id in &ids {
        entries.push(store::get_meta(id)?);
    }
    if !args.force && !confirm_rm_entries("matching refs", &entries)? {
        return Ok(());
    }
    for id in ids {
        store::remove(&id)?;
    }
    Ok(())
}
