#![cfg(feature = "reference-data")]

use std::{fs, process::Command};

use gaeb_core::{ExchangePhase, ParseOptions, SourceVersion, WriteOptions, parse_bytes};

#[test]
fn official_33_corpus_roundtrips_against_phase_schemas() {
    let fixtures = [
        (
            "bvbs-33-ava-x81/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X81",
            ExchangePhase::X81,
            "gaeb-33-lv-2021-05",
            "GAEB_DA_XML_81_3.3_2021-05.xsd",
        ),
        (
            "bvbs-33-ava-x84/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X84",
            ExchangePhase::X84,
            "gaeb-33-lv-2021-05",
            "GAEB_DA_XML_84_3.3_2021-05.xsd",
        ),
        (
            "bvbs-33-ava-x86/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X86",
            ExchangePhase::X86,
            "gaeb-33-lv-2021-05",
            "GAEB_DA_XML_86_3.3_2021-05.xsd",
        ),
        (
            "bvbs-33-construction-x83/Pruefdatei GAEB DA XML 3.3 - Bauausfuehrung - V 04 04 2024.x83",
            ExchangePhase::X83,
            "gaeb-33-lv-2021-05",
            "GAEB_DA_XML_83_3.3_2021-05.xsd",
        ),
        (
            "bvbs-33-construction-x84/BVBS_Pruefdatei GAEB DA XML 3.3 - Bauausfuehrung - V 11 06 2021.x84",
            ExchangePhase::X84,
            "gaeb-33-lv-2021-05",
            "GAEB_DA_XML_84_3.3_2021-05.xsd",
        ),
        (
            "bvbs-33-quantity-x31/Pruefdatei GAEB DA XML 3.3 - Mengenermittlung - V 11 06 2021.x31",
            ExchangePhase::X31,
            "gaeb-33-quantity-2023-01",
            "GAEB_DA_XML_31_3.3_2023-01.xsd",
        ),
        (
            "bvbs-33-quantity-x86/Pruefdatei GAEB DA XML 3.3 - Mengenermittlung - V 11 06 2021.x86",
            ExchangePhase::X86,
            "gaeb-33-lv-2021-05",
            "GAEB_DA_XML_86_3.3_2021-05.xsd",
        ),
        (
            "bvbs-33-text-x81/GAEB Pruefdatei GAEB DA XML 3.3 - Texterstellung V 11 06 2021.x81",
            ExchangePhase::X81,
            "gaeb-33-lv-2021-05",
            "GAEB_DA_XML_81_3.3_2021-05.xsd",
        ),
        (
            "bvbs-33-text-x82/GAEB Pruefdatei GAEB DA XML 3.3 - Texterstellung V 11 06 2021.x82",
            ExchangePhase::X82,
            "gaeb-33-lv-2021-05",
            "GAEB_DA_XML_82_3.3_2021-05.xsd",
        ),
    ];
    for (relative, phase, schema_dir, schema) in fixtures {
        let path = format!(
            "{}/testdata/official/{relative}",
            env!("CARGO_MANIFEST_DIR")
        );
        assert!(
            std::path::Path::new(&path).exists(),
            "missing official fixture: {path}"
        );
        let input = fs::read(path).unwrap();
        assert_writer_valid_with_schema_dir(&input, phase, schema_dir, schema);
    }
}

#[test]
fn official_legacy_schemas_validate_matching_official_samples() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/official");
    assert_schema_valid(
        &format!("{root}/gaeb-31-2007-11-schemas-lv/GAEB_DA_XML.xsd"),
        &format!("{root}/gaeb-31-2007-11-samples/Musterdatei_3.1_2008-001.X81"),
    );
    for (phase, schema) in [
        ("83Z", "GAEB_DA_XML_83Z_3.2_2014-03.xsd"),
        ("84Z", "GAEB_DA_XML_84Z_3.2_2014-03.xsd"),
        ("86ZR", "GAEB_DA_XML_86ZR_3.2_2014-03.xsd"),
        ("86ZE", "GAEB_DA_XML_86ZE_3.2_2014-03.xsd"),
    ] {
        assert_schema_valid(
            &format!("{root}/gaeb-32-schemas-time-contract/Zeitvertrag/{schema}"),
            &format!(
                "{root}/gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/Beispiel_01_{phase}_3.2_2014-03-24.X{phase}"
            ),
        );
    }
}

