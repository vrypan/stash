use clap::{ArgAction, Args};
use std::collections::BTreeMap;
use std::io;

use crate::display::{attr_value, escape_attr_output, is_writable_attr_key};
use crate::store;

#[derive(Args, Debug, Clone)]
pub(crate) struct AttrArgs {
    #[arg(help = "Entry reference: id, n, or @n")]
    reference: String,

    #[arg(
        value_name = "KEY|KEY=VALUE",
        help = "Attribute keys to read, or key=value pairs to write"
    )]
    items: Vec<String>,

    #[arg(
        long,
        default_value = "\t",
        help = "Separator used between key and value"
    )]
    separator: String,

    #[arg(long = "unset", value_name = "KEY", action = ArgAction::Append, help = "Remove attribute key (repeatable)")]
    unset: Vec<String>,

    #[arg(long, help = "Output attributes as JSON")]
    json: bool,

    #[arg(
        short = 'p',
        long = "preview",
        help = "Include preview pseudo-property when available"
    )]
    preview: bool,
}

pub(super) fn attr_command(args: AttrArgs) -> io::Result<()> {
    let id = store::resolve(&args.reference)?;
    if !args.unset.is_empty() {
        if !args.items.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot combine --unset with reads or writes",
            ));
        }
        for key in &args.unset {
            if !is_writable_attr_key(key) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("only user-defined attributes are writable: {key:?}"),
                ));
            }
        }
        return store::unset_attrs(&id, &args.unset);
    }

    let has_writes = args.items.iter().any(|item| item.contains('='));
    let has_reads = args.items.iter().any(|item| !item.contains('='));
    if has_writes && has_reads {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot mix attribute reads and writes",
        ));
    }

    if has_writes {
        let mut attrs = BTreeMap::new();
        for pair in &args.items {
            let Some((k, v)) = pair.split_once('=') else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "expected key=value",
                ));
            };
            if !is_writable_attr_key(k) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("only user-defined attributes are writable: {k:?}"),
                ));
            }
            attrs.insert(k.to_string(), v.to_string());
        }
        return store::set_attrs(&id, &attrs);
    }

    let meta = store::get_meta(&id)?;
    if args.json {
        let value = if args.items.is_empty() {
            meta.to_json_value(args.preview)
        } else {
            let mut map = serde_json::Map::new();
            for key in &args.items {
                let Some(value) = attr_value(&meta, key, args.preview) else {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("attribute not found: {key}"),
                    ));
                };
                map.insert(key.clone(), serde_json::Value::String(value));
            }
            serde_json::Value::Object(map)
        };
        serde_json::to_writer_pretty(io::stdout(), &value).map_err(io::Error::other)?;
        println!();
        return Ok(());
    }

    if args.items.len() == 1 {
        let key = &args.items[0];
        if let Some(value) = attr_value(&meta, key, args.preview) {
            println!("{}", escape_attr_output(&value));
            return Ok(());
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "attribute not found",
        ));
    }

    if !args.items.is_empty() {
        for key in &args.items {
            let Some(value) = attr_value(&meta, key, args.preview) else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("attribute not found: {key}"),
                ));
            };
            println!("{}{}{}", key, args.separator, escape_attr_output(&value));
        }
        return Ok(());
    }

    println!("id{}{}", args.separator, meta.display_id());
    println!("ts{}{}", args.separator, meta.ts);
    println!("size{}{}", args.separator, meta.size);
    for (k, v) in &meta.attrs {
        println!("{}{}{}", k, args.separator, escape_attr_output(v));
    }
    if args.preview && !meta.preview.is_empty() {
        println!("preview{}{}", args.separator, escape_attr_output(&meta.preview));
    }
    Ok(())
}
