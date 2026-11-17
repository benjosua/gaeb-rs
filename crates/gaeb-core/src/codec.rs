use std::{collections::BTreeMap, io::Write};

use encoding_rs::WINDOWS_1252;
use quick_xml::{
    Reader, Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use rust_decimal::Decimal;

use crate::{
    Boq, BoqCategory, CostDocument, CostItem, Document, DocumentMeta, ExchangePhase, GaebError,
    InvoiceAddress, InvoiceBank, InvoiceCheckNote, InvoiceDocument, InvoiceHeader, InvoiceIssued,
    InvoiceParty, InvoicePayment, InvoicePaymentTerm, InvoiceShare, InvoiceSite, InvoiceText, Item,
    ItemType, Lot, ParseOptions, ProcurementDocument, ProjectInfo, QuantityAttachment,
    QuantityCatalog, QuantityCategory, QuantityDeterminationItem, QuantityDocument, QuantityItem,
    SourceVersion, StandardService, StructuredDocument, SubDescription, TextComplement,
    TradeAddress, TradeDocument, TradeItem, TradeParty, WriteOptions, WriteReport, XmlElement,
};

#[derive(Clone, Debug, Default)]
struct Node {
    name: String,
    attributes: BTreeMap<String, String>,
    text: String,
    children: Vec<Self>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|child| child.name == name)
    }
    fn children_named(&self, name: &str) -> impl Iterator<Item = &Self> {
        self.children.iter().filter(move |child| child.name == name)
    }
    fn descendant(&self, name: &str) -> Option<&Self> {
        self.child(name).or_else(|| {
            self.children
                .iter()
                .find_map(|child| child.descendant(name))
        })
    }
    fn descendants<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = &'a Self> + 'a> {
        Box::new(self.children.iter().flat_map(move |child| {
            std::iter::once(child)
                .filter(move |node| node.name == name)
                .chain(child.descendants(name))
        }))
    }
    fn value(&self, name: &str) -> Option<String> {
        self.child(name)
            .map(Self::all_text)
            .filter(|value| !value.is_empty())
    }
    fn all_text(&self) -> String {
        let mut text = self.text.trim().to_owned();
        for child in &self.children {
            let child_text = child.all_text();
            if !child_text.is_empty() {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&child_text);
            }
        }
        text
    }
    fn text_without(&self, excluded: &str) -> String {
        if self.name == excluded {
            return String::new();
        }
        let mut text = self.text.trim().to_owned();
        for child in &self.children {
            let child_text = child.text_without(excluded);
            if !child_text.is_empty() {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&child_text);
            }
        }
        text
    }
    fn attr(&self, name: &str) -> Option<String> {
        self.attributes.get(name).cloned()
    }
}

pub(crate) fn parse_bytes(input: &[u8], options: &ParseOptions) -> Result<Document, GaebError> {
    if input.len() > options.max_input_bytes {
        return Err(GaebError::InputTooLarge {
            limit: options.max_input_bytes,
        });
    }
    let input = input.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(input);
    if !looks_like_xml(input) {
        return parse_gaeb90(&decode_gaeb90(input), options);
    }
    let upper = String::from_utf8_lossy(input).to_ascii_uppercase();
    if upper.contains("<!DOCTYPE") || upper.contains("<!ENTITY") {
        return Err(GaebError::ExternalResource);
    }
    let root = read_xml(&decode(input), options)?;
    if root.name != "GAEB" {
        return Err(GaebError::Unsupported(format!(
            "expected GAEB root, found {}",
            root.name
        )));
    }
    let document = parse_xml_document(&root, options)?;
    if options.strict
        && let Some(issue) = crate::validate(&document, &Default::default())
            .into_iter()
            .find(|issue| issue.severity == crate::ValidationSeverity::Error)
    {
        return Err(GaebError::Validation(issue.message));
    }
    Ok(document)
}

fn looks_like_xml(input: &[u8]) -> bool {
    input
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'<')
}
fn decode(input: &[u8]) -> String {
    String::from_utf8(input.to_vec()).unwrap_or_else(|_| WINDOWS_1252.decode(input).0.into_owned())
}

fn read_xml(input: &str, options: &ParseOptions) -> Result<Node, GaebError> {
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                if stack.len() >= options.max_nesting {
                    return Err(GaebError::NestingLimit {
                        limit: options.max_nesting,
                    });
                }
                stack.push(node_from_start(&event, reader.decoder())?);
            }
            Ok(Event::Empty(event)) => {
                let node = node_from_start(&event, reader.decoder())?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root = Some(node);
                }
            }
            Ok(Event::Text(event)) => {
                let text = event
                    .xml_content()
                    .map_err(|error| GaebError::Xml(error.to_string()))?;
                enforce_text_limits(&stack, text.len(), options)?;
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&text);
                }
            }
            Ok(Event::CData(event)) => {
                let text = event
                    .xml_content()
                    .map_err(|error| GaebError::Xml(error.to_string()))?;
                enforce_text_limits(&stack, text.len(), options)?;
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&text);
                }
            }
            Ok(Event::GeneralRef(event)) => {
                let reference = event
                    .decode()
                    .map_err(|error| GaebError::Xml(error.to_string()))?;
                let escaped = format!("&{reference};");
                let text = quick_xml::escape::unescape(&escaped)
                    .map_err(|error| GaebError::Xml(error.to_string()))?;
                enforce_text_limits(&stack, text.len(), options)?;
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&text);
                }
            }
            Ok(Event::End(_)) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| GaebError::Xml("unexpected closing tag".to_owned()))?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.replace(node).is_some() {
                    return Err(GaebError::Xml("multiple root elements".to_owned()));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(GaebError::Xml(error.to_string())),
        }
    }
    if !stack.is_empty() {
        return Err(GaebError::Xml("unclosed XML element".to_owned()));
    }
    root.ok_or_else(|| GaebError::Xml("empty document".to_owned()))
}

fn enforce_text_limits(
    stack: &[Node],
    length: usize,
    options: &ParseOptions,
) -> Result<(), GaebError> {
    if length > options.max_text_bytes {
        return Err(GaebError::TextLimit {
            limit: options.max_text_bytes,
        });
    }
    if stack
        .last()
        .is_some_and(|node| matches!(node.name.as_str(), "Data" | "BinaryData" | "Attachment"))
        && length > options.max_attachment_bytes
    {
        return Err(GaebError::TextLimit {
            limit: options.max_attachment_bytes,
        });
    }
    Ok(())
}

fn node_from_start(
    event: &BytesStart<'_>,
    decoder: quick_xml::encoding::Decoder,
) -> Result<Node, GaebError> {
    let mut attributes = BTreeMap::new();
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| GaebError::Xml(error.to_string()))?;
        let key = canonical_name(&String::from_utf8_lossy(attribute.key.as_ref()));
        let value = attribute
            .decode_and_unescape_value(decoder)
            .map_err(|error| GaebError::Xml(error.to_string()))?
            .into_owned();
        attributes.insert(key, value);
    }
    Ok(Node {
        name: canonical_name(&String::from_utf8_lossy(event.name().as_ref())),
        attributes,
        ..Node::default()
    })
}

fn canonical_name(name: &str) -> String {
    let local = name.rsplit(':').next().unwrap_or(name);
    match local {
        "Vergabe" => "Award",
        "VergabeInfo" => "AwardInfo",
        "Leistungsverzeichnis" => "BoQ",
        "LVInfo" => "BoQInfo",
        "LVBereich" => "BoQBody",
        "LVGruppe" => "BoQCtgy",
        "Positionsliste" => "Itemlist",
        "Position" => "Item",
        "OZ" => "RNoPart",
        "Kurztext" => "ShortText",
        "Langtext" => "LongText",
        "Menge" => "Qty",
        "Mengeneinheit" => "QU",
        "Einheitspreis" => "UP",
        "Gesamtbetrag" => "IT",
        "Projekt" => "Prj",
        "ProjektName" => "PrjName",
        "Waehrung" => "Cur",
        "Auftraggeber" => "OWN",
        "Bezeichnung" => "LblTx",
        "Datum" => "DP",
        other => other,
    }
    .to_owned()
}

fn parse_xml_document(root: &Node, options: &ParseOptions) -> Result<Document, GaebError> {
    let version = detect_version(root);
    let phase = detect_phase(root).unwrap_or_default();
    let meta = DocumentMeta {
        version,
        phase,
        namespace: root.attr("xmlns"),
        schema_release: Some(schema_release(version, phase).to_owned()),
        date: root
            .descendant("GAEBInfo")
            .and_then(|info| info.value("Date")),
        time: root
            .descendant("GAEBInfo")
            .and_then(|info| info.value("Time")),
        program: root
            .descendant("GAEBInfo")
            .and_then(|info| info.value("ProgSystem")),
        warnings: Vec::new(),
    };
    let document = match phase.kind() {
        crate::DocumentKind::Procurement => {
            Document::Procurement(parse_procurement(root, meta, options)?)
        }
        crate::DocumentKind::Trade => Document::Trade(parse_trade(root, meta, options)?),
        crate::DocumentKind::Cost => Document::Cost(parse_cost(root, meta, options)?),
        crate::DocumentKind::Quantity => Document::Quantity(parse_quantity(root, meta, options)?),
        crate::DocumentKind::Structured => Document::Structured(parse_structured(root, meta)),
    };
    Ok(document)
}

fn detect_version(root: &Node) -> SourceVersion {
    if let Some(version) = root
        .descendant("GAEBInfo")
        .and_then(|info| info.value("Version"))
        .and_then(|version| SourceVersion::from_number(&version))
    {
        return version;
    }
    let namespace = root.attr("xmlns").unwrap_or_default();
    if namespace.contains("200407") {
        SourceVersion::DaXml20
    } else if namespace.contains("200511") {
        SourceVersion::DaXml21
    } else if namespace.contains("/3.0") {
        SourceVersion::DaXml30
    } else if namespace.contains("/3.1") {
        SourceVersion::DaXml31
    } else if namespace.contains("/3.2") {
        SourceVersion::DaXml32
    } else {
        SourceVersion::DaXml33
    }
}
fn detect_phase(root: &Node) -> Option<ExchangePhase> {
    root.descendant("DP")
        .and_then(|node| ExchangePhase::from_dp(&node.all_text()))
        .or_else(|| {
            root.attr("xmlns").and_then(|namespace| {
                namespace
                    .split('/')
                    .find(|part| part.starts_with("DA"))
                    .and_then(ExchangePhase::from_dp)
            })
        })
}
fn schema_release(version: SourceVersion, phase: ExchangePhase) -> &'static str {
    if version == SourceVersion::DaXml33 && phase == ExchangePhase::X31 {
        "2023-01"
    } else if version == SourceVersion::DaXml33 {
        "2021-05"
    } else {
        "legacy"
    }
}

fn parse_procurement(
    root: &Node,
    meta: DocumentMeta,
    options: &ParseOptions,
) -> Result<ProcurementDocument, GaebError> {
    let award = root
        .descendant("Award")
        .or_else(|| root.descendant("Invoice"))
        .ok_or_else(|| {
            GaebError::Unsupported("procurement phase has no Award or Invoice".to_owned())
        })?;
    let project_node = root.child("PrjInfo").or_else(|| award.child("AwardInfo"));
    let project = ProjectInfo {
        id: project_node.and_then(|node| node.value("PrjID").or_else(|| node.value("Prj"))),
        name: project_node.and_then(|node| node.value("NamePrj").or_else(|| node.value("PrjName"))),
        label: project_node.and_then(|node| node.value("LblPrj")),
        currency: project_node
            .and_then(|node| node.value("Cur"))
            .or_else(|| award.child("AwardInfo").and_then(|node| node.value("Cur"))),
        currency_label: project_node
            .and_then(|node| node.value("CurLbl"))
            .or_else(|| {
                award
                    .child("AwardInfo")
                    .and_then(|node| node.value("CurLbl"))
            }),
        boq_id: award
            .child("AwardInfo")
            .and_then(|node| node.value("BoQID")),
    };
    let mut count = 0;
    let boq = award
        .descendant("BoQ")
        .map(|boq| parse_boq(boq, options, &mut count))
        .transpose()?
        .unwrap_or_default();
    let invoice = if matches!(meta.phase, ExchangePhase::X89 | ExchangePhase::X89B) {
        Some(parse_invoice(award)?)
    } else {
        None
    };
    Ok(ProcurementDocument {
        meta,
        project,
        boq,
        invoice,
    })
}

fn parse_invoice(node: &Node) -> Result<InvoiceDocument, GaebError> {
    Ok(InvoiceDocument {
        owner: node.child("OWN").map(parse_invoice_party).transpose()?,
        requesters: node
            .children_named("Requester")
            .map(parse_invoice_party)
            .collect::<Result<_, _>>()?,
        contractor: node.child("CTR").map(parse_invoice_party).transpose()?,
        construction_sites: node
            .children_named("CnstSite")
            .map(|site| parse_invoice_site(site, "CnstSiteIDNo", "CnstSiteName"))
            .collect::<Result<_, _>>()?,
        notification_sites: node
            .children_named("NotifSite")
            .map(|site| parse_invoice_site(site, "NotifSiteIDNo", "NotifSiteName"))
            .collect::<Result<_, _>>()?,
        additional_texts: parse_invoice_texts(node),
        header: node.child("InvoiceHeader").map(parse_invoice_header),
        creator: node
            .child("InvoiceCreator")
            .map(parse_invoice_party)
            .transpose()?,
        recipient: node
            .child("InvoiceRecipient")
            .map(parse_invoice_party)
            .transpose()?,
        check_notes: node
            .children_named("CheckNote")
            .map(|note| {
                Ok(InvoiceCheckNote {
                    state: note.value("State").unwrap_or_default(),
                    check_date: note.value("CheckDate").unwrap_or_default(),
                    annotation: note.value("Annotation").unwrap_or_default(),
                })
            })
            .collect::<Result<_, GaebError>>()?,
        budget_department: node.value("BudgetDept"),
        booking_number: node.value("BookingNo"),
        quantity_determination_remark: node.value("QuantityDeterminationRemark"),
        shares: node
            .children_named("InvoiceShare")
            .map(|share| {
                Ok(InvoiceShare {
                    share_type: share.value("InvoiceShareType").unwrap_or_default(),
                    description: share.value("Description").unwrap_or_default(),
                    total: decimal(share.value("Total"), "InvoiceShare Total")?,
                    percent: decimal(share.value("Percent"), "InvoiceShare Percent")?,
                    counter_claim: share.value("CounterClaim").is_some_and(|value| yes(&value)),
                })
            })
            .collect::<Result<_, GaebError>>()?,
        payment_terms: node
            .children_named("Terms")
            .map(parse_invoice_payment_term)
            .collect::<Result<_, _>>()?,
        payments_made: node
            .children_named("PaymentMade")
            .map(parse_invoice_payment)
            .collect::<Result<_, _>>()?,
        invoices_made_out: node
            .children_named("InvoiceMadeOut")
            .map(parse_invoice_issued)
            .collect::<Result<_, _>>()?,
        total_gross: decimal(node.value("TotalGross"), "TotalGross")?,
    })
}

fn parse_invoice_address(node: &Node) -> InvoiceAddress {
    InvoiceAddress {
        name1: node.value("Name1").unwrap_or_default(),
        name2: node.value("Name2"),
        name3: node.value("Name3"),
        name4: node.value("Name4"),
        street: node.value("Street").unwrap_or_default(),
        postal_code: node.value("PCode").unwrap_or_default(),
        city: node.value("City").unwrap_or_default(),
        country: node.value("Country"),
        iln: node.value("ILN"),
        contact: node.value("Contact"),
        phone: node.value("Phone"),
        fax: node.value("Fax"),
        email: node.value("Email"),
        vat_id: node.value("VATID"),
        banks: node
            .children_named("Bank")
            .map(parse_invoice_bank)
            .collect(),
    }
}

fn parse_invoice_bank(node: &Node) -> InvoiceBank {
    InvoiceBank {
        label: node.value("LblBank").unwrap_or_default(),
        bank_code: node.value("BRNo").unwrap_or_default(),
        account_number: node.value("AcctNo").unwrap_or_default(),
        bic: node.value("BIC"),
        iban: node.value("IBAN"),
    }
}

fn parse_invoice_text(node: &Node) -> InvoiceText {
    InvoiceText {
        outline: node.value("OutlineAddText"),
        detail: node
            .value("DetailAddText")
            .unwrap_or_else(|| node.all_text()),
    }
}

fn parse_invoice_texts(node: &Node) -> Vec<InvoiceText> {
    node.children_named("AddText")
        .map(parse_invoice_text)
        .collect()
}

fn parse_invoice_party(node: &Node) -> Result<InvoiceParty, GaebError> {
    Ok(InvoiceParty {
        address: node.child("Address").map(parse_invoice_address),
        tax_no: node.value("TaxNo"),
        dp_no: node.value("DPNo"),
        award_no: node.value("AwardNo"),
        account_receivable_no: node.value("AcctRecNo"),
        accounts_payable_no: node.value("AcctsPayNo"),
        bidder_no: node.value("BidderNo"),
        request_id_no: node.value("RequestIDNo"),
        request_name: node.value("RequestName"),
        country_type: node.value("CntryType"),
        country_name: node.value("CntryName"),
        sector_type: node.value("SectorType"),
        preferred_bidder_type: node.value("PrefBidType"),
        service_type: node.value("SCType"),
        additional_texts: parse_invoice_texts(node),
    })
}

fn parse_invoice_site(
    node: &Node,
    id_name: &str,
    name_name: &str,
) -> Result<InvoiceSite, GaebError> {
    Ok(InvoiceSite {
        address: node.child("Address").map(parse_invoice_address),
        id_no: node.value(id_name),
        name: node.value(name_name),
        additional_texts: parse_invoice_texts(node),
    })
}

fn parse_invoice_header(node: &Node) -> InvoiceHeader {
    InvoiceHeader {
        invoice_no: node.value("InvoiceNo"),
        reference_invoice_no: node.value("RefInvoiceNo"),
        invoice_date: node.value("InvoiceDate"),
        invoice_type: node.value("InvoiceType"),
        credit_note: node.value("CreditNote").is_some_and(|value| yes(&value)),
        settlement_type: node.value("SettlementType"),
        sequential_no: node
            .value("SequentialNo")
            .and_then(|value| value.parse().ok()),
        service_provision_start_date: node.value("ServiceProvisionStartDate").unwrap_or_default(),
        service_provision_end_date: node.value("ServiceProvisionEndDate").unwrap_or_default(),
    }
}

fn parse_invoice_payment_term(node: &Node) -> Result<InvoicePaymentTerm, GaebError> {
    let days = node.child("CashDiscDays");
    Ok(InvoicePaymentTerm {
        cash_discount_days: days
            .and_then(|value| value.attr("Days"))
            .and_then(|value| value.parse().ok()),
        days_type: days.and_then(|value| value.attr("TypeOfDays")),
        cash_discount_date: node.value("CashDiscDate"),
        discount_pct: decimal(node.value("DiscountPcnt"), "DiscountPcnt")?.unwrap_or_default(),
    })
}

fn parse_invoice_payment(node: &Node) -> Result<InvoicePayment, GaebError> {
    Ok(InvoicePayment {
        total_vat: decimal(node.value("TotalVAT"), "TotalVAT")?.unwrap_or_default(),
        discount_amount: decimal(node.value("DiscountAmt"), "DiscountAmt")?,
        discount_vat_amount: decimal(node.value("DiscountVATAmt"), "DiscountVATAmt")?,
        total: decimal(node.value("Total"), "Total")?.unwrap_or_default(),
        payment_date: node.value("PaymentDate").unwrap_or_default(),
        invoice_type: node.value("InvoiceType"),
        invoice_no: node.value("InvoiceNo").unwrap_or_default(),
        payment_no: node.value("PaymentNo"),
        payment_note: node.value("PaymentNote"),
    })
}

fn parse_invoice_issued(node: &Node) -> Result<InvoiceIssued, GaebError> {
    Ok(InvoiceIssued {
        total_vat: decimal(node.value("TotalVAT"), "TotalVAT")?.unwrap_or_default(),
        total: decimal(node.value("Total"), "Total")?.unwrap_or_default(),
        invoice_date: node.value("InvoiceDate").unwrap_or_default(),
        invoice_type: node.value("InvoiceType").unwrap_or_default(),
        invoice_no: node.value("InvoiceNo").unwrap_or_default(),
    })
}

fn parse_boq(node: &Node, options: &ParseOptions, count: &mut usize) -> Result<Boq, GaebError> {
    let body = node.child("BoQBody").or_else(|| node.descendant("BoQBody"));
    let (remarks, performance_descriptions) = body
        .map(parse_itemlist_annotations)
        .transpose()?
        .unwrap_or_default();
    let info = node.child("BoQInfo");
    let has_lot_breakdown = info
        .into_iter()
        .flat_map(|info| info.children_named("BoQBkdn"))
        .any(|breakdown| breakdown.value("Type").as_deref() == Some("Lot"));
    let (categories, lots) = if has_lot_breakdown {
        let lots = body
            .into_iter()
            .flat_map(|body| body.children_named("BoQCtgy"))
            .map(|lot| parse_lot(lot, options, count))
            .collect::<Result<Vec<_>, _>>()?;
        (Vec::new(), lots)
    } else {
        let categories = body
            .map(|body| parse_categories(body, options, count, &[]))
            .transpose()?
            .unwrap_or_default();
        (categories, Vec::new())
    };
    Ok(Boq {
        id: node.attr("ID"),
        name: info.and_then(|info| info.value("Name")),
        cost_types: info
            .into_iter()
            .flat_map(|info| info.children_named("CostType"))
            .map(|cost_type| crate::CostType {
                name: cost_type.value("Name").unwrap_or_default(),
                label: cost_type.value("Label").unwrap_or_default(),
            })
            .collect(),
        categories,
        lots,
        remarks,
        performance_descriptions,
        totals: info
            .and_then(|info| info.descendant("Totals"))
            .and_then(|totals| decimal(totals.value("Total"), "total").transpose())
            .transpose()?,
    })
}

fn parse_lot(node: &Node, options: &ParseOptions, count: &mut usize) -> Result<Lot, GaebError> {
    let rno = node.attr("RNoPart").unwrap_or_default();
    let container = node.child("BoQBody").unwrap_or(node);
    let (remarks, performance_descriptions) = parse_itemlist_annotations(container)?;
    let lot_path = if rno.is_empty() {
        Vec::new()
    } else {
        vec![rno.clone()]
    };
    let categories = parse_categories(container, options, count, &lot_path)?;
    Ok(Lot {
        id: node.attr("ID"),
        rno,
        label: node.value("LblTx").unwrap_or_default(),
        categories,
        remarks,
        performance_descriptions,
        declared_total: node
            .child("Totals")
            .and_then(|totals| decimal(totals.value("Total"), "lot total").transpose())
            .transpose()?,
        ..Lot::default()
    })
}

