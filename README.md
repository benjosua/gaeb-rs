# gaeb-rs

## Instructions for coding agents

- Start with this README and the manifests. `crates/gaeb-core` owns the domain model and codec; `crates/gaeb-wasm` is only a JavaScript adapter. The `gaeb` binary owns filesystem access.
- Keep parsing, validation, conversion, and writing usable without a filesystem, network, browser, or service. Do not introduce provider calls or credentials.
- Preserve decimal arithmetic for quantities, prices, tax, and totals. Do not replace `rust_decimal::Decimal` with floating-point arithmetic.
- Treat document bytes and JavaScript values as untrusted input. Keep parser limits and DTD/entity rejection intact. Never fetch external XML resources.
- Specify the target version and exchange phase when writing. Preserve `WriteReport` warnings and omissions; never hide conversion data loss.
- Keep downloaded samples, customer documents, schemas, archives, and generated exports in ignored directories. Never force-add reference data. Synthetic test inputs belong inline in Rust tests.
- Add self-contained regression tests for behavior changes. Gate tests needing external corpora or `xmllint` behind `reference-data`; default tests must pass on a clean clone.
- Pin direct dependencies to exact versions, commit `Cargo.lock`, and review dependency changes. Keep unsafe Rust forbidden.
- Preserve the PolyForm Noncommercial license and required notice. Do not describe this project as OSI open source or grant commercial rights.
- Before handing off, run the checks in [Development](#development). Do not claim external schema conformance, certification, or browser runtime verification from synthetic or native-only tests.

## Overview

A source-available Rust library and command-line tool for GAEB construction data exchange, with WebAssembly bindings for JavaScript applications.

**Licensing:** [PolyForm Noncommercial 1.0.0](LICENSE). For commercial use outside that license's permitted purposes, contact [benjosua](https://github.com/benjosua) for a separate commercial agreement.

GAEB is used in German construction workflows to exchange bills of quantities, tenders, bids, contracts, quantities, costs, invoices, and trade orders. AVA means *Ausschreibung, Vergabe und Abrechnung*: tendering, contract award, and billing. An OZ (*Ordnungszahl*) identifies a position within a bill of quantities.

`gaeb-rs` parses documents into a typed, serializable domain model, validates them, and writes an explicit target format. It also provides builders, document comparisons, bid analysis, tree navigation, unit normalization, and exports. It runs locally without a server, database, or external language runtime.

**Status:** version `0.1.0`, under development. Format recognition is not a promise of complete schema coverage or lossless conversion. Always inspect write/conversion reports and validate business-critical output independently.

## Workspace

| Component | Purpose |
| --- | --- |
| `crates/gaeb-core` | Domain types, parser, writer, validation, and document utilities |
| `crates/gaeb-core/src/bin/gaeb.rs` | Native `gaeb` CLI |
| `crates/gaeb-wasm` | `wasm-bindgen` adapter exposing serializable JavaScript values |

The core accepts byte slices and writes to any `std::io::Write` sink. The CLI handles files. The WASM adapter handles JavaScript serialization. Both crates forbid unsafe Rust.

## Formats and document kinds

Recognized source versions are GAEB DA XML **2.0, 2.1, 3.0, 3.1, 3.2, and 3.3**, plus **GAEB 90** fixed-width records. XML input accepts UTF-8, with a Windows-1252 fallback; GAEB 90 uses CP850 text handling.

| Domain model | Exchange phases represented |
| --- | --- |
| `Document::Procurement` | X52, X80-X89, X89B, X83Z, X84Z, X86ZR, X86ZE |
| `Document::Trade` | X93, X94, X95, X96, X97 |
| `Document::Cost` | X50, X51 |
| `Document::Quantity` | X31 |
| `Document::Structured` | X32-X39, X60-X69 |

These are model capabilities, not a declaration that every phase exists in every standard version:

- **X88 is historical** and is rejected for DA XML 3.3 output.
- **X95 is not a released phase**. Its model representation is retained, but validation/writing rejects it.
- Structured phases retain phase-specific XML trees rather than exposing a fully interpreted item model.
- GAEB 90 support covers implemented procurement record shapes, not every legacy vendor extension.
- Cross-version and cross-phase writes can omit fields. `WriteReport.omitted_fields` and conversion reports make those omissions visible.
- Round trips preserve supported semantics, not byte identity, original formatting, or every vendor extension. Unsupported elements in modeled item structures may be rejected rather than silently discarded.

## Getting started

Use a recent stable Rust toolchain with Rust 2024 edition support. Native builds use Cargo only; no reference datasets or external XML tools are needed for the default test suite.

```sh
git clone https://github.com/benjosua/gaeb-rs.git
cd gaeb-rs
cargo build --workspace --locked
cargo test --workspace --locked
```

Install the CLI from the checkout:

```sh
cargo install --path crates/gaeb-core --locked
```

Use the library from another Rust project through a local path:

```toml
[dependencies]
gaeb-core = { path = "../gaeb-rs/crates/gaeb-core" }
```

No crates.io publication is assumed.

## Rust usage

### Parse, validate, and write

```rust
use gaeb_core::{
    ExchangePhase, ParseOptions, SourceVersion, ValidationOptions,
    WriteOptions, parse_bytes, validate,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ><BoQBody><Itemlist><Item RNoPart="001"><Qty>2</Qty><QU>m</QU><Description><ShortText>Concrete edge</ShortText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"#;
    let document = parse_bytes(input, &ParseOptions::default())?;

    println!("{}: {} items", document.phase().code(), document.item_count());
    for issue in validate(&document, &ValidationOptions::default()) {
        eprintln!("{}: {}", issue.code, issue.message);
    }

    let mut output = Vec::new();
    let report = document.write(
        &mut output,
        &WriteOptions {
            version: SourceVersion::DaXml33,
            phase: ExchangePhase::X83,
            indent: true,
        },
    )?;
    assert!(report.omitted_fields.is_empty());
    for warning in report.warnings {
        eprintln!("{warning}");
    }
    Ok(())
}
```

`Document` is a tagged enum. Match its variant to access procurement positions, trade orders, cost elements, or quantity-specific fields. `document.items()` provides a common read-only iterator; `document.meta()` exposes version, phase, provenance fields, and parser warnings. Serde represents documents as `{ "kind": "procurement", "document": ... }` or the corresponding other kind.

### Build a bill of quantities

Add `rust_decimal = "=1.43.0"` to your application's dependencies for this example.

```rust
use gaeb_core::BoqBuilder;
use rust_decimal::Decimal;

fn main() -> Result<(), gaeb_core::GaebError> {
    let mut builder = BoqBuilder::new();
    builder.project_name("School renovation").currency("EUR");
    builder
        .add_category("01", "Concrete")?
        .add_item("Foundation")
        .quantity(Decimal::from(120))
        .unit("m3");

    let document = builder.build()?;
    assert_eq!(document.item_count(), 1);
    Ok(())
}
```

`BoqBuilder::with_profile` selects another phase/version. Builders support lots, nested categories, explicit or generated position identifiers, prices, attachments, and descriptions. `build_strict()` turns supported builder warnings into errors.

### Conversion and utility APIs

| Module | Main functionality |
| --- | --- |
| `converter` | `convert_bytes` returns output plus source/target metadata, warnings, and `fields_dropped` |
| `api` | `DocumentApi` queries, item lookup, summaries, and `ItemIndex` duplicate detection |
| `tree` | `BoqTree` navigation, categories, and full-OZ item lookup |
| `diff` | Field/structure comparisons, item changes, duplicate identifiers, significance, and financial impact |
| `bid_analysis` | Compare X84 bids or embedded X82 bidder prices, rankings, and price spreads |
| `transition` | X83 tender to X84 bid, X86 contract to historical X88 addendum or X89 invoice |
| `units` | Unit normalization, including document-wide normalization |
| `quality` | Heuristic completeness, precision, and structure scores; not a compliance certificate |
| `reb` | Parse quantity-takeoff row information |
| `cache` | `CacheBackend` interface and bounded in-memory cache |
| `export` | Document JSON/CSV and diff HTML/spreadsheet-oriented CSV |
| `xrechnung` | X89/X89B invoice mapping to UBL 2.1 XML |

Transitions prepare domain objects, not automatically submission-ready documents. In particular, historical X88 output needs a compatible target version, and an invoice needs its required invoice and party details.

XRechnung export currently emits an XRechnung 2.0 customization identifier. It is not a guarantee of compliance with current portal requirements. Validate output against the target XRechnung schemas and Schematron rules before submission.

## Command-line interface

```sh
gaeb info tender.x83
gaeb validate tender.x83
gaeb validate tender.x83 --strict
gaeb convert tender.x83 converted.x83 --version 3.3 --phase X83
gaeb diff original.x83 revised.x83
gaeb export tender.x83 positions.json
gaeb export tender.x83 positions.csv
gaeb xrechnung invoice.x89 invoice.xml \
  --invoice-number INV-001 --issue-date 2026-01-01 \
  --seller-name "Builder GmbH" --buyer-name "Customer" \
  --leitweg-id "BUYER-REFERENCE" --vat-rate 19
```

- `info` prints version, phase, document kind, item count, and total.
- `validate` prints semantic issues; errors produce a nonzero exit status. `--strict` also requests strict parsing. It does not run an external XSD validator.
- `convert` defaults to XML 3.3 and the input phase. Version values: `2.0`, `2.1`, `3.0`, `3.1`, `3.2`, `3.3`, `90`. Conversion warnings and omitted fields are printed.
- `diff` prints added, removed, and modified counts plus financial impact.
- `export` selects CSV for an output ending in `.csv`; otherwise it writes JSON.
- `xrechnung` requires an X89/X89B procurement invoice; its metadata options override mapped values.

Output commands write to the requested path and may overwrite existing files. Missing commands exit with status 2; command failures exit with status 1. Output format is selected by command/options, not inferred from the output extension except for `export`.

## WebAssembly / JavaScript

Build the WASM module:

```sh
rustup target add wasm32-unknown-unknown
cargo build -p gaeb-wasm --target wasm32-unknown-unknown --release --locked
```

Generate the browser glue and TypeScript declarations with a matching `wasm-bindgen-cli`:

```sh
cargo install wasm-bindgen-cli --version 0.2.128 --locked
wasm-bindgen target/wasm32-unknown-unknown/release/gaeb_wasm.wasm \
  --target web --out-dir crates/gaeb-wasm/pkg
```

```js
import init, { parse, validate, convert } from "./pkg/gaeb_wasm.js";

await init();
const bytes = new Uint8Array(await file.arrayBuffer()); // Browser File input.
const document = parse(bytes);
const issues = validate(document);
const [outputBytes, report] = convert(bytes, "3.3", "X83");
console.log(issues, report);
```

Exports: `parse`, `validate`, `write`, `convert`, `diff`, `quality`, `normalize_units`, `tender_to_bid`, `to_xrechnung`, and `to_xrechnung_with_options`.

- Functions exchange Serde values, not persistent Rust handles. Maps can arrive as JavaScript `Map` values; decimal values are represented by Serde's decimal encoding rather than a financial floating-point API.
- `write(document)` uses the document's source version and phase and returns bytes, but does not expose its write report. Use `convert(bytes, version, phase)` when warnings and field-loss reporting matter.
- `convert` returns an output/report pair. `normalize_units` returns the updated document and changed-unit count.
- Failures propagate as JavaScript exceptions. Functions are synchronous; large documents should be processed in a worker when UI responsiveness matters.
- Generated glue, declarations, packages, and WASM binaries are build artifacts and are not committed. There is no bundled npm wrapper.

## Validation and input safety

Parsing and semantic validation are separate operations. `ParseOptions::strict` promotes semantic validation errors to parse errors. `ValidationOptions` can enable total checks and stricter phase profiles, or suppress selected issue codes/patterns. Defaults leave `check_totals` and `strict_phase_profiles` disabled.

Default parser limits:

| Limit | Default |
| --- | --- |
| Input bytes | 32 MiB |
| XML nesting | 128 |
| Text event bytes | 1 MiB |
| Attachment text event bytes | 10 MiB, also subject to the general text-event limit |
| Modeled items | 1,000,000 |

DTD and entity declarations are rejected. The codec does not resolve XML resources over the network. Limits are defensive controls, not a security certification or a streaming-memory guarantee: the parser builds an in-memory representation. Embedding applications should apply their own upload, resource, and authorization controls.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo check -p gaeb-wasm --target wasm32-unknown-unknown --locked
cargo doc --workspace --no-deps --locked
```

The default tests are self-contained Rust unit and integration tests with inline synthetic inputs. They cover builders, supported round trips, comparison, decimal arithmetic, semantic validation, unsupported structures, and parser safety. Native checks do not exercise JavaScript runtime behavior.

### Optional reference-data tests

No downloaded sample documents, schemas, archives, or customer data are tracked. `.gitignore` excludes reference-data directories and common GAEB/XML/archive/export formats.

The `reference-data` feature enables additional corpus, cross-version conversion, and XSD tests. To run them, independently obtain the appropriate GAEB/BVBS files and preserve the paths named in the tests:

```text
crates/gaeb-core/testdata/
  official/                 # GAEB schemas, historical samples, BVBS test files
  reference/gaebxml.x83      # Optional six-position XML 3.2 X83 reference input
```

Official sources: [GAEB downloads](https://www.gaeb.de/) and [BVBS certification test files](https://www.bvbs.de/zertifizierungen/). Check their usage terms. There is no automatic download step. Exact filenames and schema releases are listed in the Rust test sources; a different document is not an interchangeable test fixture.

Install `xmllint` from libxml2 and supply the full corpus before running:

```sh
cargo test --workspace --features reference-data --locked
```

With that feature enabled, missing inputs or external tools are test failures, not silently skipped checks. Tests explicitly record known X50/X51 schema compiler blockers; these are not passing conformance claims. GAEB 90, generic structured phases, and absent corpus profiles are not covered by a blanket XSD conformance guarantee. Use of BVBS test files does not mean BVBS certification.

## License and commercial use

Required Notice: Copyright (c) 2026 benjosua (https://github.com/benjosua).

Licensed under the [PolyForm Noncommercial License 1.0.0](LICENSE). This is **source-available, not OSI-approved open source**. The license permits noncommercial use, modification, and redistribution, including its stated personal-use and noncommercial-organization allowances. Redistributors must preserve the license terms or their URL and the required notice.

Commercial use outside those permitted purposes requires a separate written commercial license. Contact [benjosua](https://github.com/benjosua) before using the software in commercial products, paid services, or other commercial applications not covered by the license. No commercial rights are granted merely by access to this repository.

The full license text controls. Dependency licenses remain their own.
