use gaeb_core::{
    Document, DocumentKind, ExchangePhase, ParseOptions, SourceVersion, WriteOptions, parse_bytes,
};

#[test]
fn x32_to_x39_and_x60_to_x69_preserve_phase_specific_trees() {
    let phases = [
        ExchangePhase::X32,
        ExchangePhase::X33,
        ExchangePhase::X34,
        ExchangePhase::X35,
        ExchangePhase::X36,
        ExchangePhase::X37,
        ExchangePhase::X38,
        ExchangePhase::X39,
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
    ];
    for phase in phases {
        let input = format!(
            "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA{}/3.3\"><GAEBInfo><Version>3.3</Version><DP>{}</DP></GAEBInfo><PhaseData ID=\"{}\"><Value>retained</Value><Nested flag=\"yes\">text</Nested></PhaseData></GAEB>",
            phase.code().trim_start_matches('X'),
            phase.code(),
            phase.code()
        );
        let document = parse_bytes(input.as_bytes(), &ParseOptions::default()).unwrap();
        assert_eq!(document.phase(), phase);
        assert_eq!(document.kind(), DocumentKind::Structured);
        let mut output = Vec::new();
        document
            .write(
                &mut output,
                &WriteOptions {
                    version: SourceVersion::DaXml33,
                    phase,
                    indent: false,
                },
            )
            .unwrap();
        let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
        let Document::Structured(reparsed) = reparsed else {
            panic!("structured document expected")
        };
        assert_eq!(reparsed.elements[0].attributes["ID"], phase.code());
        assert_eq!(reparsed.elements[0].children[1].text, "text");
    }
}