fn parse_categories(
    node: &Node,
    options: &ParseOptions,
    count: &mut usize,
    path: &[String],
) -> Result<Vec<BoqCategory>, GaebError> {
    let mut categories: Vec<_> = node
        .children_named("BoQCtgy")
        .map(|category| parse_category(category, options, count, path))
        .collect::<Result<_, _>>()?;
    let bare_items = parse_items(node, options, count, path)?;
    if !bare_items.is_empty() {
        categories.push(BoqCategory {
            items: bare_items,
            ..BoqCategory::default()
        });
    }
    Ok(categories)
}
fn parse_category(
    node: &Node,
    options: &ParseOptions,
    count: &mut usize,
    parent_path: &[String],
) -> Result<BoqCategory, GaebError> {
    let body = node.child("BoQBody");
    let item_container = body.unwrap_or(node);
    let (remarks, performance_descriptions) = parse_itemlist_annotations(item_container)?;
    let rno = node.attr("RNoPart").unwrap_or_default();
    let mut path = parent_path.to_vec();
    if !rno.is_empty() {
        path.push(rno.clone());
    }
    Ok(BoqCategory {
        id: node.attr("ID"),
        rno,
        label: node.value("LblTx").unwrap_or_default(),
        declared_total: node
            .child("Totals")
            .and_then(|totals| decimal(totals.value("Total"), "category total").transpose())
            .transpose()?,
        items: parse_items(item_container, options, count, &path)?,
        remarks,
        performance_descriptions,
        bid_up_down_request: node
            .value("BidUpDownReq")
            .or_else(|| node.value("BidUpDown")),
        categories: item_container
            .children_named("BoQCtgy")
            .map(|category| parse_category(category, options, count, &path))
            .collect::<Result<_, _>>()?,
    })
}
fn parse_itemlist_annotations(node: &Node) -> Result<(Vec<String>, Vec<String>), GaebError> {
    let mut remarks = Vec::new();
    let mut performance_descriptions = Vec::new();
    let mut annotation_nodes = node
        .children
        .iter()
        .filter(|child| matches!(child.name.as_str(), "Remark" | "PerfDescr"));
    for child in annotation_nodes.by_ref() {
        match child.name.as_str() {
            "Remark" => remarks.push(
                child
                    .descendant("Description")
                    .map(Node::all_text)
                    .unwrap_or_else(|| child.all_text()),
            ),
            "PerfDescr" => performance_descriptions.push(
                child
                    .descendant("Description")
                    .map(Node::all_text)
                    .unwrap_or_else(|| child.all_text()),
            ),
            _ => unreachable!(),
        }
    }
    for list in node.children_named("Itemlist") {
        for child in &list.children {
            match child.name.as_str() {
                "Remark" => remarks.push(
                    child
                        .descendant("Description")
                        .map(Node::all_text)
                        .unwrap_or_else(|| child.all_text()),
                ),
                "PerfDescr" => performance_descriptions.push(
                    child
                        .descendant("Description")
                        .map(Node::all_text)
                        .unwrap_or_else(|| child.all_text()),
                ),
                "Item" | "MarkupItem" | "HourItem" | "MaterialItem" | "MastAgrItems"
                | "MarkupItemOWN" | "MarkupItemCTR" => {}
                _ => {
                    return Err(GaebError::Unsupported(format!(
                        "unsupported Itemlist element: {}",
                        child.name
                    )));
                }
            }
        }
    }
    Ok((remarks, performance_descriptions))
}

fn parse_items(
    node: &Node,
    options: &ParseOptions,
    count: &mut usize,
    path: &[String],
) -> Result<Vec<Item>, GaebError> {
    let mut items = Vec::new();
    for list in node.children_named("Itemlist") {
        for child in &list.children {
            match child.name.as_str() {
                "Item" => items.push(parse_item(child, options, count, path)?),
                "MarkupItem" | "MarkupItemOWN" | "MarkupItemCTR" => {
                    items.push(parse_markup_item(child, options, count, path)?)
                }
                "HourItem" | "MaterialItem" => {
                    items.push(parse_time_item(child, options, count, path)?);
                }
                "MastAgrItems" => {
                    for special in &child.children {
                        match special.name.as_str() {
                            "HourItem" | "MaterialItem" => {
                                items.push(parse_time_item(special, options, count, path)?)
                            }
                            "MarkupItemOWN" | "MarkupItemCTR" => {
                                items.push(parse_markup_item(special, options, count, path)?)
                            }
                            _ => {
                                return Err(GaebError::Unsupported(format!(
                                    "unsupported MastAgrItems element: {}",
                                    special.name
                                )));
                            }
                        }
                    }
                }
                "Remark" | "PerfDescr" => {}
                _ => {
                    return Err(GaebError::Unsupported(format!(
                        "unsupported Itemlist element: {}",
                        child.name
                    )));
                }
            }
        }
    }
    Ok(items)
}
fn parse_item(
    node: &Node,
    options: &ParseOptions,
    count: &mut usize,
    path: &[String],
) -> Result<Item, GaebError> {
    *count += 1;
    if *count > options.max_items {
        return Err(GaebError::ItemLimit {
            limit: options.max_items,
        });
    }
    let item_type = if node.child("ALNGroupNo").is_some() || node.child("AlternativeItem").is_some()
    {
        ItemType::Alternative
    } else if node.child("LumpSumItem").is_some() {
        ItemType::LumpSum
    } else if node.child("Provis").is_some() {
        ItemType::Eventual
    } else if node.child("TextItem").is_some() {
        ItemType::TextOnly
    } else if node.child("SupplementItem").is_some() {
        ItemType::Supplement
    } else {
        ItemType::Normal
    };
    let mut extensions = BTreeMap::new();
    for child in &node.children {
        if !matches!(
            child.name.as_str(),
            "Qty"
                | "PredQty"
                | "BillQty"
                | "PredIT"
                | "QtyTBD"
                | "QU"
                | "UP"
                | "IT"
                | "VAT"
                | "Description"
                | "ShortText"
                | "ALNGroupNo"
                | "ALNSerNo"
                | "QtySplit"
                | "CONo"
                | "COStatus"
                | "GUID"
                | "BIMRef"
                | "CostApproach"
                | "BidderUP"
                | "BidderTender"
                | "TenderingPty"
                | "UPComp1"
                | "UPComp2"
                | "UPComp3"
                | "UPComp4"
                | "UPComp5"
                | "UPComp6"
                | "DiscountPcnt"
                | "NotAppl"
                | "NotOffered"
                | "HourIt"
                | "MarkupIt"
                | "UPBkdn"
                | "BidUpDownReq"
                | "BidUpDown"
                | "BidUpDownPct"
                | "SumDescr"
                | "RefDescr"
                | "RefRNo"
                | "RefPerfNo"
                | "RefItem"
                | "UPFrom"
                | "UPAvg"
                | "UPTo"
                | "CtlgAssign"
                | "SubDescr"
                | "STLBBau"
                | "StLNo"
                | "TextComplement"
                | "MarkupType"
                | "MarkupSubQty"
                | "Markup"
                | "ITMarkup"
                | "AlternativeItem"
                | "LumpSumItem"
                | "Provis"
                | "TextItem"
                | "SupplementItem"
        ) {
            extensions.insert(child.name.clone(), child.all_text());
        }
    }
    let rich_text = parse_rich_text(node);
    let long_text = node
        .value("LongText")
        .or_else(|| {
            node.descendant("DetailTxt")
                .map(|detail| detail.text_without("TextComplement"))
        })
        .unwrap_or_default();
    let long_text = rich_text
        .as_ref()
        .map(|text| text.plain_text.clone())
        .filter(|text| !text.is_empty())
        .unwrap_or(long_text);
    Ok(Item {
        id: node.attr("ID"),
        oz: node.attr("RNoPart").unwrap_or_default(),
        oz_path: path.to_vec(),
        rno_index: node.attr("RNoIndex"),
        alternative_group: node.value("ALNGroupNo"),
        alternative_serial: node.value("ALNSerNo"),
        short_text: node
            .value("ShortText")
            .or_else(|| node.descendant("TextOutlTxt").map(Node::all_text))
            .unwrap_or_default(),
        long_text,
        rich_text,
        attachments: parse_attachments(node),
        classification: None,
        extractions: BTreeMap::new(),
        raw_data: extensions.clone(),
        quantity: decimal(node.value("Qty"), "Qty")?,
        predicted_quantity: decimal(node.value("PredQty"), "PredQty")?,
        bill_quantity: decimal(node.value("BillQty"), "BillQty")?,
        quantity_tbd: node.value("QtyTBD").is_some_and(|value| yes(&value)),
        unit: node.value("QU"),
        unit_price: decimal(node.value("UP"), "UP")?,
        total_price: decimal(node.value("IT").or_else(|| node.value("PredIT")), "IT")?,
        item_type,
        source_item_kind: None,
        vat: decimal(node.value("VAT"), "VAT")?,
        bim_guid: node.value("GUID").or_else(|| node.value("BIMRef")),
        change_order_number: node.value("CONo"),
        co_status: node.value("COStatus"),
        discount_pct: decimal(node.value("DiscountPcnt"), "DiscountPcnt")?,
        markup_type: node.value("MarkupType"),
        not_applicable: node
            .value("NotAppl")
            .or_else(|| node.value("NotOffered"))
            .is_some_and(|value| yes(&value)),
        hour_item: node.value("HourIt").is_some_and(|value| yes(&value)),
        markup_item: node.value("MarkupIt").is_some_and(|value| yes(&value)),
        up_breakdown: node.value("UPBkdn").is_some_and(|value| yes(&value)),
        sum_description: node.value("SumDescr").is_some_and(|value| yes(&value)),
        bid_up_down_request: node
            .value("BidUpDownReq")
            .or_else(|| node.value("BidUpDown")),
        bid_up_down_pct: decimal(node.value("BidUpDownPct"), "BidUpDownPct")?,
        reference_description: node
            .child("RefDescr")
            .and_then(|reference| reference.attr("IDRef")),
        reference_rno: node
            .child("RefRNo")
            .and_then(|reference| reference.attr("IDRef")),
        reference_performance: node
            .child("RefPerfNo")
            .and_then(|reference| reference.attr("IDRef")),
        up_from: decimal(node.value("UPFrom"), "UPFrom")?,
        up_avg: decimal(node.value("UPAvg"), "UPAvg")?,
        up_to: decimal(node.value("UPTo"), "UPTo")?,
        up_components: (1..=6)
            .filter_map(|index| {
                decimal(node.value(&format!("UPComp{index}")), "UPComp").transpose()
            })
            .collect::<Result<_, _>>()?,
        qty_splits: parse_qty_splits(node)?,
        cost_approaches: parse_cost_approaches(node)?,
        bidder_prices: parse_bidder_prices(node)?,
        markup_sub_qtys: parse_markup_sub_qtys(node)?,
        ctlg_assigns: parse_ctlg_assigns(node)?,
        text_complements: parse_text_complements(node),
        sub_descriptions: parse_sub_descriptions(node),
        standard_service: parse_standard_service(node),
        catalog_number: node.value("WICNo"),
        extensions,
    })
}

fn collect_rich_paragraphs(node: &Node, excluded: bool, paragraphs: &mut Vec<String>) {
    let excluded = excluded || node.name == "TextComplement";
    if node.name == "p" {
        if !excluded {
            let text = node.all_text();
            if !text.is_empty() {
                paragraphs.push(text);
            }
        }
        return;
    }
    for child in &node.children {
        collect_rich_paragraphs(child, excluded, paragraphs);
    }
}

fn rich_node_html(node: &Node, output: &mut String) {
    output.push('<');
    output.push_str(&node.name);
    for (name, value) in &node.attributes {
        output.push(' ');
        output.push_str(name);
        output.push_str("=\"");
        output.push_str(&value.replace('&', "&amp;").replace('"', "&quot;"));
        output.push('"');
    }
    if node.children.is_empty() && node.text.is_empty() {
        output.push_str("/>");
        return;
    }
    output.push('>');
    output.push_str(
        &node
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;"),
    );
    for child in &node.children {
        rich_node_html(child, output);
    }
    output.push_str("</");
    output.push_str(&node.name);
    output.push('>');
}