#[test]
fn x89_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid_with_schema_dir(
        b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA89/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><Invoice><DP>89</DP></Invoice></GAEB>",
        ExchangePhase::X89,
        "gaeb-33-invoice-2021-05",
        "GAEB_DA_XML_89_3.3_2021-05.xsd",
    );
}

#[test]
fn x89b_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid_with_schema_dir(
        b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA89B/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><Invoice><DP>89B</DP></Invoice></GAEB>",
        ExchangePhase::X89B,
        "gaeb-33-invoice-2021-05",
        "GAEB_DA_XML_89B_3.3_2021-05.xsd",
    );
}

#[test]
fn x93_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid_with_schema_dir(
        b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA93/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><Order><DP>93</DP><OrderItem ID=\"A1\"><ArtNo>ART-1</ArtNo><Qty>2</Qty><QU>pc</QU><NetPrice>3.50</NetPrice></OrderItem></Order></GAEB>",
        ExchangePhase::X93,
        "gaeb-33-trade-2021-05",
        "GAEB_DA_XML_93_3.3_2021-05.xsd",
    );
}

#[test]
fn x94_writer_output_validates_against_official_gaeb_schema() {
    assert_trade_writer_valid(ExchangePhase::X94, "GAEB_DA_XML_94_3.3_2021-05.xsd");
}

#[test]
fn x96_writer_output_validates_against_official_gaeb_schema() {
    assert_trade_writer_valid(ExchangePhase::X96, "GAEB_DA_XML_96_3.3_2021-05.xsd");
}

#[test]
fn x97_writer_output_validates_against_official_gaeb_schema() {
    assert_trade_writer_valid(ExchangePhase::X97, "GAEB_DA_XML_97_3.3_2021-05.xsd");
}

#[test]
fn x50_official_schema_compiler_blocker_is_explicit() {
    assert_cost_schema_compiler_blocker("GAEB_DA_XML_50.2_3.3_2021-05.xsd");
}

#[test]
fn x51_official_schema_compiler_blocker_is_explicit() {
    assert_cost_schema_compiler_blocker("GAEB_DA_XML_51.2_3.3_2021-05.xsd");
}

#[test]
fn x52_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid_with_schema_dir(
        &reference_fixture(),
        ExchangePhase::X52,
        "gaeb-33-cost-2021-05",
        "GAEB_DA_XML_52_3.3_2021-05.xsd",
    );
}

#[test]
fn x83z_writer_output_validates_against_official_gaeb_schema() {
    assert_time_contract_writer_valid(
        "Beispiel_01_83Z_3.2_2014-03-24.X83Z",
        ExchangePhase::X83Z,
        "GAEB_DA_XML_83Z_3.3_2021-05.xsd",
    );
}

#[test]
fn x84z_writer_output_validates_against_official_gaeb_schema() {
    assert_time_contract_writer_valid(
        "Beispiel_01_84Z_3.2_2014-03-24.X84Z",
        ExchangePhase::X84Z,
        "GAEB_DA_XML_84Z_3.3_2021-05.xsd",
    );
}

#[test]
fn x86zr_writer_output_validates_against_official_gaeb_schema() {
    assert_time_contract_writer_valid(
        "Beispiel_01_86ZR_3.2_2014-03-24.X86ZR",
        ExchangePhase::X86ZR,
        "GAEB_DA_XML_86ZR_3.3_2021-05.xsd",
    );
}

#[test]
fn x86ze_writer_output_validates_against_official_gaeb_schema() {
    assert_time_contract_writer_valid(
        "Beispiel_01_86ZE_3.2_2014-03-24.X86ZE",
        ExchangePhase::X86ZE,
        "GAEB_DA_XML_86ZE_3.3_2021-05.xsd",
    );
}

#[test]
fn x80_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid(
        &reference_fixture(),
        ExchangePhase::X80,
        "GAEB_DA_XML_80_3.3_2021-05.xsd",
    );
}

