use std::{env, fs, process};

use gaeb_core::{
    Document, ExchangePhase, ParseOptions, SourceVersion, WriteOptions, XRechnungOptions,
    parse_bytes, validate,
};
use rust_decimal::Decimal;

fn main() {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        process::exit(2);
    };

    let result = match command.as_str() {
        "info" => info(args.collect()),
        "validate" => validate_command(args.collect()),
        "convert" => convert(args.collect()),
        "diff" => diff(args.collect()),
        "export" => export(args.collect()),
        "xrechnung" => xrechnung(args.collect()),
        _ => {
            usage();
            Err(format!("unknown command: {command}"))
        }
    };
    if let Err(error) = result {
        eprintln!("FAIL: {error}");
        process::exit(1);
    }
}

fn info(args: Vec<String>) -> Result<(), String> {
    let document = parse_file(
        args.first()
            .ok_or_else(|| "missing input file".to_owned())?,
        false,
    )?;
    println!("Version: {}", document.source_version().number());
    println!("Phase: {}", document.phase().code());
    println!("Kind: {:?}", document.kind());
    println!("Items: {}", document.item_count());
    println!("Total: {}", document.grand_total());
    Ok(())
}

fn validate_command(args: Vec<String>) -> Result<(), String> {
    let strict = args.iter().any(|arg| arg == "--strict");
    let path = args
        .iter()
        .find(|arg| arg.as_str() != "--strict")
        .ok_or_else(|| "missing input file".to_owned())?;
    let document = parse_file(path, strict)?;
    let issues = validate(&document, &Default::default());
    if issues.is_empty() {
        println!("OK - no issues found.");
        return Ok(());
    }
    for issue in &issues {
        println!("[{:?}] {}", issue.severity, issue.message);
    }
    if issues
        .iter()
        .any(|issue| issue.severity == gaeb_core::ValidationSeverity::Error)
    {
        return Err("validation failed".to_owned());
    }
    Ok(())
}

fn convert(args: Vec<String>) -> Result<(), String> {
    let input = args
        .first()
        .ok_or_else(|| "missing input file".to_owned())?;
    let output = args
        .get(1)
        .ok_or_else(|| "missing output file".to_owned())?;
    let version = option_value(&args, "--version")
        .map(parse_version)
        .transpose()?
        .unwrap_or(SourceVersion::DaXml33);
    let phase = option_value(&args, "--phase")
        .map(parse_phase)
        .transpose()?;
    let source = parse_file(input, false)?;
    let options = WriteOptions {
        version,
        phase: phase.unwrap_or(source.phase()),
        ..WriteOptions::default()
    };
    let mut bytes = Vec::new();
    let report = source
        .write(&mut bytes, &options)
        .map_err(|error| error.to_string())?;
    fs::write(output, bytes).map_err(|error| error.to_string())?;
    println!("Converted {} items", source.item_count());
    for warning in report.warnings {
        println!("Warning: {warning}");
    }
    for omission in report.omitted_fields {
        println!("Omitted: {omission}");
    }
    Ok(())
}

fn diff(args: Vec<String>) -> Result<(), String> {
    let left = parse_file(
        args.first()
            .ok_or_else(|| "missing first input file".to_owned())?,
        false,
    )?;
    let right = parse_file(
        args.get(1)
            .ok_or_else(|| "missing second input file".to_owned())?,
        false,
    )?;
    let result = gaeb_core::diff::compare(&left, &right);
    println!("Added: {}", result.added.len());
    println!("Removed: {}", result.removed.len());
    println!("Modified: {}", result.modified.len());
    println!(
        "Financial impact: {}",
        result.financial_impact.unwrap_or_default()
    );
    Ok(())
}