fn parse_rich_text(node: &Node) -> Option<crate::RichText> {
    let detail = node.descendant("DetailTxt")?;
    let mut paragraphs = Vec::new();
    collect_rich_paragraphs(detail, false, &mut paragraphs);
    let tables = detail
        .descendants("table")
        .map(|table| {
            table
                .descendants("tr")
                .map(|row| {
                    row.children
                        .iter()
                        .filter(|cell| matches!(cell.name.as_str(), "td" | "th" | "Cell"))
                        .map(Node::all_text)
                        .collect::<Vec<_>>()
                })
                .filter(|row: &Vec<String>| !row.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|table: &Vec<Vec<String>>| !table.is_empty())
        .collect();
    let images = detail
        .descendants("img")
        .chain(detail.descendants("image"))
        .filter_map(|image| {
            image
                .attr("src")
                .or_else(|| image.attr("IDRef"))
                .or_else(|| {
                    let text = image.all_text();
                    (!text.is_empty()).then_some(text)
                })
        })
        .collect();
    let detail_text = detail.text_without("TextComplement");
    let plain_text = if !paragraphs.is_empty() && detail_text == paragraphs.join(" ") {
        paragraphs.join("\n")
    } else {
        detail_text
    };
    let mut raw_html = String::new();
    for child in &detail.children {
        rich_node_html(child, &mut raw_html);
    }
    Some(crate::RichText {
        paragraphs,
        tables,
        images,
        raw_html: Some(raw_html),
        plain_text,
    })
}

fn parse_attachments(node: &Node) -> Vec<crate::Attachment> {
    node.descendants("Attachment")
        .filter_map(|attachment| {
            let encoded = attachment.value("Data")?;
            let data = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                encoded.as_bytes(),
            )
            .ok()?;
            let filename = attachment
                .value("Name")
                .or_else(|| attachment.attr("Name"))
                .unwrap_or_default();
            let mime_type = attachment
                .value("MimeType")
                .or_else(|| attachment.value("Type"))
                .unwrap_or_else(|| "application/octet-stream".to_owned());
            Some(crate::Attachment {
                filename,
                mime_type,
                size_bytes: data.len(),
                data,
            })
        })
        .collect()
}

fn parse_time_item(
    node: &Node,
    options: &ParseOptions,
    count: &mut usize,
    path: &[String],
) -> Result<Item, GaebError> {
    let mut item = parse_item(node, options, count, path)?;
    item.quantity = decimal(node.value("Qty").or_else(|| node.value("PredQty")), "Qty")?;
    item.total_price = decimal(node.value("IT").or_else(|| node.value("PredIT")), "IT")?;
    item.source_item_kind = Some(node.name.clone());
    Ok(item)
}

fn parse_markup_item(
    node: &Node,
    options: &ParseOptions,
    count: &mut usize,
    path: &[String],
) -> Result<Item, GaebError> {
    let mut item = parse_item(node, options, count, path)?;
    item.item_type = ItemType::Markup;
    item.source_item_kind = Some(node.name.clone());
    item.unit_price = decimal(node.value("Markup"), "Markup")?;
    item.total_price = decimal(node.value("ITMarkup"), "ITMarkup")?.or(item.total_price);
    item.markup_type = node.value("MarkupType");
    if item.markup_sub_qtys.is_empty() {
        item.markup_sub_qtys = node
            .children_named("RefItem")
            .map(|reference| crate::MarkupSubQty {
                ref_rno: String::new(),
                ref_id: reference.attr("IDRef"),
                quantity: None,
            })
            .collect();
    }
    Ok(item)
}

fn parse_qty_splits(node: &Node) -> Result<Vec<crate::QtySplit>, GaebError> {
    node.children_named("QtySplit")
        .map(|split| {
            Ok(crate::QtySplit {
                label: split
                    .value("Label")
                    .or_else(|| split.value("Description"))
                    .unwrap_or_default(),
                quantity: decimal(split.value("Qty"), "Qty")?.unwrap_or_default(),
                unit: split.value("QU"),
            })
        })
        .collect()
}

fn parse_cost_approaches(node: &Node) -> Result<Vec<crate::CostApproach>, GaebError> {
    node.children_named("CostApproach")
        .map(|approach| {
            Ok(crate::CostApproach {
                cost_type: approach.value("CostType").unwrap_or_default(),
                amount: decimal(approach.value("Amount"), "Amount")?,
                remark: approach.value("Remark").unwrap_or_default(),
            })
        })
        .collect()
}

fn parse_bidder_prices(node: &Node) -> Result<Vec<crate::BidderPrice>, GaebError> {
    ["BidderUP", "BidderTender", "TenderingPty"]
        .into_iter()
        .flat_map(|name| node.children_named(name))
        .map(|bidder| {
            Ok(crate::BidderPrice {
                bidder_name: bidder
                    .value("BidderName")
                    .or_else(|| bidder.value("Name"))
                    .or_else(|| bidder.attr("name"))
                    .unwrap_or_default(),
                bidder_id: bidder
                    .value("BidderID")
                    .or_else(|| bidder.attr("id"))
                    .or_else(|| bidder.attr("BidderID")),
                unit_price: decimal(bidder.value("UP"), "UP")?,
                total_price: decimal(bidder.value("IT"), "IT")?,
                affects_total: true,
                rank: None,
            })
        })
        .collect()
}

fn parse_markup_sub_qtys(node: &Node) -> Result<Vec<crate::MarkupSubQty>, GaebError> {
    node.children_named("MarkupSubQty")
        .map(|sub| {
            let reference = sub.child("RefItem");
            Ok(crate::MarkupSubQty {
                ref_rno: sub.value("RefRNoPart").unwrap_or_default(),
                ref_id: reference.and_then(|item| item.attr("IDRef")),
                quantity: decimal(sub.value("SubQty"), "SubQty")?,
            })
        })
        .collect()
}

fn parse_ctlg_assigns(node: &Node) -> Result<Vec<crate::CtlgAssign>, GaebError> {
    node.children_named("CtlgAssign")
        .map(|assign| {
            Ok(crate::CtlgAssign {
                ctlg_id: assign.value("CtlgID").unwrap_or_default(),
                ctlg_code: assign.value("CtlgCode").unwrap_or_default(),
                quantity: decimal(assign.value("Quantity"), "Quantity")?,
            })
        })
        .collect()
}

fn parse_text_complements(node: &Node) -> Vec<TextComplement> {
    node.child("Description")
        .into_iter()
        .flat_map(|description| description.descendants("TextComplement"))
        .map(|complement| TextComplement {
            kind: complement.attr("Kind").unwrap_or_default(),
            mark_label: complement.attr("MarkLbl"),
            empty: complement.attr("Empty").is_some_and(|value| yes(&value)),
            caption: complement.child("ComplCaption").map(Node::all_text),
            text: complement
                .child("ComplBody")
                .map(Node::all_text)
                .unwrap_or_default(),
            tail: complement.child("ComplTail").map(Node::all_text),
        })
        .collect()
}

fn parse_sub_descriptions(node: &Node) -> Vec<SubDescription> {
    node.children_named("SubDescr")
        .map(|sub| SubDescription {
            number: sub.value("SubDNo").unwrap_or_default(),
            short_text: sub
                .descendant("OutlineText")
                .map(Node::all_text)
                .unwrap_or_default(),
            long_text: sub
                .descendant("DetailTxt")
                .map(|detail| detail.text_without("TextComplement"))
                .unwrap_or_default(),
            text_complements: sub
                .child("Description")
                .map(|description| {
                    description
                        .descendants("TextComplement")
                        .map(parse_text_complement)
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect()
}

fn parse_text_complement(complement: &Node) -> TextComplement {
    TextComplement {
        kind: complement.attr("Kind").unwrap_or_default(),
        mark_label: complement.attr("MarkLbl"),
        empty: complement.attr("Empty").is_some_and(|value| yes(&value)),
        caption: complement.child("ComplCaption").map(Node::all_text),
        text: complement
            .child("ComplBody")
            .map(Node::all_text)
            .unwrap_or_default(),
        tail: complement.child("ComplTail").map(Node::all_text),
    }
}

fn parse_standard_service(node: &Node) -> Option<StandardService> {
    let description = node.child("Description")?;
    let stlb = description.child("STLBBau");
    let stl = description.child("StLNo");
    if stlb.is_none() && stl.is_none() {
        return None;
    }
    let catalog_node = stlb.and_then(|service| service.child("STLBBauCtlg"));
    let identifiers: Vec<_> = stlb
        .into_iter()
        .flat_map(|service| service.children_named("STLBBauID"))
        .collect();
    Some(StandardService {
        catalog: catalog_node
            .and_then(|catalog| catalog.value("CtlgName"))
            .or_else(|| stlb.and_then(|service| service.value("STLBBauCtlg"))),
        catalog_version: catalog_node.and_then(|catalog| catalog.value("VersDate")),
        work_category: catalog_node.and_then(|catalog| catalog.value("WCtg")),
        group: catalog_node.and_then(|catalog| catalog.value("Group")),
        identifiers: identifiers
            .iter()
            .map(|identifier| {
                identifier
                    .value("ArtChrIdent")
                    .unwrap_or_else(|| identifier.all_text())
            })
            .collect(),
        identifier_indices: identifiers
            .iter()
            .filter_map(|identifier| identifier.value("ArtChIdx"))
            .collect(),
        change_identifiers: identifiers
            .iter()
            .filter_map(|identifier| identifier.value("ChVIdent"))
            .collect(),
        number: stl.map(Node::all_text),
    })
}

fn parse_trade(
    root: &Node,
    meta: DocumentMeta,
    options: &ParseOptions,
) -> Result<TradeDocument, GaebError> {
    let order = root
        .descendant("Order")
        .ok_or_else(|| GaebError::Unsupported("trade phase has no Order".to_owned()))?;
    let project_node = root.child("PrjInfo");
    let project = ProjectInfo {
        id: project_node.and_then(|node| node.value("PrjID")),
        name: project_node.and_then(|node| node.value("NamePrj")),
        label: project_node.and_then(|node| node.value("LblPrj")),
        currency: project_node.and_then(|node| node.value("Cur")),
        currency_label: project_node.and_then(|node| node.value("CurLbl")),
        boq_id: None,
    };
    let order_info = order.child("OrderInfo");
    let items = order
        .descendants("OrderItem")
        .enumerate()
        .map(|(index, node)| {
            if index >= options.max_items {
                return Err(GaebError::ItemLimit {
                    limit: options.max_items,
                });
            }
            for child in &node.children {
                if !matches!(
                    child.name.as_str(),
                    "ItemChara"
                        | "ItemType"
                        | "Service"
                        | "EAN"
                        | "ArtNoID"
                        | "ArtNo"
                        | "SupplierArtNoID"
                        | "SupplierArtNo"
                        | "CustomerArtNo"
                        | "CatalogArtNo"
                        | "CatalogNo"
                        | "ItemID"
                        | "ShortText"
                        | "Description"
                        | "Qty"
                        | "QU"
                        | "OfferPrice"
                        | "NetPrice"
                        | "UP"
                        | "PriceBasis"
                        | "AQU"
                        | "ModeOfShipment"
                        | "DeliveryDate"
                        | "DeliveryTime"
                        | "DeliveryWeek"
                        | "DeliveryChara"
                        | "CtlgAssign"
                ) {
                    return Err(GaebError::Unsupported(format!(
                        "unsupported OrderItem element: {}",
                        child.name
                    )));
                }
            }
            Ok(TradeItem {
                id: node
                    .attr("ID")
                    .or_else(|| node.value("ItemID"))
                    .unwrap_or_default(),
                item_chara: node.value("ItemChara"),
                item_type: node.value("ItemType"),
                service: node.value("Service"),
                ean: node.value("EAN"),
                article_number_id: node.value("ArtNoID"),
                article_number: node.value("ArtNo"),
                supplier_article_number_id: node.value("SupplierArtNoID"),
                supplier_article_number: node.value("SupplierArtNo"),
                customer_article_number: node.value("CustomerArtNo"),
                catalog_article_number: node.value("CatalogArtNo"),
                catalog_number: node.value("CatalogNo"),
                short_text: node
                    .value("ShortText")
                    .or_else(|| node.descendant("TextOutlTxt").map(Node::all_text))
                    .unwrap_or_default(),
                long_text: node
                    .descendant("DetailTxt")
                    .map(|detail| detail.text_without("TextComplement"))
                    .unwrap_or_default(),
                quantity: decimal(node.value("Qty"), "Qty")?,
                unit: node.value("QU"),
                offer_price: decimal(node.value("OfferPrice"), "OfferPrice")?,
                net_price: decimal(
                    node.value("NetPrice").or_else(|| node.value("UP")),
                    "NetPrice",
                )?,
                price_basis: decimal(node.value("PriceBasis"), "PriceBasis")?,
                alternate_unit: node.value("AQU"),
                mode_of_shipment: node.value("ModeOfShipment"),
                delivery_date: node.value("DeliveryDate"),
                delivery_time: node.value("DeliveryTime"),
                delivery_week: node.value("DeliveryWeek"),
                delivery_chara: node.value("DeliveryChara"),
                is_service: node.value("Service").is_some_and(|value| yes(&value)),
                ctlg_assigns: parse_ctlg_assigns(node)?,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(TradeDocument {
        order_number: order_info
            .and_then(|info| info.value("OrderNo"))
            .or_else(|| order.descendant("OrderNo").map(Node::all_text)),
        order_reference: order_info
            .and_then(|info| info.value("Ref").or_else(|| info.value("Reference"))),
        currency: order_info
            .and_then(|info| info.value("Cur"))
            .or_else(|| order.descendant("Cur").map(Node::all_text)),
        order_date: order_info
            .and_then(|info| info.value("OrderDate").or_else(|| info.value("Date")))
            .or_else(|| order.descendant("OrderDate").map(Node::all_text)),
        delivery_date: order_info.and_then(|info| info.value("DeliveryDate")),
        supplier: order.child("SupplierInfo").map(parse_trade_party),
        customer: order.child("CustomerInfo").map(parse_trade_party),
        delivery_place: order.child("DeliveryPlaceInfo").map(parse_trade_party),
        planner: order.child("PlannerInfo").map(parse_trade_party),
        invoice_recipient: order.child("InvoiceInfo").map(parse_trade_party),
        catalog_assigns: parse_ctlg_assigns(order)?,
        project,
        meta,
        items,
    })
}

fn parse_trade_party(node: &Node) -> TradeParty {
    TradeParty {
        address: Some(parse_trade_address(node.child("Address").unwrap_or(node))),
    }
}

fn parse_trade_address(node: &Node) -> TradeAddress {
    TradeAddress {
        name: node.value("Name1").or_else(|| node.value("Name")),
        name2: node.value("Name2"),
        name3: node.value("Name3"),
        name4: node.value("Name4"),
        street: node.value("Street"),
        postal_code: node.value("PCode"),
        city: node.value("City"),
        country: node.value("Country"),
        contact: node.value("Contact"),
        phone: node.value("Phone"),
        fax: node.value("Fax"),
        email: node.value("Email"),
        iln: node.value("ILN"),
        vat_id: node.value("VATID"),
    }
}
fn parse_cost_info(cost: &Node) -> crate::CostInfo {
    let Some(info) = cost.child("ECInfo").or_else(|| cost.descendant("ECInfo")) else {
        return crate::CostInfo::default();
    };
    crate::CostInfo {
        label: info.value("LblEC"),
        cost_type: info.value("ECType"),
        cost_method: info.value("ECMethod"),
        date: info.value("Date"),
        currency: info.value("Cur"),
        currency_label: info.value("CurLbl"),
        date_of_price: info.value("DateOfPrice"),
        date_of_information: info
            .child("DateOfInformation")
            .and_then(|node| node.value("Date")),
        total_net: decimal(
            info.descendant("Totals")
                .and_then(|node| node.value("TotalNet")),
            "TotalNet",
        )
        .ok()
        .flatten(),
        total_gross: decimal(
            info.descendant("Totals")
                .and_then(|node| node.value("TotalGross")),
            "TotalGross",
        )
        .ok()
        .flatten(),
        breakdowns: info
            .children_named("ECBkdn")
            .map(|breakdown| crate::CostBreakdown {
                bkdn_type: breakdown.value("Type").unwrap_or_default(),
                label: breakdown.value("LblOutline").unwrap_or_default(),
                length: breakdown
                    .value("Length")
                    .and_then(|value| value.parse().ok()),
                is_numeric: breakdown.value("Num").is_some_and(|value| yes(&value)),
            })
            .collect(),
        consortium_members: info
            .children_named("ConsortiumMember")
            .map(|member| crate::CostConsortiumMember {
                description: member.value("Description").unwrap_or_default(),
                address: member.child("Address").map(parse_trade_address),
            })
            .collect(),
    }
}

fn parse_cost(
    root: &Node,
    meta: DocumentMeta,
    options: &ParseOptions,
) -> Result<CostDocument, GaebError> {
    let cost = root
        .descendant("ElementalCosting")
        .ok_or_else(|| GaebError::Unsupported("cost phase has no ElementalCosting".to_owned()))?;
    let mut count = 0;
    let items = parse_cost_items(cost, options, &mut count)?;
    let categories = cost
        .child("ECBody")
        .map(|body| parse_cost_categories(body, options))
        .transpose()?
        .unwrap_or_default();
    let dimension_elements = cost
        .descendants("DimensionElement")
        .map(parse_cost_dimension_element)
        .collect();
    let category_elements = cost
        .descendants("CategoryElement")
        .map(parse_cost_category_element)
        .collect();
    Ok(CostDocument {
        dp: cost.value("DP"),
        name: cost
            .descendant("ECInfo")
            .and_then(|info| info.value("Name")),
        info: parse_cost_info(cost),
        categories,
        dimension_elements,
        category_elements,
        meta,
        items,
    })
}
fn parse_cost_categories(
    node: &Node,
    options: &ParseOptions,
) -> Result<Vec<crate::CostCategory>, GaebError> {
    node.children_named("ECCtgy")
        .map(|category| parse_cost_category(category, options))
        .collect()
}

fn parse_cost_category(
    node: &Node,
    options: &ParseOptions,
) -> Result<crate::CostCategory, GaebError> {
    let mut count = 0;
    let body = node.child("ECBody");
    let items = body
        .map(|body| parse_cost_items(body, options, &mut count))
        .transpose()?
        .unwrap_or_default();
    let categories = body
        .map(|body| parse_cost_categories(body, options))
        .transpose()?
        .unwrap_or_default();
    Ok(crate::CostCategory {
        id: node.value("EleNo").unwrap_or_default(),
        description: node
            .value("Descr")
            .or_else(|| node.value("Description"))
            .unwrap_or_default(),
        portion: decimal(node.value("Portion"), "Portion")?,
        properties: parse_cost_properties(node)?,
        totals_net: node
            .child("Totals")
            .and_then(|totals| decimal(totals.value("TotalNet"), "TotalNet").ok())
            .flatten(),
        totals_gross: node
            .child("Totals")
            .and_then(|totals| decimal(totals.value("TotalGross"), "TotalGross").ok())
            .flatten(),
        categories,
        items,
    })
}

fn parse_cost_items(
    node: &Node,
    options: &ParseOptions,
    count: &mut usize,
) -> Result<Vec<CostItem>, GaebError> {
    let mut items = Vec::new();
    for child in &node.children {
        match child.name.as_str() {
            "CostElement" => items.push(parse_cost_item(child, options, count)?),
            "ECBody" => items.extend(parse_cost_items(child, options, count)?),
            "ECCtgy" => {
                if let Some(body) = child.child("ECBody") {
                    items.extend(parse_cost_items(body, options, count)?);
                }
            }
            "DP" | "ECInfo" | "AddText" | "CostType" | "CostApproach" | "RefGroup"
            | "DimensionElement" | "CategoryElement" => {}
            _ => {
                return Err(GaebError::Unsupported(format!(
                    "unsupported cost element: {}",
                    child.name
                )));
            }
        }
    }
    Ok(items)
}
fn parse_cost_item(
    node: &Node,
    options: &ParseOptions,
    count: &mut usize,
) -> Result<CostItem, GaebError> {
    *count += 1;
    if *count > options.max_items {
        return Err(GaebError::ItemLimit {
            limit: options.max_items,
        });
    }
    for child in &node.children {
        if !matches!(
            child.name.as_str(),
            "EleNo"
                | "Descr"
                | "ShortText"
                | "Description"
                | "Qty"
                | "QU"
                | "UP"
                | "IT"
                | "CatID"
                | "CtlgAssign"
                | "Remark"
                | "Portion"
                | "Totals"
                | "Markup"
                | "UPFrom"
                | "UPAvg"
                | "UPTo"
                | "BillElement"
                | "Property"
                | "RefGroup"
                | "CostElement"
        ) {
            return Err(GaebError::Unsupported(format!(
                "unsupported CostElement element: {}",
                child.name
            )));
        }
    }
    Ok(CostItem {
        id: node
            .attr("ID")
            .or_else(|| node.value("EleNo"))
            .unwrap_or_default(),
        short_text: node
            .value("ShortText")
            .or_else(|| node.value("Descr"))
            .or_else(|| node.value("Description"))
            .unwrap_or_default(),
        quantity: decimal(node.value("Qty"), "Qty")?,
        unit: node.value("QU"),
        unit_price: decimal(node.value("UP"), "UP")?,
        total_price: decimal(node.value("IT"), "IT")?,
        cat_id: node.value("CatID"),
        ctlg_assigns: parse_ctlg_assigns(node)?,
        remark: node.value("Remark").unwrap_or_default(),
        portion: decimal(node.value("Portion"), "Portion")?,
        declared_total: node
            .child("Totals")
            .and_then(|totals| decimal(totals.value("Total"), "total").transpose())
            .transpose()?,
        markup: decimal(node.value("Markup"), "Markup")?,
        up_from: decimal(node.value("UPFrom"), "UPFrom")?,
        up_avg: decimal(node.value("UPAvg"), "UPAvg")?,
        up_to: decimal(node.value("UPTo"), "UPTo")?,
        is_bill_element: node.value("BillElement").is_some_and(|value| yes(&value)),
        properties: parse_cost_properties(node)?,
        reference_groups: parse_cost_reference_groups(node)?,
        children: node
            .children_named("CostElement")
            .map(|child| parse_cost_item(child, options, count))
            .collect::<Result<_, _>>()?,
    })
}
fn parse_cost_dimension_element(node: &Node) -> crate::CostDimensionElement {
    crate::CostDimensionElement {
        id: node
            .attr("ID")
            .or_else(|| node.value("EleNo"))
            .unwrap_or_default(),
        description: node
            .value("Descr")
            .or_else(|| node.value("Description"))
            .unwrap_or_default(),
        category_id: node.value("CatID"),
        remark: node.value("Remark").unwrap_or_default(),
        properties: parse_cost_properties(node).unwrap_or_default(),
        quantity: decimal(node.value("Qty"), "Qty").ok().flatten(),
        unit: node.value("QU").unwrap_or_default(),
        markup: decimal(node.value("Markup"), "Markup").ok().flatten(),
    }
}

fn parse_cost_category_element(node: &Node) -> crate::CostCategoryElement {
    crate::CostCategoryElement {
        id: node
            .attr("ID")
            .or_else(|| node.value("EleNo"))
            .unwrap_or_default(),
        description: node
            .value("Descr")
            .or_else(|| node.value("Description"))
            .unwrap_or_default(),
        category_id: node.value("CatID"),
        remark: node.value("Remark").unwrap_or_default(),
        properties: parse_cost_properties(node).unwrap_or_default(),
        markup: decimal(node.value("Markup"), "Markup").ok().flatten(),
    }
}

fn parse_cost_properties(node: &Node) -> Result<Vec<crate::CostProperty>, GaebError> {
    node.children_named("Property")
        .map(|property| {
            Ok(crate::CostProperty {
                name: property.value("Name").unwrap_or_default(),
                label: property.value("LblProp").unwrap_or_default(),
                arithmetic_quantity_approach: property.value("ArithmeticQuantityApproach"),
                value_quantity_approach: decimal(
                    property.value("ValueQuantityApproach"),
                    "ValueQuantityApproach",
                )?,
                unit: property.value("QU").unwrap_or_default(),
                property_type: property.value("Type"),
                cad_id: property.value("CAD_ID"),
            })
        })
        .collect()
}

fn parse_cost_reference_groups(node: &Node) -> Result<Vec<crate::CostReferenceGroup>, GaebError> {
    node.children_named("RefGroup")
        .map(|group| {
            let mut references = Vec::new();
            for child in &group.children {
                if !matches!(
                    child.name.as_str(),
                    "BoQItemRef"
                        | "BoQCtgyRef"
                        | "CostElementRef"
                        | "DimensionElementRef"
                        | "CategoryElementRef"
                        | "ConsortiumMemberRef"
                ) {
                    continue;
                }
                references.push(crate::CostReference {
                    id_ref: child.attr("IDRef").unwrap_or_default(),
                    reference_kind: child.name.clone(),
                    reference_type: child.attr("Type"),
                    portion: decimal(child.value("Portion"), "Portion")?,
                });
            }
            Ok(crate::CostReferenceGroup {
                title: group.value("Title").unwrap_or_default(),
                references,
            })
        })
        .collect()
}

fn parse_structured(root: &Node, meta: DocumentMeta) -> StructuredDocument {
    StructuredDocument {
        meta,
        elements: root
            .children
            .iter()
            .filter(|child| child.name != "GAEBInfo")
            .map(xml_element)
            .collect(),
    }
}

fn xml_element(node: &Node) -> XmlElement {
    XmlElement {
        name: node.name.clone(),
        attributes: node.attributes.clone(),
        text: node.text.clone(),
        children: node.children.iter().map(xml_element).collect(),
    }
}

fn node_from_xml_element(element: &XmlElement) -> Node {
    Node {
        name: element.name.clone(),
        attributes: element.attributes.clone(),
        text: element.text.clone(),
        children: element.children.iter().map(node_from_xml_element).collect(),
    }
}

fn parse_quantity(
    root: &Node,
    meta: DocumentMeta,
    options: &ParseOptions,
) -> Result<QuantityDocument, GaebError> {
    let quantity = root
        .descendant("QtyDeterm")
        .ok_or_else(|| GaebError::Unsupported("X31 has no QtyDeterm".to_owned()))?;
    let boq = quantity.child("BoQ").or_else(|| quantity.descendant("BoQ"));
    let items = quantity
        .descendants("Item")
        .chain(quantity.descendants("QtyItem"))
        .enumerate()
        .map(|(index, node)| {
            if index >= options.max_items {
                return Err(GaebError::ItemLimit {
                    limit: options.max_items,
                });
            }
            parse_quantity_item(node)
        })
        .collect::<Result<_, _>>()?;
    let categories = boq
        .and_then(|boq| boq.child("BoQBody"))
        .map(parse_quantity_categories)
        .unwrap_or_default();
    let catalogs = boq
        .into_iter()
        .flat_map(|boq| boq.children_named("Ctlg"))
        .map(|catalog| QuantityCatalog {
            id: catalog.value("CtlgID").unwrap_or_default(),
            kind: catalog.value("CtlgType").unwrap_or_default(),
            name: catalog.value("CtlgName").unwrap_or_default(),
            assignment_type: catalog.value("CtlgAssignType").unwrap_or_default(),
        })
        .collect();
    let attachments = boq
        .into_iter()
        .flat_map(|boq| boq.children_named("CtlgAttachment"))
        .flat_map(|container| container.children_named("Attachment"))
        .filter_map(parse_quantity_attachment)
        .collect();
    let project = quantity.child("PrjInfo");
    let info = quantity.child("QtyDetermInfo");
    let owner = quantity
        .child("OWN")
        .map(|node| parse_trade_address(node.child("Address").unwrap_or(node)));
    let contractor = quantity
        .child("CTR")
        .map(|node| parse_trade_address(node.child("Address").unwrap_or(node)));
    Ok(QuantityDocument {
        reference_project_name: project.and_then(|node| node.value("RefPrjName")),
        reference_project_id: project.and_then(|node| node.value("RefPrjID")),
        reference_boq_name: boq.and_then(|node| node.value("RefBoQName")),
        reference_boq_id: boq.and_then(|node| node.value("RefBoQID")),
        method: info.and_then(|node| node.value("MethodDescription")),
        order_description: info.and_then(|node| node.value("OrdDescr")),
        project_description: info.and_then(|node| node.value("ProjDescr")),
        service_start: info.and_then(|node| node.value("ServiceProvisionStartDate")),
        service_end: info.and_then(|node| node.value("ServiceProvisionEndDate")),
        owner,
        contractor,
        catalogs,
        attachments,
        categories,
        meta,
        items,
    })
}

fn parse_quantity_item(node: &Node) -> Result<QuantityItem, GaebError> {
    let determination = node.child("QtyDeterm");
    let determination_items = determination
        .into_iter()
        .flat_map(|determination| determination.children_named("QDetermItem"))
        .map(|item| QuantityDeterminationItem {
            raw: item
                .child("QTakeoff")
                .and_then(|takeoff| takeoff.attr("Row").or_else(|| Some(takeoff.all_text())))
                .unwrap_or_default(),
            catalog_assigns: parse_ctlg_assigns(item).unwrap_or_default(),
        })
        .collect();
    let takeoff_rows = node
        .descendants("QTakeoff")
        .chain(node.descendants("QTakeoffRow"))
        .map(|row| row.attr("Row").unwrap_or_else(|| row.all_text()))
        .collect();
    Ok(QuantityItem {
        oz: node
            .attr("RNoPart")
            .or_else(|| node.value("RNoPart"))
            .unwrap_or_default(),
        rno_part: node.attr("RNoPart").unwrap_or_default(),
        rno_index: node.attr("RNoIndex").unwrap_or_default(),
        quantity: decimal(
            node.value("Qty")
                .or_else(|| node.descendant("Qty").map(Node::all_text)),
            "Qty",
        )?,
        determination_items,
        ctlg_assigns: parse_ctlg_assigns(node)?,
        takeoff_rows,
    })
}

fn parse_quantity_categories(node: &Node) -> Vec<QuantityCategory> {
    let mut categories = node
        .children_named("BoQCtgy")
        .map(|category| QuantityCategory {
            rno: category.attr("RNoPart").unwrap_or_default(),
            items: category
                .child("Itemlist")
                .into_iter()
                .flat_map(|list| list.children_named("Item"))
                .filter_map(|item| parse_quantity_item(item).ok())
                .collect(),
            categories: category
                .child("BoQBody")
                .map(parse_quantity_categories)
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    if let Some(list) = node.child("Itemlist") {
        let items: Vec<_> = list
            .children_named("Item")
            .filter_map(|item| parse_quantity_item(item).ok())
            .collect();
        if !items.is_empty() {
            categories.push(QuantityCategory {
                rno: String::new(),
                items,
                categories: Vec::new(),
            });
        }
    }
    categories
}

fn parse_quantity_attachment(node: &Node) -> Option<QuantityAttachment> {
    let encoded = node.value("Data")?;
    let data = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        encoded.as_bytes(),
    )
    .ok()?;
    Some(QuantityAttachment {
        name: node.value("Name").unwrap_or_default(),
        text: node.value("Text").unwrap_or_default(),
        description: node.value("Descrip").unwrap_or_default(),
        file_type: node.value("Type").unwrap_or_default(),
        data,
    })
}
fn decimal(value: Option<String>, field: &'static str) -> Result<Option<Decimal>, GaebError> {
    value
        .map(|value| {
            Decimal::from_str_exact(&value.replace(',', "."))
                .map_err(|_| GaebError::InvalidDecimal { field, value })
        })
        .transpose()
}
fn yes(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "yes" | "true" | "1"
    )
}

fn parse_gaeb90(input: &str, options: &ParseOptions) -> Result<Document, GaebError> {
    let mut project = ProjectInfo::default();
    let mut categories = Vec::new();
    let mut category_path = Vec::new();
    let mut current_item: Option<Item> = None;
    let mut count = 0;
    let mut phase = ExchangePhase::X83;
    let mut header_style = false;
    let mut section_heading = false;
    let mut last_category: Option<Vec<usize>> = None;
    let mut date = None;
    let mut program = None;
    let mut boq_descriptions = Vec::new();
    let mut warnings = Vec::new();

    for line in input.lines() {
        let line = line.trim_end_matches('\r');
        if line.len() < 2 {
            continue;
        }
        let (kind, content) = line.split_at(2);
        match kind {
            "T0" | "T9" => {}
            "70" => {
                if program.is_none() {
                    program = Some(gaeb90_text(content));
                }
            }
            "00" => {
                if let Some(code) = gaeb90_field(content, 8, 11)
                    .and_then(|value| value.trim_end_matches('L').parse::<u8>().ok())
                {
                    phase = match code {
                        81 => ExchangePhase::X81,
                        82 => ExchangePhase::X82,
                        83 => ExchangePhase::X83,
                        84 => ExchangePhase::X84,
                        85 => ExchangePhase::X85,
                        86 => ExchangePhase::X86,
                        _ => {
                            return Err(GaebError::Unsupported(format!(
                                "unsupported GAEB 90 exchange phase: {code}"
                            )));
                        }
                    };
                    header_style = true;
                    project.boq_id = non_empty_gaeb90_field(content, 62, 72);
                } else {
                    project.id = non_empty_gaeb90_field(content, 0, 20);
                    project.name = Some(gaeb90_text(content.get(20..).unwrap_or_default()));
                }
            }
            "01" => {
                if gaeb90_field(content, 40, 48).is_some_and(|value| value.trim().len() == 8) {
                    header_style = true;
                    project.name = non_empty_gaeb90_field(content, 0, 40);
                    date = non_empty_gaeb90_field(content, 40, 48);
                } else {
                    push_category(
                        &mut categories,
                        &mut category_path,
                        &mut last_category,
                        gaeb90_field(content, 0, 8).unwrap_or_default().trim(),
                        gaeb90_category_level(content, section_heading),
                    );
                }
            }
            "02" => {
                if header_style && category_path.is_empty() && current_item.is_none() {
                    project.name = Some(gaeb90_text(content));
                    continue;
                }
                finish_gaeb90_item(&mut categories, &category_path, &mut current_item);
                count += 1;
                if count > options.max_items {
                    return Err(GaebError::ItemLimit {
                        limit: options.max_items,
                    });
                }
                current_item = Some(Item {
                    oz: gaeb90_field(content, 0, 14)
                        .unwrap_or_default()
                        .trim()
                        .to_owned(),
                    ..Item::default()
                });
            }
            "03" => {
                let text = gaeb90_text(content);
                if let Some(item) = &mut current_item {
                    append_gaeb90_text(&mut item.short_text, &text);
                } else if header_style {
                    project.label = Some(gaeb90_text(content));
                } else {
                    add_gaeb90_warning(&mut warnings, kind, &text);
                }
            }
            "04" => add_gaeb90_warning(&mut warnings, kind, content.trim()),
            "05" => {
                if let Some(item) = &mut current_item {
                    item.quantity = decimal(non_empty_gaeb90_field(content, 0, 14), "Qty")?;
                    item.unit = non_empty_gaeb90_field(content, 14, 24);
                }
            }
            "06" => {
                if let Some(item) = &mut current_item {
                    item.unit_price = decimal(non_empty_gaeb90_field(content, 0, 14), "UP")?;
                } else {
                    add_gaeb90_warning(&mut warnings, kind, content.trim());
                }
            }
            "07" => {
                if let Some(item) = &mut current_item {
                    item.total_price = decimal(non_empty_gaeb90_field(content, 0, 14), "IT")?;
                }
            }
            "08" => {
                project.currency = non_empty_gaeb90_field(content, 0, 6);
                project.currency_label = non_empty_gaeb90_field(content, 6, 16);
            }
            "10" => {
                finish_gaeb90_item(&mut categories, &category_path, &mut current_item);
                section_heading = true;
                push_category(
                    &mut categories,
                    &mut category_path,
                    &mut last_category,
                    "",
                    1,
                );
                if let Some(path) = &last_category {
                    let category = category_at_path(&mut categories, path);
                    category.label = gaeb90_text(content);
                    category.rno = gaeb90_field(content, 0, 9)
                        .unwrap_or_default()
                        .trim()
                        .to_owned();
                }
            }
            "11" => {
                finish_gaeb90_item(&mut categories, &category_path, &mut current_item);
                push_category(
                    &mut categories,
                    &mut category_path,
                    &mut last_category,
                    gaeb90_field(content, 0, 9).unwrap_or_default().trim(),
                    gaeb90_category_level(content, section_heading),
                );
            }
            "12" => {
                let text = gaeb90_text(content);
                if let Some(path) = &last_category {
                    category_at_path(&mut categories, path).label = text;
                } else {
                    add_gaeb90_warning(&mut warnings, kind, content.trim());
                }
            }
            "20" => finish_gaeb90_item(&mut categories, &category_path, &mut current_item),
            "21" => {
                finish_gaeb90_item(&mut categories, &category_path, &mut current_item);
                count += 1;
                if count > options.max_items {
                    return Err(GaebError::ItemLimit {
                        limit: options.max_items,
                    });
                }
                let marker = gaeb90_field(content, 12, 21).unwrap_or_default();
                current_item = Some(Item {
                    oz: gaeb90_field(content, 0, 9)
                        .unwrap_or_default()
                        .trim()
                        .to_owned(),
                    source_item_kind: non_empty_gaeb90_field(content, 9, 12),
                    quantity: gaeb90_scaled_decimal(content, 21, 32, 3, "Qty")?,
                    unit: non_empty_gaeb90_field(content, 32, 42),
                    quantity_tbd: marker.contains('X'),
                    ..Item::default()
                });
            }
            "23" => {
                let oz = gaeb90_field(content, 0, 9).unwrap_or_default();
                let oz = oz.trim();
                if current_item.as_ref().is_some_and(|item| item.oz == oz) {
                    let item = current_item.as_mut().expect("checked above");
                    item.unit_price = gaeb90_scaled_decimal(content, 12, 22, 2, "UP")?;
                    item.total_price = gaeb90_scaled_decimal(content, 22, 34, 2, "IT")?;
                } else {
                    add_gaeb90_warning(&mut warnings, kind, content.trim());
                }
            }
            "25" => {
                let text = gaeb90_text(content);
                if let Some(item) = &mut current_item {
                    append_gaeb90_text(&mut item.short_text, &text);
                } else if let Some(path) = &last_category {
                    category_at_path(&mut categories, path).label = text;
                }
            }
            "26" => {
                let text = gaeb90_text(content);
                if let Some(item) = &mut current_item {
                    append_gaeb90_text(&mut item.long_text, &text);
                } else if let Some(path) = &last_category {
                    category_at_path(&mut categories, path)
                        .performance_descriptions
                        .push(text.to_owned());
                } else {
                    boq_descriptions.push(text.to_owned());
                }
            }
            "31" | "33" => {}
            "32" => {
                let rno = gaeb90_field(content, 0, 9).unwrap_or_default();
                let rno = rno.trim();
                if let Some(category) = find_category_mut(&mut categories, rno) {
                    category.declared_total =
                        gaeb90_scaled_decimal(content, 10, 22, 2, "category total")?;
                }
            }
            "99" => {
                finish_gaeb90_item(&mut categories, &category_path, &mut current_item);
                add_gaeb90_warning(&mut warnings, kind, content.trim());
                break;
            }
            _ => {
                return Err(GaebError::Unsupported(format!(
                    "unsupported GAEB 90 record: {kind}"
                )));
            }
        }
    }
    finish_gaeb90_item(&mut categories, &category_path, &mut current_item);
    Ok(Document::Procurement(ProcurementDocument {
        meta: DocumentMeta {
            version: SourceVersion::Gaeb90,
            phase,
            date,
            program,
            warnings,
            ..DocumentMeta::default()
        },
        project,
        boq: Boq {
            categories,
            performance_descriptions: boq_descriptions,
            ..Boq::default()
        },
        invoice: None,
    }))
}

fn gaeb90_text(value: &str) -> String {
    let chars: Vec<char> = value.trim().chars().collect();
    let value: String = if chars.len() >= 6
        && chars[chars.len() - 6..]
            .iter()
            .all(|character| character.is_ascii_digit())
    {
        chars[..chars.len() - 6].iter().collect()
    } else {
        chars.iter().collect()
    };
    value.trim().to_owned()
}

fn decode_gaeb90(input: &[u8]) -> String {
    input
        .iter()
        .map(|byte| {
            if *byte < 0x80 {
                char::from(*byte)
            } else {
                CP850_HIGH
                    .chars()
                    .nth(usize::from(*byte - 0x80))
                    .unwrap_or('\u{fffd}')
            }
        })
        .collect()
}

const CP850_HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜø£Ø×ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■ ";

fn gaeb90_field(value: &str, start: usize, end: usize) -> Option<String> {
    let chars: Vec<char> = value.chars().collect();
    if start >= chars.len() {
        return None;
    }
    Some(chars[start..end.min(chars.len())].iter().collect())
}

fn non_empty_gaeb90_field(value: &str, start: usize, end: usize) -> Option<String> {
    non_empty(gaeb90_field(value, start, end).as_deref())
}

fn gaeb90_scaled_decimal(
    value: &str,
    start: usize,
    end: usize,
    scale: u32,
    field: &'static str,
) -> Result<Option<Decimal>, GaebError> {
    gaeb90_field(value, start, end)
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            let digits = value.trim();
            if !digits.chars().all(|character| character.is_ascii_digit()) {
                return Err(GaebError::InvalidDecimal {
                    field,
                    value: digits.to_owned(),
                });
            }
            Decimal::from_str_exact(digits)
                .map(|value| value / Decimal::new(10_i64.pow(scale), 0))
                .map_err(|_| GaebError::InvalidDecimal {
                    field,
                    value: digits.to_owned(),
                })
        })
        .transpose()
}

fn gaeb90_category_level(value: &str, section_heading: bool) -> usize {
    let rno = gaeb90_field(value, 0, 9).unwrap_or_default();
    let groups = if rno.trim().contains(' ') {
        rno.split_whitespace().count().max(1)
    } else {
        (rno.trim().len() / 2).max(1)
    };
    if section_heading { groups + 1 } else { groups }
}

fn category_at_path<'a>(categories: &'a mut [BoqCategory], path: &[usize]) -> &'a mut BoqCategory {
    let category = &mut categories[path[0]];
    if path.len() == 1 {
        category
    } else {
        category_at_path(&mut category.categories, &path[1..])
    }
}

fn push_category(
    categories: &mut Vec<BoqCategory>,
    path: &mut Vec<usize>,
    last_category: &mut Option<Vec<usize>>,
    rno: &str,
    level: usize,
) {
    while path.len() >= level.max(1) {
        path.pop();
    }
    let category = BoqCategory {
        rno: rno.to_owned(),
        ..BoqCategory::default()
    };
    let index = if let Some(parent_path) = (!path.is_empty()).then_some(path.as_slice()) {
        let parent = category_at_path(categories, parent_path);
        parent.categories.push(category);
        parent.categories.len() - 1
    } else {
        categories.push(category);
        categories.len() - 1
    };
    path.push(index);
    *last_category = Some(path.clone());
}

fn find_category_mut<'a>(
    categories: &'a mut [BoqCategory],
    rno: &str,
) -> Option<&'a mut BoqCategory> {
    for category in categories {
        if category.rno.trim() == rno {
            return Some(category);
        }
        if let Some(found) = find_category_mut(&mut category.categories, rno) {
            return Some(found);
        }
    }
    None
}

fn finish_gaeb90_item(categories: &mut Vec<BoqCategory>, path: &[usize], item: &mut Option<Item>) {
    let Some(item) = item.take() else {
        return;
    };
    if path.is_empty() {
        categories.push(BoqCategory {
            items: vec![item],
            ..BoqCategory::default()
        });
    } else {
        category_at_path(categories, path).items.push(item);
    }
}

fn append_gaeb90_text(target: &mut String, value: &str) {
    if !value.is_empty() {
        if !target.is_empty() {
            target.push(' ');
        }
        target.push_str(value);
    }
}

fn add_gaeb90_warning(warnings: &mut Vec<String>, kind: &str, content: &str) {
    warnings.push(format!(
        "GAEB 90 record {kind} not represented in common model: {content}"
    ));
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn write_document<W: Write>(
    document: &Document,
    mut output: W,
    options: &WriteOptions,
) -> Result<WriteReport, GaebError> {
    if options.phase == ExchangePhase::X95 {
        return Err(GaebError::InvalidWriteProfile(
            "X95 is not a released GAEB DA XML phase".to_owned(),
        ));
    }
    if options.version == SourceVersion::DaXml33 && !options.phase.is_current_da_xml_33() {
        return Err(GaebError::InvalidWriteProfile(format!(
            "{} is not defined by GAEB DA XML 3.3",
            options.phase.code()
        )));
    }
    if options.phase.kind() != document.kind() {
        return Err(GaebError::InvalidWriteProfile(format!(
            "{} cannot write a {:?} document",
            options.phase.code(),
            document.kind()
        )));
    }
    if options.version == SourceVersion::Gaeb90 {
        let (output_bytes, omitted_fields) = write_gaeb90(document, options.phase)?;
        output.write_all(&output_bytes)?;
        return Ok(WriteReport {
            warnings: vec!["GAEB 90 output uses fixed-width CP850 records".to_owned()],
            omitted_fields,
        });
    }
    let root = write_xml_node(document, options);
    let mut writer = if options.indent {
        Writer::new_with_indent(Vec::new(), b' ', 2)
    } else {
        Writer::new(Vec::new())
    };
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;
    emit_node(&mut writer, &root, is_german_version(options.version))?;
    output.write_all(&writer.into_inner())?;
    let warnings = if document.phase() != options.phase {
        vec![format!(
            "source phase {} written as {}",
            document.phase().code(),
            options.phase.code()
        )]
    } else {
        Vec::new()
    };
    Ok(WriteReport {
        warnings,
        omitted_fields: omitted_fields(document, options.phase),
    })
}

fn omitted_fields(document: &Document, phase: ExchangePhase) -> Vec<String> {
    match document {
        Document::Procurement(document) => document
            .boq
            .items()
            .flat_map(|item| {
                let mut omitted = Vec::new();
                let add = |omitted: &mut Vec<String>, field: &str| {
                    omitted.push(format!("Item {}: {field}", item.oz));
                };
                if !item.extensions.is_empty() {
                    omitted.push(format!(
                        "Item {}: unsupported fields {}",
                        item.oz,
                        item.extensions
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(",")
                    ));
                }
                if !item.bidder_prices.is_empty() {
                    add(&mut omitted, "bidder prices");
                }
                if item.bill_quantity.is_some()
                    && !matches!(
                        phase,
                        ExchangePhase::X52
                            | ExchangePhase::X80
                            | ExchangePhase::X81
                            | ExchangePhase::X82
                    )
                {
                    add(&mut omitted, "bill quantity");
                }
                if item.predicted_quantity.is_some()
                    && matches!(
                        phase,
                        ExchangePhase::X83 | ExchangePhase::X84 | ExchangePhase::X85
                    )
                {
                    add(&mut omitted, "predicted quantity");
                }
                if item.discount_pct.is_some() && phase == ExchangePhase::X83 {
                    add(&mut omitted, "discount percentage");
                }
                if phase == ExchangePhase::X83 {
                    if item.unit_price.is_some() {
                        add(&mut omitted, "unit price");
                    }
                    if item.total_price.is_some() {
                        add(&mut omitted, "total price");
                    }
                    if !item.up_components.is_empty() {
                        add(&mut omitted, "unit price components");
                    }
                }
                if phase == ExchangePhase::X84
                    && (!matches!(item.item_type, ItemType::Normal | ItemType::Markup)
                        || item.reference_description.is_some()
                        || item.reference_rno.is_some()
                        || item.reference_performance.is_some()
                        || item.hour_item
                        || item.markup_item
                        || item.up_breakdown)
                {
                    add(&mut omitted, "X84 item fields");
                }
                if !supports_change_order_fields(phase)
                    && (item.change_order_number.is_some() || item.co_status.is_some())
                {
                    add(&mut omitted, "change order fields");
                }
                if !item.cost_approaches.is_empty() && phase != ExchangePhase::X52 {
                    add(&mut omitted, "cost approaches");
                }
                if item.catalog_number.is_some() && phase != ExchangePhase::X86ZE {
                    add(&mut omitted, "catalog number");
                }
                if item.unit.is_some()
                    && matches!(
                        phase,
                        ExchangePhase::X84 | ExchangePhase::X84Z | ExchangePhase::X86ZE
                    )
                {
                    add(&mut omitted, "unit");
                }
                if item.quantity.is_some()
                    && (matches!(
                        phase,
                        ExchangePhase::X84Z | ExchangePhase::X83Z | ExchangePhase::X86ZR
                    ) || (phase == ExchangePhase::X83 && item.quantity_tbd))
                {
                    add(&mut omitted, "quantity");
                }
                if !item.qty_splits.is_empty()
                    && matches!(
                        phase,
                        ExchangePhase::X84
                            | ExchangePhase::X83Z
                            | ExchangePhase::X84Z
                            | ExchangePhase::X86ZR
                    )
                {
                    add(&mut omitted, "quantity splits");
                }
                if phase == ExchangePhase::X84
                    && (!item.short_text.is_empty() || !item.long_text.is_empty())
                {
                    add(&mut omitted, "description");
                }
                if matches!(phase, ExchangePhase::X84Z | ExchangePhase::X86ZE)
                    && (!item.short_text.is_empty()
                        || !item.long_text.is_empty()
                        || !item.text_complements.is_empty())
                {
                    add(&mut omitted, "description");
                }
                if phase == ExchangePhase::X84
                    && item.sub_descriptions.iter().any(|sub| {
                        !sub.short_text.is_empty()
                            || !sub.long_text.is_empty()
                            || !sub.text_complements.is_empty()
                    })
                {
                    add(&mut omitted, "subdescription text");
                }
                if (item.up_from.is_some() || item.up_avg.is_some() || item.up_to.is_some())
                    && !matches!(
                        phase,
                        ExchangePhase::X52
                            | ExchangePhase::X80
                            | ExchangePhase::X81
                            | ExchangePhase::X82
                    )
                {
                    add(&mut omitted, "unit price range");
                }
                if phase == ExchangePhase::X84 && item.sum_description {
                    add(&mut omitted, "sum description");
                }
                if !item.text_complements.is_empty()
                    && matches!(
                        phase,
                        ExchangePhase::X83Z
                            | ExchangePhase::X84Z
                            | ExchangePhase::X86ZR
                            | ExchangePhase::X86ZE
                    )
                {
                    add(&mut omitted, "text complement metadata");
                }
                if phase == ExchangePhase::X84 && !item.qty_splits.is_empty() {
                    add(&mut omitted, "quantity splits");
                }
                if item.not_applicable && !supports_not_applicable(phase) {
                    add(&mut omitted, "not applicable");
                }
                if matches!(
                    item.source_item_kind.as_deref(),
                    Some("HourItem" | "MaterialItem" | "MarkupItemOWN" | "MarkupItemCTR")
                ) && matches!(phase, ExchangePhase::X84Z | ExchangePhase::X86ZE)
                    && (!item.short_text.is_empty()
                        || !item.long_text.is_empty()
                        || !item.text_complements.is_empty())
                {
                    add(&mut omitted, "description");
                }
                if item.quantity.is_some()
                    && matches!(
                        item.source_item_kind.as_deref(),
                        Some("HourItem" | "MaterialItem" | "MarkupItemOWN" | "MarkupItemCTR")
                    )
                    && (phase == ExchangePhase::X84Z
                        || (phase == ExchangePhase::X86ZE
                            && item.source_item_kind.as_deref() != Some("HourItem")))
                {
                    add(&mut omitted, "quantity");
                }
                if item.unit.is_some()
                    && matches!(phase, ExchangePhase::X84Z | ExchangePhase::X86ZE)
                    && matches!(
                        item.source_item_kind.as_deref(),
                        Some("HourItem" | "MaterialItem" | "MarkupItemOWN" | "MarkupItemCTR")
                    )
                {
                    add(&mut omitted, "unit");
                }
                omitted.into_iter()
            })
            .collect(),
        Document::Quantity(_) => Vec::new(),
        Document::Trade(_) => Vec::new(),
        Document::Structured(_) => Vec::new(),
        Document::Cost(document) => document
            .items
            .iter()
            .flat_map(cost_omitted_fields)
            .collect(),
    }
}

fn cost_omitted_fields(item: &CostItem) -> Vec<String> {
    let mut omitted = Vec::new();
    if item.portion.is_some() {
        omitted.push(format!("Cost element {}: portion", item.id));
    }
    if item.declared_total.is_some() {
        omitted.push(format!("Cost element {}: declared total", item.id));
    }
    for child in &item.children {
        omitted.extend(cost_omitted_fields(child));
    }
    omitted
}

fn is_german_version(version: SourceVersion) -> bool {
    matches!(version, SourceVersion::DaXml20 | SourceVersion::DaXml21)
}
fn write_xml_node(document: &Document, options: &WriteOptions) -> Node {
    let mut root = element("GAEB");
    root.attributes.insert(
        "xmlns".to_owned(),
        namespace(options.version, options.phase),
    );
    let meta = document.meta();
    let mut info = element("GAEBInfo");
    text_child(&mut info, "Version", options.version.number());
    text_child(
        &mut info,
        "VersDate",
        schema_release(options.version, options.phase),
    );
    text_child_opt(
        &mut info,
        "Date",
        meta.date.as_deref().or(Some("1970-01-01")),
    );
    let is_time_contract = matches!(
        options.phase,
        ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
    );
    text_child_opt(
        &mut info,
        "Time",
        meta.time.as_deref().or(if is_time_contract {
            Some("00:00:00")
        } else {
            None
        }),
    );
    text_child_opt(
        &mut info,
        "ProgSystem",
        meta.program.as_deref().or(if is_time_contract {
            Some("gaeb-core")
        } else {
            None
        }),
    );
    if is_time_contract {
        text_child(&mut info, "ProgName", "gaeb-core");
    }
    root.children.push(info);
    match document {
        Document::Procurement(doc) => write_procurement(&mut root, doc, options.phase),
        Document::Trade(doc) => write_trade(&mut root, doc, options.phase),
        Document::Cost(doc) => write_cost(&mut root, doc, options.phase),
        Document::Quantity(doc) => write_quantity(&mut root, doc, options.phase),
        Document::Structured(doc) => {
            root.children
                .extend(doc.elements.iter().map(node_from_xml_element));
        }
    }
    root
}
fn write_procurement(root: &mut Node, document: &ProcurementDocument, phase: ExchangePhase) {
    if matches!(phase, ExchangePhase::X89 | ExchangePhase::X89B) {
        write_invoice(root, document, phase);
        return;
    }
    if !matches!(
        phase,
        ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
    ) {
        let mut project = element("PrjInfo");
        if phase == ExchangePhase::X84 {
            text_child(
                &mut project,
                "NamePrj",
                document.project.name.as_deref().unwrap_or("UNSPECIFIED"),
            );
            text_child_opt(&mut project, "PrjID", document.project.id.as_deref());
            text_child_opt(&mut project, "LblPrj", document.project.label.as_deref());
        } else {
            text_child_opt(&mut project, "PrjID", document.project.id.as_deref());
            text_child_opt(&mut project, "NamePrj", document.project.name.as_deref());
            text_child_opt(&mut project, "LblPrj", document.project.label.as_deref());
        }
        if phase != ExchangePhase::X84 {
            text_child_opt(&mut project, "Cur", document.project.currency.as_deref());
            text_child_opt(
                &mut project,
                "CurLbl",
                document.project.currency_label.as_deref(),
            );
        }
        if !project.children.is_empty() {
            root.children.push(project);
        }
    }
    let mut award = element("Award");
    text_child(&mut award, "DP", phase.code().trim_start_matches('X'));
    let mut award_info = element("AwardInfo");
    if !matches!(
        phase,
        ExchangePhase::X85 | ExchangePhase::X86 | ExchangePhase::X87
    ) {
        text_child_opt(&mut award_info, "BoQID", document.project.boq_id.as_deref());
    }
    if phase == ExchangePhase::X84 {
        text_child_opt(&mut award_info, "Cur", document.project.currency.as_deref());
        text_child_opt(
            &mut award_info,
            "CurLbl",
            document.project.currency_label.as_deref(),
        );
    }
    write_time_contract_award_info(&mut award_info, phase);
    if !award_info.children.is_empty()
        || matches!(
            phase,
            ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
        )
    {
        award.children.push(award_info);
    }
    if matches!(
        phase,
        ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
    ) {
        match phase {
            ExchangePhase::X83Z => {
                award.children.push(time_contract_party("OWN", true));
                award.children.push(required_party("CnstSite"));
            }
            ExchangePhase::X84Z => {
                award.children.push(time_contract_party("OWN", false));
                award.children.push(required_party("CTR"));
            }
            ExchangePhase::X86ZR => {
                award.children.push(time_contract_party("OWN", true));
                award.children.push(required_party("CTR"));
                award.children.push(required_party("CnstSite"));
            }
            ExchangePhase::X86ZE => {
                award.children.push(element("IndivAgrInfo"));
                award.children.push(time_contract_party("OWN", false));
                award.children.push(required_party("CnstSite"));
            }
            _ => unreachable!(),
        }
        award.children.push(write_boq(&document.boq, phase));
        root.children.push(award);
        return;
    }
    if phase == ExchangePhase::X84 {
        award.children.push(required_party("CTR"));
    }
    if matches!(phase, ExchangePhase::X86 | ExchangePhase::X87) {
        award.children.push(required_party("OWN"));
        award.children.push(required_party("CTR"));
        award.children.push(required_party("CnstSite"));
    }
    award.children.push(write_boq(&document.boq, phase));
    root.children.push(award);
}
fn write_time_contract_award_info(info: &mut Node, phase: ExchangePhase) {
    match phase {
        ExchangePhase::X84Z => text_child(info, "BidDate", "1970-01-01"),
        ExchangePhase::X86ZE => {
            text_child(info, "ContrNo", "UNSPECIFIED");
            text_child(info, "ContrDate", "1970-01-01");
        }
        ExchangePhase::X83Z | ExchangePhase::X86ZR => {
            text_child(info, "Cat", "OpenCall");
            text_child(info, "Cur", "EUR");
            if phase == ExchangePhase::X86ZR {
                text_child(info, "BidDate", "1970-01-01");
            }
            let mut agreement = element("MastAgrInfo");
            text_child(&mut agreement, "BidUpDown", "Yes");
            agreement
                .children
                .push(span_text("MastAgrLbl", "UNSPECIFIED"));
            let mut description = element("Descrip");
            let mut div = element("div");
            div.children.push(Node {
                name: "span".to_owned(),
                text: "UNSPECIFIED".to_owned(),
                ..Node::default()
            });
            description.children.push(div);
            agreement.children.push(description);
            text_child(&mut agreement, "MastAgrBeg", "1970-01-01");
            text_child(&mut agreement, "MastAgrEnd", "1970-01-01");
            if phase == ExchangePhase::X86ZR {
                text_child(&mut agreement, "MastAgrDate", "1970-01-01");
            }
            info.children.push(agreement);
        }
        _ => {}
    }
}

fn time_contract_party(name: &str, address: bool) -> Node {
    let mut party = element(name);
    if address {
        let mut value = element("Address");
        for field in ["Name1", "Street", "PCode", "City"] {
            value.children.push(element(field));
        }
        party.children.push(value);
    }
    text_child(&mut party, "AwardNo", "UNSPECIFIED");
    party
}

fn category_total(category: &BoqCategory) -> Decimal {
    category
        .items
        .iter()
        .filter(|item| item.item_type.affects_total())
        .filter_map(|item| item.total_price)
        .sum::<Decimal>()
        + category
            .categories
            .iter()
            .map(category_total)
            .sum::<Decimal>()
}

fn write_invoice(root: &mut Node, document: &ProcurementDocument, phase: ExchangePhase) {
    let data = document.invoice.as_ref();
    let mut invoice = element("Invoice");
    text_child(&mut invoice, "DP", phase.code().trim_start_matches('X'));
    if let Some(party) = data.and_then(|value| value.owner.as_ref()) {
        invoice.children.push(write_invoice_party("OWN", party));
    }
    if let Some(data) = data {
        for requester in &data.requesters {
            invoice
                .children
                .push(write_invoice_party("Requester", requester));
        }
    }
    if let Some(party) = data.and_then(|value| value.contractor.as_ref()) {
        invoice.children.push(write_invoice_party("CTR", party));
    }
    if let Some(data) = data {
        for (construction, notification) in data
            .construction_sites
            .iter()
            .zip(data.notification_sites.iter())
        {
            invoice.children.push(write_invoice_site(
                "CnstSite",
                construction,
                "CnstSiteIDNo",
                "CnstSiteName",
            ));
            invoice.children.push(write_invoice_site(
                "NotifSite",
                notification,
                "NotifSiteIDNo",
                "NotifSiteName",
            ));
        }
        for construction in data
            .construction_sites
            .iter()
            .skip(data.notification_sites.len())
        {
            invoice.children.push(write_invoice_site(
                "CnstSite",
                construction,
                "CnstSiteIDNo",
                "CnstSiteName",
            ));
        }
        for notification in data
            .notification_sites
            .iter()
            .skip(data.construction_sites.len())
        {
            invoice.children.push(write_invoice_site(
                "NotifSite",
                notification,
                "NotifSiteIDNo",
                "NotifSiteName",
            ));
        }
        for text in &data.additional_texts {
            invoice.children.push(write_invoice_text(text));
        }
    }
    if document.boq.items().next().is_some() {
        invoice.children.push(write_boq(&document.boq, phase));
    }

    let header = data
        .and_then(|value| value.header.as_ref())
        .map(|value| write_invoice_header(value, phase))
        .unwrap_or_else(|| {
            let value = InvoiceHeader {
                invoice_no: (phase == ExchangePhase::X89).then(|| "UNSPECIFIED".to_owned()),
                invoice_date: (phase == ExchangePhase::X89).then(|| "1970-01-01".to_owned()),
                invoice_type: (phase == ExchangePhase::X89).then(|| "single invoice".to_owned()),
                service_provision_start_date: "1970-01-01".to_owned(),
                service_provision_end_date: "1970-01-01".to_owned(),
                ..InvoiceHeader::default()
            };
            write_invoice_header(&value, phase)
        });
    invoice.children.push(header);

    invoice.children.push(
        data.and_then(|value| value.creator.as_ref())
            .map(|party| write_invoice_party("InvoiceCreator", party))
            .unwrap_or_else(|| invoice_party("InvoiceCreator", true)),
    );
    if phase == ExchangePhase::X89 {
        invoice.children.push(
            data.and_then(|value| value.recipient.as_ref())
                .map(|party| write_invoice_party("InvoiceRecipient", party))
                .unwrap_or_else(|| invoice_party("InvoiceRecipient", false)),
        );
        if let Some(data) = data {
            for note in &data.check_notes {
                let mut value = element("CheckNote");
                text_child(&mut value, "State", &note.state);
                text_child(&mut value, "CheckDate", &note.check_date);
                text_child(&mut value, "Annotation", &note.annotation);
                invoice.children.push(value);
            }
            text_child_opt(
                &mut invoice,
                "BudgetDept",
                data.budget_department.as_deref(),
            );
            text_child_opt(&mut invoice, "BookingNo", data.booking_number.as_deref());
            text_child_opt(
                &mut invoice,
                "QuantityDeterminationRemark",
                data.quantity_determination_remark.as_deref(),
            );
            for share in &data.shares {
                invoice.children.push(write_invoice_share(share));
            }
            for term in &data.payment_terms {
                invoice.children.push(write_invoice_payment_term(term));
            }
            for payment in &data.payments_made {
                invoice.children.push(write_invoice_payment(payment));
            }
            for issued in &data.invoices_made_out {
                invoice.children.push(write_invoice_issued(issued));
            }
        }
        if data.is_none_or(|value| value.shares.is_empty()) {
            let mut share = element("InvoiceShare");
            text_child(&mut share, "InvoiceShareType", "basic amount");
            text_child(&mut share, "Description", "Invoice total");
            text_child(&mut share, "Total", &invoice_total(document).to_string());
            invoice.children.push(share);
        }
        text_child(
            &mut invoice,
            "TotalGross",
            &data
                .and_then(|value| value.total_gross)
                .unwrap_or_else(|| invoice_total(document))
                .to_string(),
        );
    }
    root.children.push(invoice);
}

fn write_invoice_header(header: &InvoiceHeader, phase: ExchangePhase) -> Node {
    let mut node = element("InvoiceHeader");
    if phase == ExchangePhase::X89 {
        text_child_opt(&mut node, "InvoiceNo", header.invoice_no.as_deref());
        text_child_opt(&mut node, "InvoiceDate", header.invoice_date.as_deref());
        text_child_opt(&mut node, "InvoiceType", header.invoice_type.as_deref());
    }
    if phase == ExchangePhase::X89B {
        text_child_opt(
            &mut node,
            "RefInvoiceNo",
            header.reference_invoice_no.as_deref(),
        );
    }
    if phase == ExchangePhase::X89 {
        if header.credit_note {
            text_child(&mut node, "CreditNote", "Yes");
        }
        text_child_opt(
            &mut node,
            "SettlementType",
            header.settlement_type.as_deref(),
        );
        if let Some(value) = header.sequential_no {
            text_child(&mut node, "SequentialNo", &value.to_string());
        }
    }
    text_child(
        &mut node,
        "ServiceProvisionStartDate",
        if header.service_provision_start_date.is_empty() {
            "1970-01-01"
        } else {
            &header.service_provision_start_date
        },
    );
    text_child(
        &mut node,
        "ServiceProvisionEndDate",
        if header.service_provision_end_date.is_empty() {
            "1970-01-01"
        } else {
            &header.service_provision_end_date
        },
    );
    node
}

fn write_invoice_party(name: &str, party: &InvoiceParty) -> Node {
    let mut node = element(name);
    if let Some(address) = &party.address {
        node.children.push(write_invoice_address(address));
    } else if matches!(name, "InvoiceCreator" | "InvoiceRecipient") {
        node.children.push(invoice_address());
    }
    match name {
        "OWN" => {
            text_child_opt(&mut node, "DPNo", party.dp_no.as_deref());
            text_child_opt(&mut node, "AwardNo", party.award_no.as_deref());
            text_child_opt(
                &mut node,
                "AcctRecNo",
                party.account_receivable_no.as_deref(),
            );
        }
        "CTR" => {
            text_child_opt(&mut node, "DPNo", party.dp_no.as_deref());
            text_child_opt(&mut node, "AwardNo", party.award_no.as_deref());
            text_child_opt(
                &mut node,
                "AcctsPayNo",
                party.accounts_payable_no.as_deref(),
            );
            text_child_opt(&mut node, "BidderNo", party.bidder_no.as_deref());
        }
        "Requester" => {
            text_child_opt(&mut node, "RequestIDNo", party.request_id_no.as_deref());
            text_child_opt(&mut node, "RequestName", party.request_name.as_deref());
        }
        _ => {}
    }
    if matches!(name, "OWN" | "CTR") {
        text_child_opt(&mut node, "CntryType", party.country_type.as_deref());
        text_child_opt(&mut node, "CntryName", party.country_name.as_deref());
    }
    if name == "CTR" {
        text_child_opt(&mut node, "SectorType", party.sector_type.as_deref());
        text_child_opt(
            &mut node,
            "PrefBidType",
            party.preferred_bidder_type.as_deref(),
        );
        text_child_opt(&mut node, "SCType", party.service_type.as_deref());
    }
    if name == "InvoiceCreator" {
        text_child(
            &mut node,
            "TaxNo",
            party.tax_no.as_deref().unwrap_or("UNSPECIFIED"),
        );
    } else if name == "InvoiceRecipient" {
        text_child_opt(&mut node, "TaxNo", party.tax_no.as_deref());
    }
    for text in &party.additional_texts {
        node.children.push(write_invoice_text(text));
    }
    node
}

fn invoice_address() -> Node {
    let mut address = element("Address");
    for field in ["Name1", "Street", "PCode", "City"] {
        text_child(&mut address, field, "UNSPECIFIED");
    }
    address
}

fn write_invoice_address(address: &InvoiceAddress) -> Node {
    let mut node = element("Address");
    text_child(&mut node, "Name1", &address.name1);
    text_child_opt(&mut node, "Name2", address.name2.as_deref());
    text_child_opt(&mut node, "Name3", address.name3.as_deref());
    text_child_opt(&mut node, "Name4", address.name4.as_deref());
    text_child(&mut node, "Street", &address.street);
    text_child(&mut node, "PCode", &address.postal_code);
    text_child(&mut node, "City", &address.city);
    text_child_opt(&mut node, "Country", address.country.as_deref());
    text_child_opt(&mut node, "ILN", address.iln.as_deref());
    text_child_opt(&mut node, "Contact", address.contact.as_deref());
    text_child_opt(&mut node, "Phone", address.phone.as_deref());
    text_child_opt(&mut node, "Fax", address.fax.as_deref());
    text_child_opt(&mut node, "Email", address.email.as_deref());
    text_child_opt(&mut node, "VATID", address.vat_id.as_deref());
    for bank in &address.banks {
        let mut value = element("Bank");
        text_child(&mut value, "LblBank", &bank.label);
        text_child(&mut value, "BRNo", &bank.bank_code);
        text_child(&mut value, "AcctNo", &bank.account_number);
        text_child_opt(&mut value, "BIC", bank.bic.as_deref());
        text_child_opt(&mut value, "IBAN", bank.iban.as_deref());
        node.children.push(value);
    }
    node
}

fn write_invoice_text(text: &InvoiceText) -> Node {
    let mut node = element("AddText");
    if let Some(outline) = &text.outline {
        let mut value = element("OutlineAddText");
        value.children.push(Node {
            name: "span".to_owned(),
            text: outline.clone(),
            ..Node::default()
        });
        node.children.push(value);
    }
    let mut detail = element("DetailAddText");
    let mut paragraph = element("p");
    paragraph.children.push(Node {
        name: "span".to_owned(),
        text: text.detail.clone(),
        ..Node::default()
    });
    detail.children.push(paragraph);
    node.children.push(detail);
    node
}

fn write_invoice_site(name: &str, site: &InvoiceSite, id_name: &str, name_name: &str) -> Node {
    let mut node = element(name);
    if let Some(address) = &site.address {
        node.children.push(write_invoice_address(address));
    }
    text_child_opt(&mut node, id_name, site.id_no.as_deref());
    text_child_opt(&mut node, name_name, site.name.as_deref());
    for text in &site.additional_texts {
        node.children.push(write_invoice_text(text));
    }
    node
}

fn write_invoice_share(share: &InvoiceShare) -> Node {
    let mut node = element("InvoiceShare");
    text_child(&mut node, "InvoiceShareType", &share.share_type);
    text_child(&mut node, "Description", &share.description);
    if let Some(total) = share.total {
        text_child(&mut node, "Total", &total.to_string());
    } else if let Some(percent) = share.percent {
        text_child(&mut node, "Percent", &percent.to_string());
    } else {
        text_child(&mut node, "Total", "0");
    }
    if share.counter_claim {
        text_child(&mut node, "CounterClaim", "Yes");
    }
    node
}

fn write_invoice_payment_term(term: &InvoicePaymentTerm) -> Node {
    let mut node = element("Terms");
    if let Some(days) = term.cash_discount_days {
        let mut value = element("CashDiscDays");
        value.attributes.insert("Days".to_owned(), days.to_string());
        attribute_opt(&mut value, "TypeOfDays", term.days_type.as_deref());
        node.children.push(value);
    } else {
        text_child_opt(
            &mut node,
            "CashDiscDate",
            term.cash_discount_date.as_deref(),
        );
    }
    text_child(&mut node, "DiscountPcnt", &term.discount_pct.to_string());
    node
}

fn write_invoice_payment(payment: &InvoicePayment) -> Node {
    let mut node = element("PaymentMade");
    text_child(&mut node, "TotalVAT", &payment.total_vat.to_string());
    text_decimal(&mut node, "DiscountAmt", payment.discount_amount);
    text_decimal(&mut node, "DiscountVATAmt", payment.discount_vat_amount);
    text_child(&mut node, "Total", &payment.total.to_string());
    text_child(&mut node, "PaymentDate", &payment.payment_date);
    text_child_opt(&mut node, "InvoiceType", payment.invoice_type.as_deref());
    text_child(&mut node, "InvoiceNo", &payment.invoice_no);
    text_child_opt(&mut node, "PaymentNo", payment.payment_no.as_deref());
    text_child_opt(&mut node, "PaymentNote", payment.payment_note.as_deref());
    node
}

fn write_invoice_issued(issued: &InvoiceIssued) -> Node {
    let mut node = element("InvoiceMadeOut");
    text_child(&mut node, "TotalVAT", &issued.total_vat.to_string());
    text_child(&mut node, "Total", &issued.total.to_string());
    text_child(&mut node, "InvoiceDate", &issued.invoice_date);
    text_child(&mut node, "InvoiceType", &issued.invoice_type);
    text_child(&mut node, "InvoiceNo", &issued.invoice_no);
    node
}

fn invoice_total(document: &ProcurementDocument) -> Decimal {
    document
        .boq
        .items()
        .filter(|item| item.item_type.affects_total())
        .filter_map(|item| item.total_price)
        .sum()
}

fn invoice_party(name: &str, creator: bool) -> Node {
    let mut party = element(name);
    party.children.push(invoice_address());
    if creator {
        text_child(&mut party, "TaxNo", "UNSPECIFIED");
    }
    party
}

fn required_party(name: &str) -> Node {
    let mut party = element(name);
    let mut address = element("Address");
    for field in ["Name1", "Street", "PCode", "City"] {
        address.children.push(element(field));
    }
    party.children.push(address);
    party
}

fn write_boq(boq: &Boq, phase: ExchangePhase) -> Node {
    let mut generated_id = 0;
    let mut node = element("BoQ");
    if matches!(phase, ExchangePhase::X89 | ExchangePhase::X89B) {
        node.attributes.insert(
            "ID".to_owned(),
            boq.id.clone().unwrap_or_else(|| "BOQ1".to_owned()),
        );
    } else {
        attribute_opt(&mut node, "ID", boq.id.as_deref());
    }
    let mut info = element("BoQInfo");
    if matches!(phase, ExchangePhase::X89 | ExchangePhase::X89B) {
        text_child(
            &mut info,
            "Name",
            boq.name.as_deref().unwrap_or("UNSPECIFIED"),
        );
        text_child(
            &mut info,
            "LblBoQ",
            boq.name.as_deref().unwrap_or("UNSPECIFIED"),
        );
        text_child(&mut info, "OutlCompl", "AllTxt");
    } else if phase != ExchangePhase::X84Z {
        if matches!(
            phase,
            ExchangePhase::X80
                | ExchangePhase::X81
                | ExchangePhase::X82
                | ExchangePhase::X83
                | ExchangePhase::X84
                | ExchangePhase::X85
                | ExchangePhase::X86
                | ExchangePhase::X87
                | ExchangePhase::X83Z
                | ExchangePhase::X86ZR
                | ExchangePhase::X86ZE
        ) {
            text_child(
                &mut info,
                "Name",
                boq.name.as_deref().unwrap_or("UNSPECIFIED"),
            );
        } else {
            text_child_opt(&mut info, "Name", boq.name.as_deref());
        }
    }
    if !matches!(
        phase,
        ExchangePhase::X84 | ExchangePhase::X84Z | ExchangePhase::X89 | ExchangePhase::X89B
    ) {
        if matches!(
            phase,
            ExchangePhase::X80
                | ExchangePhase::X81
                | ExchangePhase::X82
                | ExchangePhase::X83
                | ExchangePhase::X85
                | ExchangePhase::X86
                | ExchangePhase::X87
        ) {
            text_child(
                &mut info,
                "LblBoQ",
                boq.name.as_deref().unwrap_or("UNSPECIFIED"),
            );
        } else {
            text_child_opt(&mut info, "LblBoQ", boq.name.as_deref());
        }
    }
    if phase == ExchangePhase::X86ZE {
        text_child(&mut info, "Date", "1970-01-01");
    }
    if !matches!(
        phase,
        ExchangePhase::X84
            | ExchangePhase::X83Z
            | ExchangePhase::X84Z
            | ExchangePhase::X86ZR
            | ExchangePhase::X86ZE
            | ExchangePhase::X89
            | ExchangePhase::X89B
    ) {
        text_child(&mut info, "OutlCompl", "AllTxt");
    }
    if !boq.lots.is_empty() {
        let mut breakdown = element("BoQBkdn");
        text_child(&mut breakdown, "Type", "Lot");
        text_child(&mut breakdown, "Length", "2");
        text_child(&mut breakdown, "Num", "Yes");
        info.children.push(breakdown);
    }
    let mut breakdown = element("BoQBkdn");
    text_child(&mut breakdown, "Type", "BoQLevel");
    text_child(&mut breakdown, "Length", "2");
    text_child(&mut breakdown, "Num", "Yes");
    info.children.push(breakdown);
    if matches!(phase, ExchangePhase::X89 | ExchangePhase::X89B) {
        let mut totals = element("Totals");
        text_child(
            &mut totals,
            "Total",
            &boq.totals
                .unwrap_or_else(|| {
                    boq.items()
                        .filter(|item| item.item_type.affects_total())
                        .filter_map(|item| item.total_price)
                        .sum::<Decimal>()
                })
                .to_string(),
        );
        info.children.push(totals);
    } else if !matches!(
        phase,
        ExchangePhase::X83
            | ExchangePhase::X83Z
            | ExchangePhase::X84Z
            | ExchangePhase::X86ZR
            | ExchangePhase::X86ZE
    ) && let Some(total) = boq.totals
    {
        let mut totals = element("Totals");
        text_child(&mut totals, "Total", &total.to_string());
        info.children.push(totals);
    }
    for cost_type in &boq.cost_types {
        let mut value = element("CostType");
        text_child(&mut value, "Name", &cost_type.name);
        text_child(&mut value, "Label", &cost_type.label);
        info.children.push(value);
    }
    if !info.children.is_empty() {
        node.children.push(info);
    }
    let mut body = element("BoQBody");
    if !matches!(phase, ExchangePhase::X84Z | ExchangePhase::X86ZE) {
        for remark in &boq.remarks {
            body.children
                .push(write_remark(remark, phase, &mut generated_id));
        }
        for description in &boq.performance_descriptions {
            body.children.push(write_performance_description(
                description,
                phase,
                &mut generated_id,
            ));
        }
    }
    if boq.lots.is_empty() {
        for category in &boq.categories {
            body.children
                .push(write_category(category, phase, &mut generated_id));
        }
    } else {
        for lot in &boq.lots {
            body.children.push(write_lot(lot, phase, &mut generated_id));
        }
    }
    node.children.push(body);
    node
}

fn write_lot(lot: &Lot, phase: ExchangePhase, generated_id: &mut usize) -> Node {
    let mut node = element("BoQCtgy");
    if matches!(phase, ExchangePhase::X89 | ExchangePhase::X89B) {
        *generated_id += 1;
        node.attributes.insert(
            "ID".to_owned(),
            lot.id.clone().unwrap_or_else(|| format!("L{generated_id}")),
        );
    } else {
        attribute_opt(&mut node, "ID", lot.id.as_deref());
    }
    attribute_opt(
        &mut node,
        "RNoPart",
        (!lot.rno.is_empty()).then_some(lot.rno.as_str()),
    );
    if !matches!(
        phase,
        ExchangePhase::X84 | ExchangePhase::X84Z | ExchangePhase::X86ZE
    ) {
        let mut label = element("LblTx");
        label.children.push(Node {
            name: "span".to_owned(),
            text: lot.label.clone(),
            ..Node::default()
        });
        node.children.push(label);
    }
    let mut body = element("BoQBody");
    if !matches!(phase, ExchangePhase::X84Z | ExchangePhase::X86ZE) {
        for remark in &lot.remarks {
            body.children
                .push(write_remark(remark, phase, generated_id));
        }
        for description in &lot.performance_descriptions {
            body.children.push(write_performance_description(
                description,
                phase,
                generated_id,
            ));
        }
    }
    for category in &lot.categories {
        body.children
            .push(write_category(category, phase, generated_id));
    }
    node.children.push(body);
    if let Some(total) = lot.declared_total {
        let mut totals = element("Totals");
        text_child(&mut totals, "Total", &total.to_string());
        node.children.push(totals);
    }
    node
}

fn write_category(category: &BoqCategory, phase: ExchangePhase, generated_id: &mut usize) -> Node {
    if category.rno.is_empty() && category.label.is_empty() && category.categories.is_empty() {
        return write_itemlist(category, phase, generated_id);
    }
    let mut node = element("BoQCtgy");
    if matches!(phase, ExchangePhase::X89 | ExchangePhase::X89B) {
        *generated_id += 1;
        node.attributes.insert(
            "ID".to_owned(),
            category
                .id
                .clone()
                .unwrap_or_else(|| format!("C{generated_id}")),
        );
    } else {
        attribute_opt(&mut node, "ID", category.id.as_deref());
    }
    if !category.rno.is_empty() {
        node.attributes
            .insert("RNoPart".to_owned(), category.rno.clone());
    }
    if !matches!(
        phase,
        ExchangePhase::X84 | ExchangePhase::X84Z | ExchangePhase::X86ZE
    ) {
        let mut label = element("LblTx");
        label.children.push(Node {
            name: "span".to_owned(),
            text: category.label.clone(),
            ..Node::default()
        });
        node.children.push(label);
    }
    let mut body = element("BoQBody");
    for child in &category.categories {
        body.children
            .push(write_category(child, phase, generated_id));
    }
    if !category.items.is_empty() {
        body.children
            .push(write_itemlist(category, phase, generated_id));
    } else {
        if !matches!(phase, ExchangePhase::X84Z | ExchangePhase::X86ZE) {
            for remark in &category.remarks {
                body.children
                    .push(write_remark(remark, phase, generated_id));
            }
        }
        if !matches!(phase, ExchangePhase::X84Z | ExchangePhase::X86ZE) {
            for description in &category.performance_descriptions {
                body.children.push(write_performance_description(
                    description,
                    phase,
                    generated_id,
                ));
            }
        }
    }
    if !body.children.is_empty() {
        node.children.push(body);
    }
    if phase == ExchangePhase::X83Z {
        text_child(
            &mut node,
            "BidUpDownReq",
            category.bid_up_down_request.as_deref().unwrap_or("No"),
        );
    }
    if matches!(phase, ExchangePhase::X89 | ExchangePhase::X89B)
        || (!matches!(
            phase,
            ExchangePhase::X83
                | ExchangePhase::X83Z
                | ExchangePhase::X84Z
                | ExchangePhase::X86ZR
                | ExchangePhase::X86ZE
        ) && (category.declared_total.is_some()
            || matches!(
                phase,
                ExchangePhase::X84 | ExchangePhase::X86 | ExchangePhase::X87
            )))
    {
        let mut totals = element("Totals");
        text_child(
            &mut totals,
            "Total",
            &category
                .declared_total
                .unwrap_or_else(|| category_total(category))
                .to_string(),
        );
        node.children.push(totals);
    }
    node
}
fn write_remark(value: &str, phase: ExchangePhase, generated_id: &mut usize) -> Node {
    *generated_id += 1;
    let mut remark = element("Remark");
    remark
        .attributes
        .insert("ID".to_owned(), format!("R{generated_id}"));
    let mut description = element("Description");
    let mut complete = element("CompleteText");
    let mut detail = element("DetailTxt");
    if phase == ExchangePhase::X84 {
        detail.children.push(text_complement(value));
    } else {
        detail.children.push(paragraph_text("Text", value));
    }
    complete.children.push(detail);
    if requires_outline(phase) {
        complete.children.push(outline_text(value));
    }
    description.children.push(complete);
    remark.children.push(description);
    remark
}

fn write_performance_description(
    value: &str,
    phase: ExchangePhase,
    generated_id: &mut usize,
) -> Node {
    *generated_id += 1;
    let mut performance = element("PerfDescr");
    performance
        .attributes
        .insert("ID".to_owned(), format!("P{generated_id}"));
    text_child(&mut performance, "PerfNo", "1");
    if phase != ExchangePhase::X84 {
        text_child(&mut performance, "PerfLbl", "UNSPECIFIED");
    }
    let mut description = element("Description");
    let mut complete = element("CompleteText");
    if phase == ExchangePhase::X84 {
        let mut detail = element("DetailTxt");
        detail.children.push(text_complement(value));
        complete.children.push(detail);
    } else {
        complete.children.push(element("DetailTxt"));
        complete.children.push(outline_text(value));
    }
    description.children.push(complete);
    performance.children.push(description);
    performance
}

fn write_itemlist(category: &BoqCategory, phase: ExchangePhase, generated_id: &mut usize) -> Node {
    let mut list = element("Itemlist");
    if !matches!(phase, ExchangePhase::X84Z | ExchangePhase::X86ZE) {
        for remark in &category.remarks {
            list.children
                .push(write_remark(remark, phase, generated_id));
        }
    }
    if !matches!(
        phase,
        ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
    ) {
        for description_text in &category.performance_descriptions {
            list.children.push(write_performance_description(
                description_text,
                phase,
                generated_id,
            ));
        }
    }
    let mut mast_items = element("MastAgrItems");
    for item in &category.items {
        if matches!(
            item.source_item_kind.as_deref(),
            Some("HourItem" | "MaterialItem" | "MarkupItemOWN" | "MarkupItemCTR")
        ) && matches!(
            phase,
            ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
        ) {
            mast_items.children.push(write_time_item(item, phase));
        } else {
            list.children.push(write_item(item, phase));
        }
    }
    if !mast_items.children.is_empty() {
        list.children.push(mast_items);
    }
    list
}

fn write_time_description(item: &Item, phase: ExchangePhase) -> Node {
    let mut description = element("Description");
    if phase == ExchangePhase::X86ZE {
        text_child(
            &mut description,
            "WICNo",
            item.catalog_number.as_deref().unwrap_or("0"),
        );
        return description;
    }
    if let Some(service) = &item.standard_service {
        if let Some(number) = &service.number {
            text_child(&mut description, "StLNo", number);
        } else {
            description.children.push(write_stlb(service));
        }
    }
    let mut complete = element("CompleteText");
    let mut detail = element("DetailTxt");
    if !item.long_text.is_empty() {
        detail
            .children
            .push(paragraph_text("Text", &item.long_text));
    }
    complete.children.push(detail);
    if !item.short_text.is_empty() || requires_outline(phase) {
        complete.children.push(outline_text(&item.short_text));
    }
    description.children.push(complete);
    description
}

fn write_time_item(item: &Item, phase: ExchangePhase) -> Node {
    let kind = match (item.source_item_kind.as_deref().unwrap_or("Item"), phase) {
        ("MarkupItemOWN", ExchangePhase::X84Z) => "MarkupItemCTR",
        (kind, _) => kind,
    };
    let mut node = element(kind);
    node.attributes.insert(
        "ID".to_owned(),
        item.id
            .clone()
            .unwrap_or_else(|| format!("I{}", item.oz.replace('.', "_"))),
    );
    node.attributes
        .insert("RNoPart".to_owned(), item.oz.clone());
    attribute_opt(&mut node, "RNoIndex", item.rno_index.as_deref());
    match kind {
        "HourItem" => {
            if phase == ExchangePhase::X84Z {
                text_child(
                    &mut node,
                    "UP",
                    &item.unit_price.unwrap_or(Decimal::ZERO).to_string(),
                );
            } else if phase == ExchangePhase::X86ZE {
                text_child(
                    &mut node,
                    "Qty",
                    &item.quantity.unwrap_or(Decimal::ZERO).to_string(),
                );
                node.children.push(write_time_description(item, phase));
            } else {
                text_child(
                    &mut node,
                    "PredQty",
                    &item.quantity.unwrap_or(Decimal::ZERO).to_string(),
                );
                text_child(&mut node, "QU", item.unit.as_deref().unwrap_or("h"));
                if phase == ExchangePhase::X86ZR {
                    text_child(
                        &mut node,
                        "UP",
                        &item.unit_price.unwrap_or(Decimal::ZERO).to_string(),
                    );
                }
                node.children.push(write_time_description(item, phase));
                for assign in &item.ctlg_assigns {
                    node.children.push(write_ctlg_assign(assign));
                }
            }
        }
        "MaterialItem" => {
            if phase == ExchangePhase::X86ZE {
                text_child(
                    &mut node,
                    "PredIT",
                    &item.total_price.unwrap_or(Decimal::ZERO).to_string(),
                );
                node.children.push(write_time_description(item, phase));
            } else if phase == ExchangePhase::X84Z {
                text_child(
                    &mut node,
                    "BidUpDownPct",
                    &item.bid_up_down_pct.unwrap_or(Decimal::ZERO).to_string(),
                );
            } else {
                text_child(
                    &mut node,
                    "PredIT",
                    &item.total_price.unwrap_or(Decimal::ZERO).to_string(),
                );
                node.children.push(write_time_description(item, phase));
                if phase == ExchangePhase::X86ZR {
                    text_child(
                        &mut node,
                        "BidUpDownPct",
                        &item.bid_up_down_pct.unwrap_or(Decimal::ZERO).to_string(),
                    );
                }
                for assign in &item.ctlg_assigns {
                    node.children.push(write_ctlg_assign(assign));
                }
            }
        }
        "MarkupItemOWN" | "MarkupItemCTR" => {
            if phase == ExchangePhase::X86ZE {
                if item.markup_sub_qtys.is_empty() {
                    node.children.push(write_ref_item("RefItem", "UNSPECIFIED"));
                } else {
                    for reference in &item.markup_sub_qtys {
                        let mut value = element("RefItem");
                        attribute_opt(&mut value, "IDRef", reference.ref_id.as_deref());
                        node.children.push(value);
                    }
                }
                node.children.push(write_time_description(item, phase));
            } else if phase == ExchangePhase::X84Z {
                text_child(
                    &mut node,
                    "Markup",
                    &item.unit_price.unwrap_or(Decimal::ZERO).to_string(),
                );
            } else {
                if kind == "MarkupItemOWN" || phase == ExchangePhase::X86ZR {
                    if matches!(phase, ExchangePhase::X83Z | ExchangePhase::X86ZR) {
                        text_child(
                            &mut node,
                            "Markup",
                            &item.unit_price.unwrap_or(Decimal::ZERO).to_string(),
                        );
                    } else {
                        text_decimal(&mut node, "Markup", item.unit_price);
                    }
                }
                node.children.push(write_time_description(item, phase));
                for assign in &item.ctlg_assigns {
                    node.children.push(write_ctlg_assign(assign));
                }
            }
        }
        _ => {}
    }
    node
}

fn supports_not_applicable(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X52
            | ExchangePhase::X80
            | ExchangePhase::X81
            | ExchangePhase::X82
            | ExchangePhase::X84
            | ExchangePhase::X86
            | ExchangePhase::X87
    )
}

fn supports_item_flags(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X52
            | ExchangePhase::X80
            | ExchangePhase::X81
            | ExchangePhase::X82
            | ExchangePhase::X83
            | ExchangePhase::X85
            | ExchangePhase::X86
            | ExchangePhase::X87
    )
}

fn supports_change_order_fields(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X52
            | ExchangePhase::X80
            | ExchangePhase::X81
            | ExchangePhase::X82
            | ExchangePhase::X83
            | ExchangePhase::X86
            | ExchangePhase::X87
    )
}

fn supports_predicted_quantity(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X52
            | ExchangePhase::X80
            | ExchangePhase::X81
            | ExchangePhase::X82
            | ExchangePhase::X86
            | ExchangePhase::X87
    )
}

