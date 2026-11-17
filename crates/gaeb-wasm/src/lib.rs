#![forbid(unsafe_code)]
//! Plain DTO WebAssembly adapter. Consumers receive JSON-compatible values,
//! never Rust handles. The npm wrapper owns WASM initialization.

use gaeb_core::{
    Document, ExchangePhase, ParseOptions, SourceVersion, ValidationOptions, WriteOptions,
    XRechnungOptions, parse_bytes, validate as validate_document,
};
use wasm_bindgen::prelude::*;

fn error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
pub fn parse(input: &[u8]) -> Result<JsValue, JsValue> {
    let document = parse_bytes(input, &ParseOptions::default()).map_err(error)?;
    serde_wasm_bindgen::to_value(&document).map_err(error)
}

#[wasm_bindgen]
pub fn validate(document: JsValue) -> Result<JsValue, JsValue> {
    let document: Document = serde_wasm_bindgen::from_value(document).map_err(error)?;
    serde_wasm_bindgen::to_value(&validate_document(&document, &ValidationOptions::default()))
        .map_err(error)
}

#[wasm_bindgen]
pub fn write(document: JsValue) -> Result<Vec<u8>, JsValue> {
    let document: Document = serde_wasm_bindgen::from_value(document).map_err(error)?;
    let options = WriteOptions {
        version: document.source_version(),
        phase: document.phase(),
        ..WriteOptions::default()
    };
    let mut output = Vec::new();
    document.write(&mut output, &options).map_err(error)?;
    Ok(output)
}

#[wasm_bindgen]
pub fn convert(input: &[u8], version: &str, phase: &str) -> Result<JsValue, JsValue> {
    let version = parse_version(version).map_err(error)?;
    let phase = parse_phase(phase).map_err(error)?;
    let (output, report) = gaeb_core::converter::convert_bytes(
        input,
        &ParseOptions::default(),
        &WriteOptions {
            version,
            phase,
            ..WriteOptions::default()
        },
    )
    .map_err(error)?;
    serde_wasm_bindgen::to_value(&(output, report)).map_err(error)
}

#[wasm_bindgen]
pub fn quality(document: JsValue) -> Result<JsValue, JsValue> {
    let document: Document = serde_wasm_bindgen::from_value(document).map_err(error)?;
    serde_wasm_bindgen::to_value(&gaeb_core::quality::quality_score(&document)).map_err(error)
}

#[wasm_bindgen]
pub fn normalize_units(document: JsValue) -> Result<JsValue, JsValue> {
    let mut document: Document = serde_wasm_bindgen::from_value(document).map_err(error)?;
    let changed = gaeb_core::units::normalize_units_in_document(&mut document);
    serde_wasm_bindgen::to_value(&(document, changed)).map_err(error)
}

#[wasm_bindgen]
pub fn diff(left: JsValue, right: JsValue) -> Result<JsValue, JsValue> {
    let left: Document = serde_wasm_bindgen::from_value(left).map_err(error)?;
    let right: Document = serde_wasm_bindgen::from_value(right).map_err(error)?;
    serde_wasm_bindgen::to_value(&gaeb_core::diff::compare(&left, &right)).map_err(error)
}

#[wasm_bindgen]
pub fn to_xrechnung(document: JsValue) -> Result<String, JsValue> {
    let document: Document = serde_wasm_bindgen::from_value(document).map_err(error)?;
    gaeb_core::to_xrechnung_string(&document, &XRechnungOptions::default()).map_err(error)
}

#[wasm_bindgen]
pub fn to_xrechnung_with_options(document: JsValue, options: JsValue) -> Result<String, JsValue> {
    let document: Document = serde_wasm_bindgen::from_value(document).map_err(error)?;
    let options: XRechnungOptions = serde_wasm_bindgen::from_value(options).map_err(error)?;
    gaeb_core::to_xrechnung_string(&document, &options).map_err(error)
}

#[wasm_bindgen]
pub fn tender_to_bid(document: JsValue) -> Result<JsValue, JsValue> {
    let document: Document = serde_wasm_bindgen::from_value(document).map_err(error)?;
    let result = gaeb_core::transition::tender_to_bid(&document).map_err(error)?;
    serde_wasm_bindgen::to_value(&result).map_err(error)
}

fn parse_version(value: &str) -> Result<SourceVersion, String> {
    match value.trim() {
        "2.0" => Ok(SourceVersion::DaXml20),
        "2.1" => Ok(SourceVersion::DaXml21),
        "3.0" => Ok(SourceVersion::DaXml30),
        "3.1" => Ok(SourceVersion::DaXml31),
        "3.2" => Ok(SourceVersion::DaXml32),
        "3.3" => Ok(SourceVersion::DaXml33),
        "90" => Ok(SourceVersion::Gaeb90),
        value => Err(format!("unknown version: {value}")),
    }
}

fn parse_phase(value: &str) -> Result<ExchangePhase, String> {
    let value = value.trim().trim_start_matches(['D', 'd', 'X', 'x']);
    let code = format!("X{value}");
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