fn xrechnung(args: Vec<String>) -> Result<(), String> {
    let input = args
        .first()
        .ok_or_else(|| "missing input file".to_owned())?;
    let output = args
        .get(1)
        .ok_or_else(|| "missing output file".to_owned())?;
    let vat_rate = option_value(&args, "--vat-rate")
        .map(|value| Decimal::from_str_exact(&value).map_err(|error| error.to_string()))
        .transpose()?;
    let options = XRechnungOptions {
        invoice_number: option_value(&args, "--invoice-number"),
        issue_date: option_value(&args, "--issue-date"),
        seller_name: option_value(&args, "--seller-name"),
        buyer_name: option_value(&args, "--buyer-name"),
        leitweg_id: option_value(&args, "--leitweg-id"),
        vat_rate,
    };
    let document = parse_file(input, false)?;
    let content =
        gaeb_core::to_xrechnung_string(&document, &options).map_err(|error| error.to_string())?;
    fs::write(output, content).map_err(|error| error.to_string())
}

fn export(args: Vec<String>) -> Result<(), String> {
    let input = args
        .first()
        .ok_or_else(|| "missing input file".to_owned())?;
    let output = args
        .get(1)
        .ok_or_else(|| "missing output file".to_owned())?;
    let document = parse_file(input, false)?;
    let content = if output.ends_with(".csv") {
        gaeb_core::export::to_csv_string(&document, ',')
    } else {
        gaeb_core::export::to_json_string(&document, true).map_err(|error| error.to_string())?
    };
    fs::write(output, content).map_err(|error| error.to_string())
}

fn parse_file(path: &str, strict: bool) -> Result<Document, String> {
    let input = fs::read(path).map_err(|error| error.to_string())?;
    parse_bytes(
        &input,
        &ParseOptions {
            strict,
            ..ParseOptions::default()
        },
    )
    .map_err(|error| error.to_string())
}

fn option_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn parse_version(value: String) -> Result<SourceVersion, String> {
    match value.as_str() {
        "2.0" => Ok(SourceVersion::DaXml20),
        "2.1" => Ok(SourceVersion::DaXml21),
        "3.0" => Ok(SourceVersion::DaXml30),
        "3.1" => Ok(SourceVersion::DaXml31),
        "3.2" => Ok(SourceVersion::DaXml32),
        "3.3" => Ok(SourceVersion::DaXml33),
        "90" => Ok(SourceVersion::Gaeb90),
        _ => Err(format!("unknown version: {value}")),
    }
}

fn parse_phase(value: String) -> Result<ExchangePhase, String> {
    let normalized = value.trim().trim_start_matches(['D', 'd', 'X', 'x']);
    let code = format!("X{normalized}");
    [
        ExchangePhase::X31,
        ExchangePhase::X32,
        ExchangePhase::X33,
        ExchangePhase::X34,
        ExchangePhase::X35,
        ExchangePhase::X36,
        ExchangePhase::X37,
        ExchangePhase::X38,
        ExchangePhase::X39,
        ExchangePhase::X50,
        ExchangePhase::X51,
        ExchangePhase::X52,
        ExchangePhase::X60,
        ExchangePhase::X61,
        ExchangePhase::X62,
        ExchangePhase::X63,
        ExchangePhase::X64,
        ExchangePhase::X65,
        ExchangePhase::X66,
        ExchangePhase::X67,
        ExchangePhase::X68,
        ExchangePhase::X69,
        ExchangePhase::X80,
        ExchangePhase::X81,
        ExchangePhase::X82,
        ExchangePhase::X83,
        ExchangePhase::X84,
        ExchangePhase::X85,
        ExchangePhase::X86,
        ExchangePhase::X87,
        ExchangePhase::X88,
        ExchangePhase::X89,
        ExchangePhase::X89B,
        ExchangePhase::X83Z,
        ExchangePhase::X84Z,
        ExchangePhase::X86ZR,
        ExchangePhase::X86ZE,
        ExchangePhase::X93,
        ExchangePhase::X94,
        ExchangePhase::X95,
        ExchangePhase::X96,
        ExchangePhase::X97,
    ]
    .into_iter()
    .find(|phase| phase.code() == code)
    .ok_or_else(|| format!("unknown phase: {value}"))
}

fn usage() {
    eprintln!("Usage: gaeb <info|validate|convert|diff|export|xrechnung> ...");
}