fn supports_billed_quantity(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X52 | ExchangePhase::X80 | ExchangePhase::X81 | ExchangePhase::X82
    )
}

fn supports_prices(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X52
            | ExchangePhase::X80
            | ExchangePhase::X81
            | ExchangePhase::X82
            | ExchangePhase::X84
            | ExchangePhase::X85
            | ExchangePhase::X86
            | ExchangePhase::X87
    )
}

fn requires_outline(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
    )
}

fn requires_unit_price(phase: ExchangePhase) -> bool {
    matches!(phase, ExchangePhase::X83Z | ExchangePhase::X86ZR)
}

fn supports_references(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X52
            | ExchangePhase::X80
            | ExchangePhase::X81
            | ExchangePhase::X82
            | ExchangePhase::X83
            | ExchangePhase::X85
            | ExchangePhase::X86
            | ExchangePhase::X87
    )
}

fn supports_sub_descriptions(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X52
            | ExchangePhase::X80
            | ExchangePhase::X81
            | ExchangePhase::X82
            | ExchangePhase::X83
            | ExchangePhase::X84
            | ExchangePhase::X85
            | ExchangePhase::X86
            | ExchangePhase::X87
    )
}

fn supports_sum_description(phase: ExchangePhase) -> bool {
    matches!(
        phase,
        ExchangePhase::X52
            | ExchangePhase::X80
            | ExchangePhase::X81
            | ExchangePhase::X82
            | ExchangePhase::X83
            | ExchangePhase::X85
            | ExchangePhase::X86
            | ExchangePhase::X87
    )
}

