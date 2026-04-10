use clap::{ArgAction, Args};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::store;

#[derive(Args, Debug, Clone, Default)]
pub(crate) struct PushArgs {
    #[arg(short = 'a', long = "attr", value_name = "key=value", action = ArgAction::Append, help = "Set attribute key=value (repeatable)")]
    pub(crate) attr: Vec<String>,

    #[arg(long, default_value = "null", help = "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0")]
    pub(crate) print: String,

    #[arg(help = "Optional file to stash; reads stdin when omitted")]
    pub(crate) file: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct TeeArgs {
    #[arg(short = 'a', long = "attr", value_name = "key=value", action = ArgAction::Append, help = "Set attribute key=value (repeatable)")]
    pub(crate) attr: Vec<String>,

    #[arg(long, default_value = "null", help = "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0")]
    pub(crate) print: String,

    #[arg(long, num_args = 0..=1, default_value_t = true, default_missing_value = "true", help = "Save captured input when an upstream or processing error happens: true or false")]
    pub(crate) save_on_error: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PrintTarget {
    Stdout,
    Stderr,
    None,
}

fn parse_print_target(value: &str) -> io::Result<PrintTarget> {
    match value {
        "stdout" | "1" => Ok(PrintTarget::Stdout),
        "stderr" | "2" => Ok(PrintTarget::Stderr),
        "null" | "0" => Ok(PrintTarget::None),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--print must be stdout, stderr, null, 1, 2, or 0",
        )),
    }
}

fn emit_generated_id(
    target: PrintTarget,
    id: &str,
    stdout: Option<&mut dyn Write>,
) -> io::Result<()> {
    match target {
        PrintTarget::Stdout => {
            if let Some(out) = stdout {
                writeln!(out, "{id}")?;
            } else {
                println!("{id}");
            }
        }
        PrintTarget::Stderr => {
            eprintln!("{id}");
        }
        PrintTarget::None => {}
    }
    Ok(())
}

fn parse_meta_flags(values: &[String]) -> io::Result<BTreeMap<String, String>> {
    let mut attrs = BTreeMap::new();
    for value in values {
        let Some((k, v)) = value.split_once('=') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "attribute must be key=value",
            ));
        };
        attrs.insert(k.to_string(), v.to_string());
    }
    Ok(attrs)
}

pub(super) fn push_command(args: PushArgs) -> io::Result<()> {
    let mut attrs = parse_meta_flags(&args.attr)?;
    let print_target = parse_print_target(&args.print)?;
    let id = if let Some(path) = args.file {
        let mut file = File::open(&path)?;
        store::add_filename_attr(&path, &mut attrs);
        store::push_from_reader(&mut file, attrs)?
    } else {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        store::push_from_reader(&mut input, attrs)?
    };
    emit_generated_id(print_target, &id, None)?;
    Ok(())
}

pub(super) fn tee_command(args: TeeArgs) -> io::Result<()> {
    let attrs = parse_meta_flags(&args.attr)?;
    let print_target = parse_print_target(&args.print)?;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match store::tee_from_reader_partial(&mut input, &mut out, attrs, args.save_on_error) {
        Ok(id) => {
            emit_generated_id(print_target, &id, Some(&mut out))?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}