#[test]
fn x81_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid(
        &fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/official/bvbs-33-ava-x81/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X81")).unwrap(),
        ExchangePhase::X81,
        "GAEB_DA_XML_81_3.3_2021-05.xsd",
    );
}

#[test]
fn x84_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid(
        &fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/official/bvbs-33-ava-x84/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X84")).unwrap(),
        ExchangePhase::X84,
        "GAEB_DA_XML_84_3.3_2021-05.xsd",
    );
}

#[test]
fn x85_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid(
        &reference_fixture(),
        ExchangePhase::X85,
        "GAEB_DA_XML_85_3.3_2021-05.xsd",
    );
}

#[test]
fn x86_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid(
        &fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/official/bvbs-33-ava-x86/BVBS_Pruefdatei GAEB DA XML 3.3 - AVA - V 11 06 2021.X86")).unwrap(),
        ExchangePhase::X86,
        "GAEB_DA_XML_86_3.3_2021-05.xsd",
    );
}

#[test]
fn x87_writer_output_validates_against_official_gaeb_schema() {
    assert_writer_valid(
        &reference_fixture(),
        ExchangePhase::X87,
        "GAEB_DA_XML_87_3.3_2021-05.xsd",
    );
}

fn reference_fixture() -> Vec<u8> {
    fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/reference/gaebxml.x83"
    ))
    .unwrap()
}

fn assert_schema_valid(schema: &str, input: &str) {
    let result = Command::new("xmllint")
        .args(["--noout", "--schema", schema, input])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn assert_time_contract_writer_valid(file: &str, phase: ExchangePhase, schema: &str) {
    let input = fs::read(format!(
        "{}/testdata/official/gaeb-32-time-contract-examples/Zeitvertrag - Beispieldateien/{file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    assert_writer_valid_with_schema_dir(&input, phase, "gaeb-33-time-contract-2021-05", schema);
}

fn assert_cost_schema_compiler_blocker(schema: &str) {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let schema_dir = format!("{manifest}/testdata/official/gaeb-33-cost-2021-05");
    let result = Command::new("xmllint")
        .current_dir(schema_dir)
        .args(["--noout", "--schema", schema, "/dev/null"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(
        stderr.contains("redefined") || stderr.contains("redefine"),
        "unexpected schema compiler result: {stderr}"
    );
}

fn assert_trade_writer_valid(phase: ExchangePhase, schema: &str) {
    let input = format!(
        "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA{}/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><Order><DP>{}</DP><OrderItem ID=\"A1\"><ArtNo>ART-1</ArtNo><Qty>2</Qty><QU>pc</QU><NetPrice>3.50</NetPrice></OrderItem></Order></GAEB>",
        phase.code().trim_start_matches('X'),
        phase.code().trim_start_matches('X'),
    );
    assert_writer_valid_with_schema_dir(input.as_bytes(), phase, "gaeb-33-trade-2021-05", schema);
}

fn assert_writer_valid(input: &[u8], phase: ExchangePhase, schema: &str) {
    assert_writer_valid_with_schema_dir(input, phase, "gaeb-33-lv-2021-05", schema);
}

fn assert_writer_valid_with_schema_dir(
    input: &[u8],
    phase: ExchangePhase,
    schema_dir_name: &str,
    schema: &str,
) {
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    let output = std::env::temp_dir().join(format!(
        "gaeb-core-{}-{}-{}.xml",
        std::process::id(),
        phase.code(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let mut file = fs::File::create(&output).unwrap();
    document
        .write(
            &mut file,
            &WriteOptions {
                version: SourceVersion::DaXml33,
                phase,
                indent: true,
            },
        )
        .unwrap();

    let manifest = env!("CARGO_MANIFEST_DIR");
    let schema_dir = format!("{manifest}/testdata/official/{schema_dir_name}");
    let result = Command::new("xmllint")
        .current_dir(&schema_dir)
        .args(["--noout", "--schema", schema, output.to_str().unwrap()])
        .output()
        .unwrap();
    eprintln!("writer output: {}", output.display());

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