fn write_item(item: &Item, phase: ExchangePhase) -> Node {
    if matches!(phase, ExchangePhase::X89 | ExchangePhase::X89B) {
        return write_invoice_item(item);
    }
    if matches!(
        item.source_item_kind.as_deref(),
        Some("HourItem" | "MaterialItem" | "MarkupItemOWN" | "MarkupItemCTR")
    ) {
        if matches!(
            phase,
            ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
        ) {
            return write_time_item(item, phase);
        }
        let mut converted = item.clone();
        let source_kind = converted.source_item_kind.take();
        converted.item_type = if matches!(
            source_kind.as_deref(),
            Some("MarkupItemOWN" | "MarkupItemCTR")
        ) {
            ItemType::Markup
        } else {
            ItemType::Normal
        };
        return write_item(&converted, phase);
    }
    if item.item_type == ItemType::Markup {
        if matches!(
            phase,
            ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
        ) {
            let mut converted = item.clone();
            converted.item_type = ItemType::Normal;
            return write_item(&converted, phase);
        }
        return write_markup_item(item, phase);
    }
    let mut node = element("Item");
    node.attributes.insert(
        "ID".to_owned(),
        item.id
            .clone()
            .unwrap_or_else(|| format!("I{}", item.oz.replace('.', "_"))),
    );
    node.attributes
        .insert("RNoPart".to_owned(), item.oz.clone());
    attribute_opt(&mut node, "RNoIndex", item.rno_index.as_deref());
    if phase == ExchangePhase::X84Z {
        text_child(
            &mut node,
            "BidUpDownPct",
            &item.bid_up_down_pct.unwrap_or(Decimal::ZERO).to_string(),
        );
        return node;
    }
    if phase == ExchangePhase::X86ZE {
        text_child(
            &mut node,
            "Qty",
            &item.quantity.unwrap_or(Decimal::ZERO).to_string(),
        );
        node.children.push(write_time_description(item, phase));
        return node;
    }
    if supports_item_flags(phase) {
        match item.item_type {
            ItemType::LumpSum => text_child(&mut node, "LumpSumItem", "Yes"),
            ItemType::Alternative => {
                text_child(
                    &mut node,
                    "ALNGroupNo",
                    item.alternative_group.as_deref().unwrap_or("1"),
                );
                text_child(
                    &mut node,
                    "ALNSerNo",
                    item.alternative_serial.as_deref().unwrap_or("1"),
                );
            }
            ItemType::Eventual => text_child(&mut node, "Provis", "WithoutTotal"),
            ItemType::TextOnly | ItemType::Supplement => {}
            ItemType::Normal => {}
            ItemType::Markup => unreachable!("markup items are handled before this match"),
        }
    }
    if item.not_applicable && supports_not_applicable(phase) {
        text_child(
            &mut node,
            if phase == ExchangePhase::X84 {
                "NotOffered"
            } else {
                "NotAppl"
            },
            "Yes",
        );
    }
    if supports_item_flags(phase) {
        if item.hour_item {
            text_child(&mut node, "HourIt", "Yes");
        }
        if item.markup_item {
            text_child(&mut node, "MarkupIt", "Yes");
        }
        if item.up_breakdown {
            text_child(&mut node, "UPBkdn", "Yes");
        }
    }
    if supports_change_order_fields(phase) {
        text_child_opt(&mut node, "CONo", item.change_order_number.as_deref());
        text_child_opt(&mut node, "COStatus", item.co_status.as_deref());
    }
    if supports_references(phase) {
        if let Some(reference) = &item.reference_description {
            node.children.push(write_ref_item("RefDescr", reference));
        }
        if let Some(reference) = &item.reference_rno {
            node.children.push(write_ref_item("RefRNo", reference));
        }
        if let Some(reference) = &item.reference_performance {
            node.children.push(write_ref_item("RefPerfNo", reference));
        }
    }
    if item.quantity_tbd
        && matches!(
            phase,
            ExchangePhase::X80
                | ExchangePhase::X81
                | ExchangePhase::X82
                | ExchangePhase::X83
                | ExchangePhase::X86
                | ExchangePhase::X87
        )
    {
        text_child(&mut node, "QtyTBD", "Yes");
    }
    if !(matches!(phase, ExchangePhase::X83Z | ExchangePhase::X86ZR)
        || (phase == ExchangePhase::X83 && item.quantity_tbd))
    {
        text_decimal(&mut node, "Qty", item.quantity);
    }
    for split in item.qty_splits.iter().filter(|_| {
        !matches!(
            phase,
            ExchangePhase::X84 | ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR
        )
    }) {
        let mut value = element("QtySplit");
        text_child_opt(&mut value, "Label", Some(&split.label));
        text_child(&mut value, "Qty", &split.quantity.to_string());
        text_child_opt(&mut value, "QU", split.unit.as_deref());
        node.children.push(value);
    }
    if supports_predicted_quantity(phase) {
        text_decimal(&mut node, "PredQty", item.predicted_quantity);
    }
    if supports_billed_quantity(phase) {
        text_decimal(&mut node, "BillQty", item.bill_quantity);
    }
    if matches!(
        phase,
        ExchangePhase::X83 | ExchangePhase::X83Z | ExchangePhase::X85 | ExchangePhase::X86ZR
    ) && item.unit.is_none()
    {
        text_child(
            &mut node,
            "QU",
            if matches!(phase, ExchangePhase::X83Z | ExchangePhase::X86ZR) {
                "h"
            } else {
                "N/A"
            },
        );
    } else if phase != ExchangePhase::X84 {
        text_child_opt(&mut node, "QU", item.unit.as_deref());
    }
    if !matches!(
        phase,
        ExchangePhase::X84 | ExchangePhase::X84Z | ExchangePhase::X86ZE
    ) {
        for assign in &item.ctlg_assigns {
            node.children.push(write_ctlg_assign(assign));
        }
    }
    if supports_prices(phase) || requires_unit_price(phase) {
        if requires_unit_price(phase) {
            text_child(
                &mut node,
                "UP",
                &item.unit_price.unwrap_or(Decimal::ZERO).to_string(),
            );
        } else {
            text_decimal(&mut node, "UP", item.unit_price);
        }
    }
    if supports_prices(phase) {
        for (index, component) in item.up_components.iter().enumerate().take(6) {
            text_child(
                &mut node,
                &format!("UPComp{}", index + 1),
                &component.to_string(),
            );
        }
    }
    if supports_prices(phase) {
        text_decimal(&mut node, "DiscountPcnt", item.discount_pct);
        if matches!(
            phase,
            ExchangePhase::X52 | ExchangePhase::X80 | ExchangePhase::X81 | ExchangePhase::X82
        ) {
            text_decimal(&mut node, "UPFrom", item.up_from);
            text_decimal(&mut node, "UPAvg", item.up_avg);
            text_decimal(&mut node, "UPTo", item.up_to);
        }
        text_decimal(&mut node, "IT", item.total_price);
    }
    text_decimal(&mut node, "VAT", item.vat);
    for approach in &item.cost_approaches {
        if phase != ExchangePhase::X52 {
            continue;
        }
        let mut value = element("CostApproach");
        text_child_opt(&mut value, "CostType", Some(&approach.cost_type));
        text_decimal(&mut value, "Amount", approach.amount);
        text_child_opt(&mut value, "Remark", Some(&approach.remark));
        node.children.push(value);
    }
    let has_rich_detail = item
        .rich_text
        .as_ref()
        .is_some_and(|text| !text.paragraphs.is_empty() || !text.tables.is_empty());
    let has_detail = !item.long_text.is_empty() || has_rich_detail;
    if phase == ExchangePhase::X84
        && (!item.short_text.is_empty() || has_detail || !item.text_complements.is_empty())
    {
        let mut description = element("Description");
        let mut complete = element("CompleteText");
        let mut detail = element("DetailTxt");
        for complement in &item.text_complements {
            push_text_complement(&mut detail, complement, phase);
        }
        complete.children.push(detail);
        description.children.push(complete);
        node.children.push(description);
    } else if !item.short_text.is_empty()
        || has_detail
        || !item.text_complements.is_empty()
        || item.standard_service.is_some()
        || requires_outline(phase)
    {
        let mut description = element("Description");
        if let Some(service) = &item.standard_service {
            if let Some(number) = &service.number {
                text_child(&mut description, "StLNo", number);
            } else {
                description.children.push(write_stlb(service));
            }
        }
        if has_detail {
            let mut complete = element("CompleteText");
            let mut detail = element("DetailTxt");
            write_item_detail(&mut detail, item);
            for complement in &item.text_complements {
                push_text_complement(&mut detail, complement, phase);
            }
            complete.children.push(detail);
            if !item.short_text.is_empty() || requires_outline(phase) {
                complete.children.push(outline_text(&item.short_text));
            }
            description.children.push(complete);
        } else if !item.short_text.is_empty()
            && !matches!(
                phase,
                ExchangePhase::X81
                    | ExchangePhase::X82
                    | ExchangePhase::X83
                    | ExchangePhase::X86
                    | ExchangePhase::X87
                    | ExchangePhase::X83Z
                    | ExchangePhase::X86ZR
            )
        {
            description.children.push(outline_text(&item.short_text));
        } else if !item.short_text.is_empty() {
            let mut complete = element("CompleteText");
            let mut detail = element("DetailTxt");
            if !item.text_complements.is_empty() {
                for complement in &item.text_complements {
                    push_text_complement(&mut detail, complement, phase);
                }
            }
            complete.children.push(detail);
            complete.children.push(outline_text(&item.short_text));
            description.children.push(complete);
        } else if !item.text_complements.is_empty() || requires_outline(phase) {
            let mut complete = element("CompleteText");
            let mut detail = element("DetailTxt");
            for complement in &item.text_complements {
                push_text_complement(&mut detail, complement, phase);
            }
            complete.children.push(detail);
            if requires_outline(phase) {
                complete.children.push(outline_text(&item.short_text));
            }
            description.children.push(complete);
        }
        node.children.push(description);
    }
    write_item_attachments(&mut node, item);
    if phase == ExchangePhase::X86ZE && item.short_text.is_empty() && !has_detail {
        let mut description = element("Description");
        text_child(
            &mut description,
            "WICNo",
            item.catalog_number.as_deref().unwrap_or("0"),
        );
        node.children.push(description);
    }
    if supports_sum_description(phase)
        && (item.sum_description
            || (!item.sub_descriptions.is_empty() && phase != ExchangePhase::X52))
    {
        text_child(&mut node, "SumDescr", "Yes");
    }
    for sub in item
        .sub_descriptions
        .iter()
        .filter(|_| supports_sub_descriptions(phase))
    {
        let mut value = element("SubDescr");
        text_child(&mut value, "SubDNo", &sub.number);
        if supports_sub_descriptions(phase) && phase != ExchangePhase::X84 {
            let mut description = element("Description");
            let mut complete = element("CompleteText");
            let mut detail = element("DetailTxt");
            if !sub.long_text.is_empty() {
                detail.children.push(paragraph_text("Text", &sub.long_text));
            }
            for complement in &sub.text_complements {
                push_text_complement(&mut detail, complement, phase);
            }
            complete.children.push(detail);
            if !sub.short_text.is_empty() {
                complete.children.push(outline_text(&sub.short_text));
            }
            description.children.push(complete);
            value.children.push(description);
        }
        node.children.push(value);
    }
    if phase == ExchangePhase::X86ZR {
        text_decimal(&mut node, "BidUpDownPct", item.bid_up_down_pct);
    }
    if phase == ExchangePhase::X83Z {
        text_child(
            &mut node,
            "BidUpDownReq",
            item.bid_up_down_request.as_deref().unwrap_or("No"),
        );
    }
    if phase == ExchangePhase::X84Z {
        text_decimal(&mut node, "BidUpDownPct", item.bid_up_down_pct);
    }
    node
}

