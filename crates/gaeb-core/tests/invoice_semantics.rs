use gaeb_core::{Document, ExchangePhase, ParseOptions, SourceVersion, WriteOptions, parse_bytes};

const X89: &[u8] = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA89/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Invoice><DP>89</DP><OWN><Address><Name1>Owner GmbH</Name1><Street>Main 1</Street><PCode>10115</PCode><City>Berlin</City><VATID>DE123</VATID><Bank><LblBank>Bank</LblBank><BRNo>100</BRNo><AcctNo>200</AcctNo><IBAN>DE00</IBAN></Bank></Address><AwardNo>A-7</AwardNo></OWN><Requester><Address><Name1>Requester</Name1><Street>Side 2</Street><PCode>10117</PCode><City>Berlin</City></Address><RequestIDNo>R-1</RequestIDNo><RequestName>Dept</RequestName></Requester><CTR><Address><Name1>Contractor</Name1><Street>Work 3</Street><PCode>10119</PCode><City>Berlin</City></Address><AcctsPayNo>C-9</AcctsPayNo></CTR><CnstSite><Address><Name1>Site</Name1><Street>Build 4</Street><PCode>10121</PCode><City>Berlin</City></Address><CnstSiteIDNo>S-1</CnstSiteIDNo><CnstSiteName>Building</CnstSiteName></CnstSite><AddText><OutlineAddText>Note heading</OutlineAddText><DetailAddText><p>Note body</p></DetailAddText></AddText><BoQ><BoQBody><BoQCtgy RNoPart="01"><LblTx>Work</LblTx><BoQBody><Itemlist><Item RNoPart="001"><Qty>2</Qty><QU>m</QU><UP>10</UP><IT>20</IT><Description><ShortText>Concrete</ShortText></Description></Item></Itemlist></BoQBody></BoQCtgy></BoQBody></BoQ><InvoiceHeader><InvoiceNo>INV-1</InvoiceNo><InvoiceDate>2026-01-02</InvoiceDate><InvoiceType>single invoice</InvoiceType><CreditNote>Yes</CreditNote><SettlementType>periodic</SettlementType><SequentialNo>2</SequentialNo><ServiceProvisionStartDate>2025-12-01</ServiceProvisionStartDate><ServiceProvisionEndDate>2025-12-31</ServiceProvisionEndDate></InvoiceHeader><InvoiceCreator><Address><Name1>Creator</Name1><Street>Office 5</Street><PCode>10123</PCode><City>Berlin</City></Address><TaxNo>T-1</TaxNo></InvoiceCreator><InvoiceRecipient><Address><Name1>Recipient</Name1><Street>Office 6</Street><PCode>10125</PCode><City>Berlin</City></Address></InvoiceRecipient><CheckNote><State>approved</State><CheckDate>2026-01-03</CheckDate><Annotation>Checked</Annotation></CheckNote><BudgetDept>B-1</BudgetDept><BookingNo>BK-1</BookingNo><QuantityDeterminationRemark>See attachment</QuantityDeterminationRemark><InvoiceShare><InvoiceShareType>basic amount</InvoiceShareType><Description>Net</Description><Total>20</Total></InvoiceShare><Terms><CashDiscDays Days="10" TypeOfDays="calendar days"/><DiscountPcnt>2.5</DiscountPcnt></Terms><PaymentMade><TotalVAT>3.8</TotalVAT><Total>20</Total><PaymentDate>2026-02-01</PaymentDate><InvoiceNo>INV-1</InvoiceNo><PaymentNo>P-1</PaymentNo></PaymentMade><InvoiceMadeOut><TotalVAT>3.8</TotalVAT><Total>20</Total><InvoiceDate>2026-01-02</InvoiceDate><InvoiceType>single invoice</InvoiceType><InvoiceNo>INV-1</InvoiceNo></InvoiceMadeOut><TotalGross>23.8</TotalGross></Invoice></GAEB>"#;

#[test]
fn x89_invoice_semantics_survive_round_trip() {
    let document = parse_bytes(X89, &ParseOptions::default()).unwrap();
    let Document::Procurement(document) = document else {
        panic!("procurement expected")
    };
    let invoice = document.invoice.as_ref().expect("invoice model");
    assert_eq!(
        invoice.owner.as_ref().unwrap().award_no.as_deref(),
        Some("A-7")
    );
    assert_eq!(invoice.requesters[0].request_name.as_deref(), Some("Dept"));
    assert_eq!(
        invoice
            .contractor
            .as_ref()
            .unwrap()
            .accounts_payable_no
            .as_deref(),
        Some("C-9")
    );
    assert_eq!(
        invoice.construction_sites[0].name.as_deref(),
        Some("Building")
    );
    assert_eq!(
        invoice.header.as_ref().unwrap().invoice_no.as_deref(),
        Some("INV-1")
    );
    assert_eq!(invoice.shares[0].total.unwrap().to_string(), "20");
    assert_eq!(invoice.total_gross.unwrap().to_string(), "23.8");

    let mut output = Vec::new();
    Document::Procurement(document)
        .write(
            &mut output,
            &WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X89,
                indent: false,
            },
        )
        .unwrap();
    let reparsed = parse_bytes(&output, &ParseOptions::default()).unwrap();
    let Document::Procurement(reparsed) = reparsed else {
        panic!("procurement expected")
    };
    let invoice = reparsed.invoice.unwrap();
    assert_eq!(invoice.header.unwrap().invoice_no.as_deref(), Some("INV-1"));
    assert_eq!(
        invoice.owner.unwrap().address.unwrap().banks[0]
            .iban
            .as_deref(),
        Some("DE00")
    );
    assert_eq!(invoice.total_gross.unwrap().to_string(), "23.8");
}

#[cfg(feature = "reference-data")]
#[test]
fn x89_writer_preserves_invoice_schema_order() {
    let document = parse_bytes(X89, &ParseOptions::default()).unwrap();
    let mut output = Vec::new();
    document
        .write(
            &mut output,
            &WriteOptions {
                version: SourceVersion::DaXml33,
                phase: ExchangePhase::X89,
                indent: true,
            },
        )
        .unwrap();
    let path = std::env::temp_dir().join(format!("gaeb-core-invoice-{}.xml", std::process::id()));
    std::fs::write(&path, output).unwrap();
    let schema_dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/official/gaeb-33-invoice-2021-05"
    );
    let result = std::process::Command::new("xmllint")
        .current_dir(schema_dir)
        .args([
            "--noout",
            "--schema",
            "GAEB_DA_XML_89_3.3_2021-05.xsd",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(path);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn x89b_invoice_information_has_optional_header_and_no_totals() {
    let input = br#"<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA89B/3.3"><GAEBInfo><Version>3.3</Version></GAEBInfo><Invoice><DP>89B</DP><InvoiceHeader><RefInvoiceNo>INV-1</RefInvoiceNo><ServiceProvisionStartDate>2026-01-01</ServiceProvisionStartDate><ServiceProvisionEndDate>2026-01-31</ServiceProvisionEndDate></InvoiceHeader><InvoiceCreator><Address><Name1>Creator</Name1><Street>Office</Street><PCode>1</PCode><City>Town</City></Address><TaxNo>T</TaxNo></InvoiceCreator></Invoice></GAEB>"#;
    let document = parse_bytes(input, &ParseOptions::default()).unwrap();
    let Document::Procurement(document) = document else {
        panic!("procurement expected")
    };
    assert_eq!(
        document
            .invoice
            .unwrap()
            .header
            .unwrap()
            .reference_invoice_no
            .as_deref(),
        Some("INV-1")
    );
}