fn write_invoice_item(item: &Item) -> Node {
    let mut node = write_item(item, ExchangePhase::X86);
    node.children
        .retain(|child| !matches!(child.name.as_str(), "Qty" | "PredQty"));
    if !node.children.iter().any(|child| child.name == "BillQty") {
        let insert_at = node
            .children
            .iter()
            .position(|child| child.name == "QU")
            .unwrap_or(node.children.len());
        let mut bill_qty = element("BillQty");
        bill_qty.text = item
            .bill_quantity
            .or(item.quantity)
            .unwrap_or_default()
            .to_string();
        node.children.insert(insert_at, bill_qty);
    }
    if !node.children.iter().any(|child| child.name == "QU") {
        let mut unit = element("QU");
        unit.text = item.unit.as_deref().unwrap_or("N/A").to_owned();
        let position = node
            .children
            .iter()
            .position(|child| child.name == "UP")
            .unwrap_or(node.children.len());
        node.children.insert(position, unit);
    }
    if !node.children.iter().any(|child| child.name == "IT") {
        let mut total = element("IT");
        total.text = item.total_price.unwrap_or_default().to_string();
        let position = node
            .children
            .iter()
            .position(|child| child.name == "VAT")
            .unwrap_or(node.children.len());
        node.children.insert(position, total);
    }
    if !node
        .children
        .iter()
        .any(|child| child.name == "Description")
    {
        let mut description = element("Description");
        let mut complete = element("CompleteText");
        complete.children.push(element("DetailTxt"));
        description.children.push(complete);
        node.children.push(description);
    }
    node
}

fn write_markup_item(item: &Item, phase: ExchangePhase) -> Node {
    let mut node = element("MarkupItem");
    node.attributes.insert(
        "ID".to_owned(),
        item.id
            .clone()
            .unwrap_or_else(|| format!("M{}", item.oz.replace('.', "_"))),
    );
    node.attributes
        .insert("RNoPart".to_owned(), item.oz.clone());
    attribute_opt(&mut node, "RNoIndex", item.rno_index.as_deref());
    if phase != ExchangePhase::X84 {
        text_child(
            &mut node,
            "MarkupType",
            item.markup_type.as_deref().unwrap_or("IdentAsMark"),
        );
    }
    if phase != ExchangePhase::X84 {
        for sub in &item.markup_sub_qtys {
            let mut value = element("MarkupSubQty");
            let mut reference = element("RefItem");
            attribute_opt(&mut reference, "IDRef", sub.ref_id.as_deref());
            value.children.push(reference);
            text_decimal(&mut value, "SubQty", sub.quantity);
            node.children.push(value);
        }
    }
    if phase == ExchangePhase::X84 {
        text_decimal(&mut node, "ITMarkup", item.total_price);
        text_decimal(&mut node, "Markup", item.unit_price);
    } else if matches!(phase, ExchangePhase::X86 | ExchangePhase::X87) {
        text_child(
            &mut node,
            "ITMarkup",
            &item.total_price.unwrap_or(Decimal::ZERO).to_string(),
        );
        text_child(
            &mut node,
            "Markup",
            &item.unit_price.unwrap_or(Decimal::ZERO).to_string(),
        );
    }
    text_decimal(&mut node, "DiscountPcnt", item.discount_pct);
    if phase == ExchangePhase::X84 {
        text_decimal(&mut node, "IT", item.total_price);
    } else if matches!(phase, ExchangePhase::X86 | ExchangePhase::X87) {
        text_child(
            &mut node,
            "IT",
            &item.total_price.unwrap_or(Decimal::ZERO).to_string(),
        );
    }
    let mut description = element("Description");
    let mut complete = element("CompleteText");
    let text = item.short_text.as_str();
    let mut detail = element("DetailTxt");
    if phase != ExchangePhase::X84 && !item.long_text.is_empty() {
        detail
            .children
            .push(paragraph_text("Text", &item.long_text));
    } else if phase != ExchangePhase::X84 && !text.is_empty() {
        detail.children.push(paragraph_text("Text", text));
    }
    for complement in &item.text_complements {
        push_text_complement(&mut detail, complement, phase);
    }
    complete.children.push(detail);
    if phase != ExchangePhase::X84 && (!text.is_empty() || requires_outline(phase)) {
        complete.children.push(outline_text(text));
    }
    description.children.push(complete);
    if phase != ExchangePhase::X84
        || !item.short_text.is_empty()
        || !item.long_text.is_empty()
        || !item.text_complements.is_empty()
    {
        node.children.push(description);
    }
    if phase != ExchangePhase::X84 {
        for assign in &item.ctlg_assigns {
            node.children.push(write_ctlg_assign(assign));
        }
    }
    node
}
fn write_trade(root: &mut Node, document: &TradeDocument, phase: ExchangePhase) {
    if document.project != ProjectInfo::default() {
        let mut project = element("PrjInfo");
        text_child_opt(&mut project, "PrjID", document.project.id.as_deref());
        text_child_opt(&mut project, "NamePrj", document.project.name.as_deref());
        text_child_opt(&mut project, "LblPrj", document.project.label.as_deref());
        text_child_opt(&mut project, "Cur", document.project.currency.as_deref());
        text_child_opt(
            &mut project,
            "CurLbl",
            document.project.currency_label.as_deref(),
        );
        root.children.push(project);
    }
    let mut order = element("Order");
    text_child(&mut order, "DP", phase.code().trim_start_matches('X'));
    let mut order_info = element("OrderInfo");
    text_child_opt(&mut order_info, "OrderNo", document.order_number.as_deref());
    text_child_opt(&mut order_info, "Ref", document.order_reference.as_deref());
    text_child_opt(&mut order_info, "Cur", document.currency.as_deref());
    text_child_opt(&mut order_info, "OrderDate", document.order_date.as_deref());
    text_child_opt(
        &mut order_info,
        "DeliveryDate",
        document.delivery_date.as_deref(),
    );
    if !order_info.children.is_empty() {
        order.children.push(order_info);
    }
    for (name, party) in [
        ("SupplierInfo", document.supplier.as_ref()),
        ("CustomerInfo", document.customer.as_ref()),
        ("DeliveryPlaceInfo", document.delivery_place.as_ref()),
        ("PlannerInfo", document.planner.as_ref()),
        ("InvoiceInfo", document.invoice_recipient.as_ref()),
    ] {
        if let Some(party) = party {
            order.children.push(write_trade_party(name, party));
        }
    }
    for assign in &document.catalog_assigns {
        order.children.push(write_ctlg_assign(assign));
    }
    for item in &document.items {
        let mut node = element("OrderItem");
        node.attributes.insert("ID".to_owned(), item.id.clone());
        text_child_opt(&mut node, "ItemChara", item.item_chara.as_deref());
        text_child_opt(&mut node, "ItemType", item.item_type.as_deref());
        text_child_opt(
            &mut node,
            "Service",
            item.service.as_deref().or(item.is_service.then_some("Yes")),
        );
        text_child_opt(&mut node, "EAN", item.ean.as_deref());
        text_child_opt(&mut node, "ArtNoID", item.article_number_id.as_deref());
        text_child_opt(&mut node, "ArtNo", item.article_number.as_deref());
        text_child_opt(
            &mut node,
            "SupplierArtNoID",
            item.supplier_article_number_id.as_deref(),
        );
        text_child_opt(
            &mut node,
            "SupplierArtNo",
            item.supplier_article_number.as_deref(),
        );
        text_child_opt(
            &mut node,
            "CustomerArtNo",
            item.customer_article_number.as_deref(),
        );
        text_child_opt(
            &mut node,
            "CatalogArtNo",
            item.catalog_article_number.as_deref(),
        );
        text_child_opt(&mut node, "CatalogNo", item.catalog_number.as_deref());
        text_decimal(&mut node, "Qty", item.quantity);
        text_child_opt(&mut node, "QU", item.unit.as_deref());
        if !item.short_text.is_empty() || !item.long_text.is_empty() {
            node.children
                .push(trade_description(&item.short_text, &item.long_text));
        }
        text_decimal(&mut node, "OfferPrice", item.offer_price);
        text_decimal(&mut node, "NetPrice", item.net_price);
        text_decimal(&mut node, "PriceBasis", item.price_basis);
        text_child_opt(&mut node, "AQU", item.alternate_unit.as_deref());
        text_child_opt(
            &mut node,
            "ModeOfShipment",
            item.mode_of_shipment.as_deref(),
        );
        text_child_opt(&mut node, "DeliveryDate", item.delivery_date.as_deref());
        text_child_opt(&mut node, "DeliveryTime", item.delivery_time.as_deref());
        text_child_opt(&mut node, "DeliveryWeek", item.delivery_week.as_deref());
        text_child_opt(&mut node, "DeliveryChara", item.delivery_chara.as_deref());
        for assign in &item.ctlg_assigns {
            node.children.push(write_ctlg_assign(assign));
        }
        order.children.push(node);
    }
    root.children.push(order);
}

fn write_trade_address_container(name: &str, address: &crate::TradeAddress) -> Node {
    let mut node = element(name);
    let mut address_node = element("Address");
    text_child(
        &mut address_node,
        "Name1",
        address.name.as_deref().unwrap_or("UNSPECIFIED"),
    );
    text_child_opt(&mut address_node, "Name2", address.name2.as_deref());
    text_child_opt(&mut address_node, "Name3", address.name3.as_deref());
    text_child_opt(&mut address_node, "Name4", address.name4.as_deref());
    text_child(
        &mut address_node,
        "Street",
        address.street.as_deref().unwrap_or("UNSPECIFIED"),
    );
    text_child(
        &mut address_node,
        "PCode",
        address.postal_code.as_deref().unwrap_or("UNSPECIFIED"),
    );
    text_child(
        &mut address_node,
        "City",
        address.city.as_deref().unwrap_or("UNSPECIFIED"),
    );
    text_child_opt(&mut address_node, "Country", address.country.as_deref());
    text_child_opt(&mut address_node, "Contact", address.contact.as_deref());
    text_child_opt(&mut address_node, "Phone", address.phone.as_deref());
    text_child_opt(&mut address_node, "Fax", address.fax.as_deref());
    text_child_opt(&mut address_node, "Email", address.email.as_deref());
    text_child_opt(&mut address_node, "ILN", address.iln.as_deref());
    text_child_opt(&mut address_node, "VATID", address.vat_id.as_deref());
    node.children.push(address_node);
    node
}

fn write_trade_party(name: &str, party: &crate::TradeParty) -> Node {
    let mut node = element(name);
    if let Some(address) = &party.address {
        let mut address_node = element("Address");
        text_child_opt(&mut address_node, "Name1", address.name.as_deref());
        text_child_opt(&mut address_node, "Name2", address.name2.as_deref());
        text_child_opt(&mut address_node, "Name3", address.name3.as_deref());
        text_child_opt(&mut address_node, "Name4", address.name4.as_deref());
        text_child_opt(&mut address_node, "Street", address.street.as_deref());
        text_child_opt(&mut address_node, "PCode", address.postal_code.as_deref());
        text_child_opt(&mut address_node, "City", address.city.as_deref());
        text_child_opt(&mut address_node, "Country", address.country.as_deref());
        text_child_opt(&mut address_node, "Contact", address.contact.as_deref());
        text_child_opt(&mut address_node, "Phone", address.phone.as_deref());
        text_child_opt(&mut address_node, "Fax", address.fax.as_deref());
        text_child_opt(&mut address_node, "Email", address.email.as_deref());
        text_child_opt(&mut address_node, "ILN", address.iln.as_deref());
        text_child_opt(&mut address_node, "VATID", address.vat_id.as_deref());
        node.children.push(address_node);
    }
    node
}

fn trade_description(short_text: &str, long_text: &str) -> Node {
    let mut description = element("Description");
    let mut complete = element("CompleteText");
    if !long_text.is_empty() {
        complete.children.push(paragraph_text("Text", long_text));
    }
    if !short_text.is_empty() {
        complete.children.push(outline_text(short_text));
    }
    description.children.push(complete);
    description
}
fn write_cost(root: &mut Node, document: &CostDocument, phase: ExchangePhase) {
    let mut costing = element("ElementalCosting");
    text_child(
        &mut costing,
        "DP",
        document
            .dp
            .as_deref()
            .filter(|dp| cost_dp_matches_phase(dp, phase))
            .unwrap_or_else(|| default_cost_dp(phase)),
    );
    let mut info = element("ECInfo");
    text_child(
        &mut info,
        "Name",
        document
            .name
            .as_deref()
            .or(document.info.label.as_deref())
            .unwrap_or("UNSPECIFIED"),
    );
    text_child_opt(&mut info, "LblEC", document.info.label.as_deref());
    text_child_opt(&mut info, "ECType", document.info.cost_type.as_deref());
    text_child_opt(&mut info, "ECMethod", document.info.cost_method.as_deref());
    text_child_opt(&mut info, "Date", document.info.date.as_deref());
    text_child_opt(
        &mut info,
        "Cur",
        document.info.currency.as_deref().or(Some("EUR")),
    );
    text_child_opt(
        &mut info,
        "CurLbl",
        document.info.currency_label.as_deref().or(Some("Euro")),
    );
    text_child_opt(
        &mut info,
        "DateOfPrice",
        document.info.date_of_price.as_deref(),
    );
    for breakdown in &document.info.breakdowns {
        let mut value = element("ECBkdn");
        text_child(&mut value, "Type", &breakdown.bkdn_type);
        text_child(&mut value, "LblOutline", &breakdown.label);
        if let Some(length) = breakdown.length {
            text_child(&mut value, "Length", &length.to_string());
        }
        text_child(
            &mut value,
            "Num",
            if breakdown.is_numeric { "Yes" } else { "No" },
        );
        info.children.push(value);
    }
    for member in &document.info.consortium_members {
        let mut value = element("ConsortiumMember");
        text_child(&mut value, "Description", &member.description);
        if let Some(address) = &member.address {
            let mut address_node = element("Address");
            text_child_opt(&mut address_node, "Name", address.name.as_deref());
            text_child_opt(&mut address_node, "Street", address.street.as_deref());
            text_child_opt(&mut address_node, "PCode", address.postal_code.as_deref());
            text_child_opt(&mut address_node, "City", address.city.as_deref());
            text_child_opt(&mut address_node, "Country", address.country.as_deref());
            value.children.push(address_node);
        }
        info.children.push(value);
    }
    if let Some(date) = &document.info.date_of_information {
        let mut date_info = element("DateOfInformation");
        text_child(&mut date_info, "Date", date);
        info.children.push(date_info);
    }
    if document.info.total_net.is_some() || document.info.total_gross.is_some() {
        let mut totals = element("Totals");
        text_decimal(&mut totals, "TotalNet", document.info.total_net);
        text_decimal(&mut totals, "TotalGross", document.info.total_gross);
        info.children.push(totals);
    }
    costing.children.push(info);
    let mut body = element("ECBody");
    for category in &document.categories {
        body.children.push(write_cost_category(category));
    }
    for element in &document.dimension_elements {
        body.children.push(write_cost_dimension_element(element));
    }
    for element in &document.category_elements {
        body.children.push(write_cost_category_element(element));
    }
    if document.categories.is_empty() {
        for item in &document.items {
            body.children.push(write_cost_item(item));
        }
    }
    costing.children.push(body);
    root.children.push(costing);
}
fn write_cost_property(property: &crate::CostProperty) -> Node {
    let mut node = element("Property");
    text_child(&mut node, "Name", &property.name);
    text_child(&mut node, "LblProp", &property.label);
    text_child_opt(
        &mut node,
        "ArithmeticQuantityApproach",
        property.arithmetic_quantity_approach.as_deref(),
    );
    text_decimal(
        &mut node,
        "ValueQuantityApproach",
        property.value_quantity_approach,
    );
    text_child(&mut node, "QU", &property.unit);
    text_child_opt(&mut node, "Type", property.property_type.as_deref());
    text_child_opt(&mut node, "CAD_ID", property.cad_id.as_deref());
    node
}

fn write_cost_reference_group(group: &crate::CostReferenceGroup) -> Node {
    let mut node = element("RefGroup");
    text_child(&mut node, "Title", &group.title);
    for reference in &group.references {
        let mut child = element(if reference.reference_kind.is_empty() {
            "CostElementRef"
        } else {
            &reference.reference_kind
        });
        attribute_opt(&mut child, "IDRef", Some(&reference.id_ref));
        attribute_opt(&mut child, "Type", reference.reference_type.as_deref());
        text_decimal(&mut child, "Portion", reference.portion);
        node.children.push(child);
    }
    node
}

fn write_cost_dimension_element(element_value: &crate::CostDimensionElement) -> Node {
    let mut node = element("DimensionElement");
    text_child(&mut node, "EleNo", &element_value.id);
    text_child(&mut node, "Descr", &element_value.description);
    text_child_opt(&mut node, "CatID", element_value.category_id.as_deref());
    text_child(&mut node, "Remark", &element_value.remark);
    for property in &element_value.properties {
        node.children.push(write_cost_property(property));
    }
    text_decimal(&mut node, "Qty", element_value.quantity);
    text_child(&mut node, "QU", &element_value.unit);
    text_decimal(&mut node, "Markup", element_value.markup);
    node
}

fn write_cost_category_element(element_value: &crate::CostCategoryElement) -> Node {
    let mut node = element("CategoryElement");
    text_child(&mut node, "EleNo", &element_value.id);
    text_child(&mut node, "Descr", &element_value.description);
    text_child_opt(&mut node, "CatID", element_value.category_id.as_deref());
    text_child(&mut node, "Remark", &element_value.remark);
    for property in &element_value.properties {
        node.children.push(write_cost_property(property));
    }
    text_decimal(&mut node, "Markup", element_value.markup);
    node
}

fn write_cost_category(category: &crate::CostCategory) -> Node {
    let mut node = element("ECCtgy");
    text_child(&mut node, "EleNo", &category.id);
    text_child(&mut node, "Descr", &category.description);
    text_decimal(&mut node, "Portion", category.portion);
    for property in &category.properties {
        node.children.push(write_cost_property(property));
    }
    let mut body = element("ECBody");
    for item in &category.items {
        body.children.push(write_cost_item(item));
    }
    for child in &category.categories {
        body.children.push(write_cost_category(child));
    }
    if !body.children.is_empty() {
        node.children.push(body);
    }
    if category.totals_net.is_some() || category.totals_gross.is_some() {
        let mut totals = element("Totals");
        text_decimal(&mut totals, "TotalNet", category.totals_net);
        text_decimal(&mut totals, "TotalGross", category.totals_gross);
        node.children.push(totals);
    }
    node
}

fn cost_dp_matches_phase(dp: &str, phase: ExchangePhase) -> bool {
    dp.trim().starts_with(phase.code().trim_start_matches('X'))
}
fn default_cost_dp(phase: ExchangePhase) -> &'static str {
    match phase {
        ExchangePhase::X50 => "50.2",
        ExchangePhase::X51 => "51.2",
        _ => phase.code().trim_start_matches('X'),
    }
}
fn write_cost_item(item: &CostItem) -> Node {
    let mut node = element("CostElement");
    node.attributes.insert("ID".to_owned(), item.id.clone());
    text_child(&mut node, "EleNo", &item.id);
    text_child(&mut node, "Descr", &item.short_text);
    text_child_opt(&mut node, "CatID", item.cat_id.as_deref());
    for assign in &item.ctlg_assigns {
        node.children.push(write_ctlg_assign(assign));
    }
    text_child_opt(&mut node, "Remark", Some(&item.remark));
    for property in &item.properties {
        node.children.push(write_cost_property(property));
    }
    for group in &item.reference_groups {
        node.children.push(write_cost_reference_group(group));
    }
    if item.is_bill_element {
        text_child(&mut node, "BillElement", "Yes");
    }
    text_decimal(&mut node, "Portion", item.portion);
    if item.declared_total.is_some() {
        let mut totals = element("Totals");
        text_decimal(&mut totals, "Total", item.declared_total);
        node.children.push(totals);
    }
    text_decimal(&mut node, "Qty", item.quantity);
    text_child_opt(&mut node, "QU", item.unit.as_deref());
    text_decimal(&mut node, "UP", item.unit_price);
    text_decimal(&mut node, "Markup", item.markup);
    text_decimal(&mut node, "IT", item.total_price);
    text_decimal(&mut node, "UPFrom", item.up_from);
    text_decimal(&mut node, "UPAvg", item.up_avg);
    text_decimal(&mut node, "UPTo", item.up_to);
    for child in &item.children {
        node.children.push(write_cost_item(child));
    }
    node
}
fn write_quantity(root: &mut Node, document: &QuantityDocument, phase: ExchangePhase) {
    let mut quantity = element("QtyDeterm");
    let mut info = element("QtyDetermInfo");
    text_child_opt(
        &mut info,
        "MethodDescription",
        document.method.as_deref().or(Some("REB23003-2009")),
    );
    text_child_opt(&mut info, "OrdDescr", document.order_description.as_deref());
    text_child_opt(
        &mut info,
        "ProjDescr",
        document.project_description.as_deref(),
    );
    text_child_opt(
        &mut info,
        "ServiceProvisionStartDate",
        document.service_start.as_deref(),
    );
    text_child_opt(
        &mut info,
        "ServiceProvisionEndDate",
        document.service_end.as_deref(),
    );
    quantity.children.push(info);
    text_child(&mut quantity, "DP", phase.code().trim_start_matches('X'));
    if document.reference_project_name.is_some() || document.reference_project_id.is_some() {
        let mut project = element("PrjInfo");
        text_child_opt(
            &mut project,
            "RefPrjName",
            document.reference_project_name.as_deref(),
        );
        text_child_opt(
            &mut project,
            "RefPrjID",
            document.reference_project_id.as_deref(),
        );
        quantity.children.push(project);
    }
    if let Some(address) = &document.owner {
        quantity
            .children
            .push(write_trade_address_container("OWN", address));
    }
    if let Some(address) = &document.contractor {
        quantity
            .children
            .push(write_trade_address_container("CTR", address));
    }

    let mut boq = element("BoQ");
    boq.attributes.insert("ID".to_owned(), "QB1".to_owned());
    text_child_opt(
        &mut boq,
        "RefBoQName",
        document.reference_boq_name.as_deref(),
    );
    text_child_opt(&mut boq, "RefBoQID", document.reference_boq_id.as_deref());
    for catalog in &document.catalogs {
        let mut node = element("Ctlg");
        text_child(&mut node, "CtlgID", &catalog.id);
        text_child(&mut node, "CtlgType", &catalog.kind);
        text_child(&mut node, "CtlgName", &catalog.name);
        text_child(&mut node, "CtlgAssignType", &catalog.assignment_type);
        boq.children.push(node);
    }
    for attachment in &document.attachments {
        let mut container = element("CtlgAttachment");
        let mut node = element("Attachment");
        text_child(&mut node, "Name", &attachment.name);
        text_child(&mut node, "Text", &attachment.text);
        text_child(&mut node, "Descrip", &attachment.description);
        text_child(&mut node, "Type", &attachment.file_type);
        let encoded =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &attachment.data);
        text_child(&mut node, "Data", &encoded);
        container.children.push(node);
        boq.children.push(container);
    }
    let mut breakdown = element("BoQBkdn");
    text_child(&mut breakdown, "Type", "Item");
    text_child(&mut breakdown, "Length", "3");
    text_child(&mut breakdown, "Num", "Yes");
    boq.children.push(breakdown);
    let mut body = element("BoQBody");
    if document.categories.is_empty() {
        let mut list = element("Itemlist");
        let mut id_counter = 0;
        for item in &document.items {
            list.children
                .push(write_quantity_item(item, &mut id_counter));
        }
        body.children.push(list);
    } else {
        let mut id_counter = 0;
        for category in &document.categories {
            body.children
                .push(write_quantity_category(category, &mut id_counter));
        }
    }
    boq.children.push(body);
    quantity.children.push(boq);
    root.children.push(quantity);
}

fn write_quantity_category(category: &crate::QuantityCategory, id_counter: &mut usize) -> Node {
    if category.rno.is_empty() && category.categories.is_empty() {
        let mut list = element("Itemlist");
        for item in &category.items {
            list.children.push(write_quantity_item(item, id_counter));
        }
        return list;
    }
    *id_counter += 1;
    let mut node = element("BoQCtgy");
    node.attributes
        .insert("ID".to_owned(), format!("QC{}", id_counter));
    node.attributes.insert(
        "RNoPart".to_owned(),
        if category.rno.is_empty() {
            "1".to_owned()
        } else {
            category.rno.clone()
        },
    );
    let mut body = element("BoQBody");
    if !category.items.is_empty() {
        let mut list = element("Itemlist");
        for item in &category.items {
            list.children.push(write_quantity_item(item, id_counter));
        }
        body.children.push(list);
    }
    for child in &category.categories {
        body.children
            .push(write_quantity_category(child, id_counter));
    }
    node.children.push(body);
    node
}

fn write_quantity_item(item: &crate::QuantityItem, id_counter: &mut usize) -> Node {
    *id_counter += 1;
    let mut node = element("Item");
    node.attributes
        .insert("ID".to_owned(), format!("QI{}", id_counter));
    node.attributes
        .insert("RNoPart".to_owned(), item.rno_part.clone());
    if !item.rno_index.is_empty() {
        node.attributes
            .insert("RNoIndex".to_owned(), item.rno_index.clone());
    }
    if item.quantity.is_some()
        || !item.takeoff_rows.is_empty()
        || !item.determination_items.is_empty()
    {
        let mut determination = element("QtyDeterm");
        text_decimal(&mut determination, "Qty", item.quantity);
        if item.determination_items.is_empty() {
            for row in &item.takeoff_rows {
                let mut determination_item = element("QDetermItem");
                let mut takeoff = element("QTakeoff");
                takeoff.attributes.insert("Row".to_owned(), row.clone());
                determination_item.children.push(takeoff);
                determination.children.push(determination_item);
            }
        } else {
            for determination_item in &item.determination_items {
                let mut value = element("QDetermItem");
                let mut takeoff = element("QTakeoff");
                takeoff
                    .attributes
                    .insert("Row".to_owned(), determination_item.raw.clone());
                value.children.push(takeoff);
                determination.children.push(value);
            }
        }
        node.children.push(determination);
    }
    for assign in &item.ctlg_assigns {
        node.children.push(write_ctlg_assign(assign));
    }
    node
}
fn element(name: &str) -> Node {
    Node {
        name: name.to_owned(),
        ..Node::default()
    }
}

fn span_text(name: &str, value: &str) -> Node {
    let mut text = element(name);
    text.children.push(Node {
        name: "span".to_owned(),
        text: value.to_owned(),
        ..Node::default()
    });
    text
}
fn write_ref_item(name: &str, id: &str) -> Node {
    let mut reference = element(name);
    reference
        .attributes
        .insert("IDRef".to_owned(), id.to_owned());
    reference
}

fn write_stlb(service: &StandardService) -> Node {
    let mut stlb = element("STLBBau");
    let mut catalog = element("STLBBauCtlg");
    text_child(
        &mut catalog,
        "CtlgName",
        service.catalog.as_deref().unwrap_or("UNSPECIFIED"),
    );
    text_child(
        &mut catalog,
        "VersDate",
        service.catalog_version.as_deref().unwrap_or("2001-10"),
    );
    text_child(
        &mut catalog,
        "WCtg",
        service.work_category.as_deref().unwrap_or("000"),
    );
    text_child(
        &mut catalog,
        "Group",
        service.group.as_deref().unwrap_or("0"),
    );
    stlb.children.push(catalog);
    for (index, identifier) in service.identifiers.iter().enumerate() {
        let mut value = element("STLBBauID");
        text_child(&mut value, "ArtChrIdent", identifier);
        if let Some(index_value) = service.identifier_indices.get(index) {
            text_child(&mut value, "ArtChIdx", index_value);
        }
        if let Some(change_value) = service.change_identifiers.get(index) {
            text_child(&mut value, "ChVIdent", change_value);
        }
        stlb.children.push(value);
    }
    stlb
}

fn write_ctlg_assign(assign: &crate::CtlgAssign) -> Node {
    let mut value = element("CtlgAssign");
    text_child(&mut value, "CtlgID", &assign.ctlg_id);
    text_child(&mut value, "CtlgCode", &assign.ctlg_code);
    text_decimal(&mut value, "Quantity", assign.quantity);
    value
}
fn text_complement(value: &str) -> Node {
    write_text_complement(&TextComplement {
        kind: "Bidder".to_owned(),
        mark_label: Some("0".to_owned()),
        empty: false,
        caption: None,
        text: value.to_owned(),
        tail: None,
    })
}
fn push_text_complement(detail: &mut Node, value: &TextComplement, phase: ExchangePhase) {
    if matches!(
        phase,
        ExchangePhase::X83Z | ExchangePhase::X84Z | ExchangePhase::X86ZR | ExchangePhase::X86ZE
    ) {
        detail.children.push(paragraph_text("Text", &value.text));
        return;
    }
    detail
        .children
        .push(write_text_complement_for_phase(value, phase));
}

fn write_text_complement_for_phase(value: &TextComplement, phase: ExchangePhase) -> Node {
    let mut complement = write_text_complement(value);
    if phase == ExchangePhase::X84 {
        complement
            .children
            .retain(|child| child.name == "ComplBody");
    }
    complement
}

fn write_text_complement(value: &TextComplement) -> Node {
    let mut complement = element("TextComplement");
    complement.attributes.insert(
        "Kind".to_owned(),
        if value.kind.is_empty() {
            "Bidder"
        } else {
            &value.kind
        }
        .to_owned(),
    );
    complement.attributes.insert(
        "MarkLbl".to_owned(),
        value.mark_label.as_deref().unwrap_or("0").to_owned(),
    );
    if value.empty {
        complement
            .attributes
            .insert("Empty".to_owned(), "Yes".to_owned());
    }
    if let Some(caption) = &value.caption {
        let mut caption_node = element("ComplCaption");
        caption_node.children.push(Node {
            name: "span".to_owned(),
            text: caption.clone(),
            ..Node::default()
        });
        complement.children.push(caption_node);
    }
    let mut body = element("ComplBody");
    body.children.push(Node {
        name: "span".to_owned(),
        text: value.text.to_owned(),
        ..Node::default()
    });
    complement.children.push(body);
    if let Some(tail) = &value.tail {
        let mut tail_node = element("ComplTail");
        tail_node.children.push(Node {
            name: "span".to_owned(),
            text: tail.clone(),
            ..Node::default()
        });
        complement.children.push(tail_node);
    }
    complement
}
fn outline_text(value: &str) -> Node {
    let mut outline = element("OutlineText");
    let mut outline_text = element("OutlTxt");
    outline_text.children.push(span_text("TextOutlTxt", value));
    outline.children.push(outline_text);
    outline
}
fn paragraph_text(name: &str, value: &str) -> Node {
    let mut text = element(name);
    let mut paragraph = element("p");
    paragraph.children.push(Node {
        name: "span".to_owned(),
        text: value.to_owned(),
        ..Node::default()
    });
    text.children.push(paragraph);
    text
}

fn write_item_detail(detail: &mut Node, item: &Item) {
    if let Some(rich_text) = &item.rich_text {
        let rich_plain = rich_text.paragraphs.join("\n");
        if !rich_text.paragraphs.is_empty() && rich_plain == item.long_text {
            for paragraph in &rich_text.paragraphs {
                detail.children.push(paragraph_text("Text", paragraph));
            }
        }
        // GAEB DA XML permits paragraph content here, but not HTML table
        // elements. Keep table cell text only when no paragraphs exist, so
        // ordinary mixed rich text does not gain duplicate content.
        if rich_text.paragraphs.is_empty() {
            for table in &rich_text.tables {
                for row in table {
                    detail
                        .children
                        .push(paragraph_text("Text", &row.join(" | ")));
                }
            }
        }
        if (!rich_text.paragraphs.is_empty() && rich_plain == item.long_text)
            || (!rich_text.paragraphs.is_empty()
                && item.long_text.is_empty()
                && rich_text.tables.is_empty())
        {
            return;
        }
    }
    if !item.long_text.is_empty() {
        detail
            .children
            .push(paragraph_text("Text", &item.long_text));
    }
}

fn write_item_attachments(node: &mut Node, item: &Item) {
    for attachment in &item.attachments {
        let mut value = element("Attachment");
        text_child(&mut value, "Name", &attachment.filename);
        text_child(&mut value, "MimeType", &attachment.mime_type);
        let encoded =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &attachment.data);
        text_child(&mut value, "Data", &encoded);
        node.children.push(value);
    }
}
fn attribute_opt(node: &mut Node, name: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        node.attributes.insert(name.to_owned(), value.to_owned());
    }
}
fn text_child(node: &mut Node, name: &str, value: &str) {
    if !value.is_empty() {
        node.children.push(Node {
            name: name.to_owned(),
            text: value.to_owned(),
            ..Node::default()
        });
    }
}
fn text_child_opt(node: &mut Node, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        text_child(node, name, value);
    }
}
fn text_decimal(node: &mut Node, name: &str, value: Option<Decimal>) {
    if let Some(value) = value {
        text_child(node, name, &value.to_string());
    }
}
fn emit_node(writer: &mut Writer<Vec<u8>>, node: &Node, german: bool) -> std::io::Result<()> {
    let tag = translated_tag(&node.name, german);
    let mut start = BytesStart::new(tag.as_str());
    for (key, value) in &node.attributes {
        start.push_attribute((translated_tag(key, german).as_str(), value.as_str()));
    }
    if node.children.is_empty() && node.text.is_empty() {
        return writer.write_event(Event::Empty(start));
    }
    writer.write_event(Event::Start(start))?;
    if !node.text.is_empty() {
        writer.write_event(Event::Text(BytesText::new(&node.text)))?;
    }
    for child in &node.children {
        emit_node(writer, child, german)?;
    }
    writer.write_event(Event::End(BytesEnd::new(tag.as_str())))
}
fn translated_tag(tag: &str, german: bool) -> String {
    if !german {
        return tag.to_owned();
    }
    match tag {
        "Award" => "Vergabe",
        "AwardInfo" => "VergabeInfo",
        "BoQ" => "Leistungsverzeichnis",
        "BoQInfo" => "LVInfo",
        "BoQBody" => "LVBereich",
        "BoQCtgy" => "LVGruppe",
        "Itemlist" => "Positionsliste",
        "Item" => "Position",
        "RNoPart" => "OZ",
        "Qty" => "Menge",
        "QU" => "Mengeneinheit",
        "UP" => "Einheitspreis",
        "IT" => "Gesamtbetrag",
        "ShortText" => "Kurztext",
        "LongText" => "Langtext",
        "LblTx" => "Bezeichnung",
        other => other,
    }
    .to_owned()
}
fn namespace(version: SourceVersion, phase: ExchangePhase) -> String {
    match version {
        SourceVersion::DaXml20 => "http://www.gaeb.de/GAEB_DA_XML/200407".to_owned(),
        SourceVersion::DaXml21 => "http://www.gaeb.de/GAEB_DA_XML/200511".to_owned(),
        _ => format!(
            "http://www.gaeb.de/GAEB_DA_XML/DA{}/{}",
            phase.code().trim_start_matches('X'),
            version.number()
        ),
    }
}
fn write_gaeb90(
    document: &Document,
    phase: ExchangePhase,
) -> Result<(Vec<u8>, Vec<String>), GaebError> {
    let Document::Procurement(document) = document else {
        return Err(GaebError::InvalidWriteProfile(
            "GAEB 90 supports procurement documents only".to_owned(),
        ));
    };
    let mut omitted_fields = Vec::new();
    if document.project.label.is_some() {
        omitted_fields.push("Project: label".to_owned());
    }
    if document.project.boq_id.is_some() {
        omitted_fields.push("Project: BoQ ID".to_owned());
    }
    if document.boq.id.is_some() {
        omitted_fields.push("BoQ: ID".to_owned());
    }
    if document.boq.name.is_some() {
        omitted_fields.push("BoQ: name".to_owned());
    }
    if !document.boq.remarks.is_empty() {
        omitted_fields.push("BoQ: remarks".to_owned());
    }
    if !document.boq.performance_descriptions.is_empty() {
        omitted_fields.push("BoQ: performance descriptions".to_owned());
    }
    if document.boq.totals.is_some() {
        omitted_fields.push("BoQ: total".to_owned());
    }

    let phase_code = phase.code().trim_start_matches('X');
    let mut out = String::new();
    out.push_str(&fixed("00", &format!("        {phase_code}L")));
    if let Some(name) = &document.project.name {
        out.push_str(&fixed("02", name));
    }
    if let Some(label) = &document.project.label {
        out.push_str(&fixed("03", label));
    }
    for description in &document.boq.performance_descriptions {
        out.push_str(&fixed("26", description));
    }
    if document.project.currency.is_some() || document.project.currency_label.is_some() {
        out.push_str(&fixed(
            "08",
            &format!(
                "{:<6}{:<10}",
                document.project.currency.as_deref().unwrap_or_default(),
                document
                    .project
                    .currency_label
                    .as_deref()
                    .unwrap_or_default()
            ),
        ));
    }
    for category in document.boq.top_level_categories() {
        write_gaeb90_category(category, &mut out, &mut omitted_fields)?;
    }
    out.push_str(&fixed("99", ""));
    Ok((encode_gaeb90(&out)?, omitted_fields))
}

fn write_gaeb90_category(
    category: &BoqCategory,
    out: &mut String,
    omitted_fields: &mut Vec<String>,
) -> Result<(), GaebError> {
    let rno = if category.rno.is_empty() {
        "01"
    } else {
        &category.rno
    };
    out.push_str(&fixed("11", &format!("{rno:<9}N")));
    if !category.label.is_empty() {
        out.push_str(&fixed("12", &category.label));
    }
    for description in &category.performance_descriptions {
        out.push_str(&fixed("26", description));
    }
    for remark in &category.remarks {
        out.push_str(&fixed("26", remark));
    }
    for item in &category.items {
        gaeb90_item_omissions(item, omitted_fields);
        out.push_str(&fixed("21", &gaeb90_item_record(item)));
        if item.unit_price.is_some() || item.total_price.is_some() {
            out.push_str(&fixed("23", &gaeb90_price_record(item)));
        }
        for text in gaeb90_text_records(&item.short_text) {
            out.push_str(&fixed("25", &text));
        }
        for text in gaeb90_text_records(&item.long_text) {
            out.push_str(&fixed("26", &text));
        }
    }
    for child in &category.categories {
        write_gaeb90_category(child, out, omitted_fields)?;
    }
    out.push_str(&fixed("31", rno));
    if let Some(total) = category.declared_total {
        out.push_str(&fixed(
            "32",
            &format!("{rno:<9}{:<12}", gaeb90_scaled(total, 2, 12)),
        ));
    }
    Ok(())
}

fn gaeb90_item_record(item: &Item) -> String {
    let marker = if item.quantity_tbd {
        "       X "
    } else {
        "         "
    };
    let quantity = item
        .quantity
        .map(|value| gaeb90_scaled(value, 3, 11))
        .unwrap_or_else(|| "           ".to_owned());
    let source_kind = item
        .source_item_kind
        .as_deref()
        .filter(|value| value.chars().count() == 3)
        .unwrap_or("NNN");
    format!(
        "{:<9}{source_kind}{marker}{quantity}{:<10}",
        item.oz,
        item.unit.as_deref().unwrap_or_default()
    )
}

fn gaeb90_price_record(item: &Item) -> String {
    format!(
        "{:<9} 00{:<10}{:<12}",
        item.oz,
        item.unit_price
            .map(|value| gaeb90_scaled(value, 2, 10))
            .unwrap_or_else(|| "          ".to_owned()),
        item.total_price
            .map(|value| gaeb90_scaled(value, 2, 12))
            .unwrap_or_else(|| "            ".to_owned())
    )
}

fn gaeb90_scaled(value: Decimal, scale: u32, width: usize) -> String {
    let factor = Decimal::new(10_i64.pow(scale), 0);
    let digits = (value * factor).round_dp(0).to_string();
    format!("{digits:>width$}")
}

fn gaeb90_item_omissions(item: &Item, omitted_fields: &mut Vec<String>) {
    let add = |field: &str, omitted_fields: &mut Vec<String>| {
        omitted_fields.push(format!("Item {}: {field}", item.oz));
    };
    if item.id.is_some() {
        add("ID", omitted_fields);
    }
    if item.rno_index.is_some() {
        add("RNoIndex", omitted_fields);
    }
    if item
        .source_item_kind
        .as_deref()
        .is_some_and(|value| value.chars().count() != 3)
    {
        add("source item kind", omitted_fields);
    }
    if item.alternative_group.is_some() || item.alternative_serial.is_some() {
        add("alternative assignment", omitted_fields);
    }
    if item.predicted_quantity.is_some() {
        add("predicted quantity", omitted_fields);
    }
    if item.bill_quantity.is_some() {
        add("billed quantity", omitted_fields);
    }
    if item.item_type != ItemType::Normal {
        add("item type", omitted_fields);
    }
    if item.vat.is_some() {
        add("VAT", omitted_fields);
    }
    if item.bim_guid.is_some() {
        add("BIM GUID", omitted_fields);
    }
    if item.change_order_number.is_some() {
        add("change order number", omitted_fields);
    }
    if item.co_status.is_some() {
        add("change order status", omitted_fields);
    }
    if item.discount_pct.is_some() {
        add("discount percentage", omitted_fields);
    }
    if item.markup_type.is_some() {
        add("markup type", omitted_fields);
    }
    if item.not_applicable {
        add("not applicable", omitted_fields);
    }
    if item.hour_item {
        add("hour item", omitted_fields);
    }
    if item.markup_item {
        add("markup item", omitted_fields);
    }
    if item.up_breakdown {
        add("unit price breakdown", omitted_fields);
    }
    if item.sum_description {
        add("sum description", omitted_fields);
    }
    if item.bid_up_down_request.is_some() || item.bid_up_down_pct.is_some() {
        add("bid up/down", omitted_fields);
    }
    if item.reference_description.is_some()
        || item.reference_rno.is_some()
        || item.reference_performance.is_some()
    {
        add("references", omitted_fields);
    }
    if item.up_from.is_some() || item.up_avg.is_some() || item.up_to.is_some() {
        add("unit price range", omitted_fields);
    }
    if !item.up_components.is_empty() {
        add("unit price components", omitted_fields);
    }
    if !item.qty_splits.is_empty() {
        add("quantity splits", omitted_fields);
    }
    if !item.bidder_prices.is_empty() {
        add("bidder prices", omitted_fields);
    }
    if !item.cost_approaches.is_empty() {
        add("cost approaches", omitted_fields);
    }
    if !item.markup_sub_qtys.is_empty() {
        add("markup sub-quantities", omitted_fields);
    }
    if !item.ctlg_assigns.is_empty() {
        add("catalog assignments", omitted_fields);
    }
    if !item.text_complements.is_empty() {
        add("text complements", omitted_fields);
    }
    if !item.sub_descriptions.is_empty() {
        add("sub-descriptions", omitted_fields);
    }
    if item.standard_service.is_some() {
        add("standard service", omitted_fields);
    }
    if item.catalog_number.is_some() {
        add("catalog number", omitted_fields);
    }
    if !item.extensions.is_empty() {
        add("extensions", omitted_fields);
    }
}

fn gaeb90_text_records(value: &str) -> Vec<String> {
    let value = value.replace(['\r', '\n'], " ");
    let chars: Vec<char> = value.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    let mut records = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + 78).min(chars.len());
        if end < chars.len()
            && let Some(offset) = chars[start..end]
                .iter()
                .rposition(|character| character.is_whitespace())
            && offset > 0
        {
            end = start + offset;
        }
        records.push(chars[start..end].iter().collect());
        start = end;
        while start < chars.len() && chars[start].is_whitespace() {
            start += 1;
        }
    }
    records
}

fn fixed(kind: &str, value: &str) -> String {
    let mut line = String::with_capacity(82);
    line.push_str(kind);
    let mut chars = value.chars().take(78).collect::<String>();
    while chars.chars().count() < 78 {
        chars.push(' ');
    }
    line.push_str(&chars);
    line.push_str("\r\n");
    line
}

fn encode_gaeb90(input: &str) -> Result<Vec<u8>, GaebError> {
    input
        .chars()
        .map(|character| {
            if character.is_ascii() {
                Ok(character as u8)
            } else {
                CP850_HIGH
                    .chars()
                    .position(|candidate| candidate == character)
                    .map(|index| (index as u8) + 0x80)
                    .ok_or_else(|| {
                        GaebError::InvalidWriteProfile(format!(
                            "GAEB 90 cannot encode character U+{:04X}",
                            character as u32
                        ))
                    })
            }
        })
        .collect()
}
