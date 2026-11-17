use std::io::Write;

use chrono::Utc;
use quick_xml::{
    Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{Document, GaebError, InvoiceAddress, InvoiceDocument, InvoiceParty, Item};

const UBL: &str = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2";
const CAC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2";
const CBC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2";
const CUSTOMIZATION_ID: &str =
    "urn:cen.eu:en16931:2017#compliant#urn:xoev-de:kosit:standard:xrechnung_2.0";

/// Options for exporting a GAEB X89/X89B invoice as XRechnung UBL 2.1.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct XRechnungOptions {
    /// Overrides the invoice header number, project number, and fallback number.
    pub invoice_number: Option<String>,
    /// ISO date. Defaults to GAEB invoice date, GAEB document date, or today.
    pub issue_date: Option<String>,
    /// Overrides the contractor address name.
    pub seller_name: Option<String>,
    /// Overrides the owner address name or project name.
    pub buyer_name: Option<String>,
    /// German public-sector buyer reference (Leitweg-ID).
    pub leitweg_id: Option<String>,
    /// VAT percentage. Defaults to the first item VAT value, then 19 percent.
    #[serde(with = "rust_decimal::serde::str_option", default)]
    pub vat_rate: Option<Decimal>,
}

/// Export a GAEB X89/X89B invoice as UTF-8 XRechnung UBL 2.1 XML.
///
/// This produces a submission-oriented EN 16931 core mapping. Validate the
/// result with the official XRechnung schematron before sending it to a portal.
pub fn to_xrechnung_string(
    document: &Document,
    options: &XRechnungOptions,
) -> Result<String, GaebError> {
    let mut output = Vec::new();
    render_xrechnung(document, options, &mut output)?;
    String::from_utf8(output).map_err(|error| GaebError::Xml(error.to_string()))
}

/// Export a GAEB X89/X89B invoice as UTF-8 XRechnung UBL 2.1 bytes.
pub fn to_xrechnung_bytes(
    document: &Document,
    options: &XRechnungOptions,
) -> Result<Vec<u8>, GaebError> {
    let mut output = Vec::new();
    render_xrechnung(document, options, &mut output)?;
    Ok(output)
}

/// Write an XRechnung document to any Rust [`Write`] sink.
///
/// Callers can pass a file, buffer, HTTP response, or any other byte sink
/// without making the core crate depend on a filesystem.
pub fn to_xrechnung<W: Write>(
    document: &Document,
    options: &XRechnungOptions,
    output: &mut W,
) -> Result<(), GaebError> {
    let bytes = to_xrechnung_bytes(document, options)?;
    output.write_all(&bytes).map_err(GaebError::from)
}

fn render_xrechnung(
    document: &Document,
    options: &XRechnungOptions,
    output: &mut Vec<u8>,
) -> Result<(), GaebError> {
    let crate::Document::Procurement(document) = document else {
        return Err(GaebError::Unsupported(
            "XRechnung export requires a procurement X89/X89B document".to_owned(),
        ));
    };
    if !matches!(
        document.meta.phase,
        crate::ExchangePhase::X89 | crate::ExchangePhase::X89B
    ) {
        return Err(GaebError::Unsupported(format!(
            "XRechnung export requires X89 or X89B, got {}",
            document.meta.phase.code()
        )));
    }

    let invoice = document.invoice.as_ref();
    let invoice_number = options
        .invoice_number
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| invoice.and_then(|value| value.header.as_ref()?.invoice_no.as_deref()))
        .or(document.project.id.as_deref())
        .unwrap_or("AUTO-INVOICE");
    let issue_date = options
        .issue_date
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| invoice.and_then(|value| value.header.as_ref()?.invoice_date.as_deref()))
        .or(document.meta.date.as_deref())
        .map(date_only)
        .unwrap_or_else(|| Utc::now().date_naive().to_string());
    let currency = document
        .project
        .currency
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("EUR");
    let vat_rate = options
        .vat_rate
        .or_else(|| first_item_vat(document))
        .unwrap_or_else(|| Decimal::new(19, 0));
    let net_total: Decimal = document
        .boq
        .items()
        .filter(|item| item.item_type.affects_total())
        .filter_map(|item| item.total_price.or_else(|| item.computed_total()))
        .sum();
    let gross_total = invoice
        .and_then(|value| value.total_gross)
        .unwrap_or_else(|| net_total + tax_amount(net_total, vat_rate));
    let vat_amount = gross_total - net_total;
    let seller_name = options
        .seller_name
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| party_name(invoice.and_then(|value| value.contractor.as_ref())))
        .unwrap_or("Auftragnehmer");
    let buyer_name = options
        .buyer_name
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| party_name(invoice.and_then(|value| value.owner.as_ref())))
        .or(document.project.name.as_deref())
        .unwrap_or("Auftraggeber");
    let leitweg_id = options
        .leitweg_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| first_request_id(invoice));

    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    event(
        &mut writer,
        Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)),
    )?;
    start_attrs(
        &mut writer,
        "Invoice",
        &[("xmlns", UBL), ("xmlns:cac", CAC), ("xmlns:cbc", CBC)],
    )?;
    leaf(&mut writer, "cbc:CustomizationID", CUSTOMIZATION_ID, &[])?;
    leaf(&mut writer, "cbc:ID", invoice_number, &[])?;
    leaf(&mut writer, "cbc:IssueDate", &issue_date, &[])?;
    leaf(
        &mut writer,
        "cbc:InvoiceTypeCode",
        if invoice
            .and_then(|value| value.header.as_ref())
            .is_some_and(|header| header.credit_note)
        {
            "381"
        } else {
            "380"
        },
        &[],
    )?;
    leaf(&mut writer, "cbc:DocumentCurrencyCode", currency, &[])?;
    if let Some(leitweg_id) = leitweg_id {
        leaf(&mut writer, "cbc:BuyerReference", leitweg_id, &[])?;
    }

    if let Some(reference) = invoice
        .and_then(|value| value.header.as_ref())
        .and_then(|header| header.reference_invoice_no.as_deref())
    {
        start(&mut writer, "cac:AdditionalDocumentReference")?;
        leaf(&mut writer, "cbc:ID", reference, &[])?;
        end(&mut writer, "cac:AdditionalDocumentReference")?;
    }
    if let Some(award_no) = invoice
        .and_then(|value| value.owner.as_ref())
        .and_then(|party| party.award_no.as_deref())
    {
        start(&mut writer, "cac:OrderReference")?;
        leaf(&mut writer, "cbc:ID", award_no, &[])?;
        end(&mut writer, "cac:OrderReference")?;
    }

    write_party(
        &mut writer,
        "cac:AccountingSupplierParty",
        invoice.and_then(|value| value.contractor.as_ref()),
        seller_name,
    )?;
    write_party(
        &mut writer,
        "cac:AccountingCustomerParty",
        invoice.and_then(|value| value.owner.as_ref()),
        buyer_name,
    )?;

    if let Some(header) = invoice.and_then(|value| value.header.as_ref())
        && (!header.service_provision_start_date.is_empty()
            || !header.service_provision_end_date.is_empty())
    {
        start(&mut writer, "cac:InvoicePeriod")?;
        if !header.service_provision_start_date.is_empty() {
            leaf(
                &mut writer,
                "cbc:StartDate",
                &date_only(&header.service_provision_start_date),
                &[],
            )?;
        }
        if !header.service_provision_end_date.is_empty() {
            leaf(
                &mut writer,
                "cbc:EndDate",
                &date_only(&header.service_provision_end_date),
                &[],
            )?;
        }
        end(&mut writer, "cac:InvoicePeriod")?;
    }

    write_payment(&mut writer, invoice)?;

    start(&mut writer, "cac:TaxTotal")?;
    leaf_currency(&mut writer, "cbc:TaxAmount", vat_amount, currency)?;
    start(&mut writer, "cac:TaxSubtotal")?;
    leaf_currency(&mut writer, "cbc:TaxableAmount", net_total, currency)?;
    leaf_currency(&mut writer, "cbc:TaxAmount", vat_amount, currency)?;
    write_tax_category(&mut writer, vat_rate, "S")?;
    end(&mut writer, "cac:TaxSubtotal")?;
    end(&mut writer, "cac:TaxTotal")?;

    start(&mut writer, "cac:LegalMonetaryTotal")?;
    leaf_currency(&mut writer, "cbc:LineExtensionAmount", net_total, currency)?;
    leaf_currency(&mut writer, "cbc:TaxExclusiveAmount", net_total, currency)?;
    leaf_currency(&mut writer, "cbc:TaxInclusiveAmount", gross_total, currency)?;
    leaf_currency(&mut writer, "cbc:PayableAmount", gross_total, currency)?;
    end(&mut writer, "cac:LegalMonetaryTotal")?;

    let mut line_number = 0;
    for item in document.boq.items() {
        let line_total = item.total_price.or_else(|| item.computed_total());
        let Some(line_total) = line_total else {
            continue;
        };
        line_number += 1;
        write_invoice_line(
            &mut writer,
            item,
            line_number,
            line_total,
            currency,
            vat_rate,
        )?;
    }

    end(&mut writer, "Invoice")?;
    *output = writer.into_inner();
    Ok(())
}

fn write_party(
    writer: &mut Writer<Vec<u8>>,
    container: &str,
    party: Option<&InvoiceParty>,
    fallback_name: &str,
) -> Result<(), GaebError> {
    start(writer, container)?;
    start(writer, "cac:Party")?;
    if !fallback_name.is_empty() {
        start(writer, "cac:PartyName")?;
        leaf(writer, "cbc:Name", fallback_name, &[])?;
        end(writer, "cac:PartyName")?;
    }
    if let Some(address) = party.and_then(|value| value.address.as_ref()) {
        write_address(writer, address)?;
        if let Some(vat_id) = address.vat_id.as_deref().filter(|value| !value.is_empty()) {
            start(writer, "cac:PartyTaxScheme")?;
            leaf(writer, "cbc:CompanyID", vat_id, &[])?;
            start(writer, "cac:TaxScheme")?;
            leaf(writer, "cbc:ID", "VAT", &[])?;
            end(writer, "cac:TaxScheme")?;
            end(writer, "cac:PartyTaxScheme")?;
        }
        if let Some(email) = address.email.as_deref().filter(|value| !value.is_empty()) {
            start(writer, "cac:Contact")?;
            leaf(writer, "cbc:ElectronicMail", email, &[])?;
            end(writer, "cac:Contact")?;
        }
    }
    start(writer, "cac:PartyLegalEntity")?;
    leaf(writer, "cbc:RegistrationName", fallback_name, &[])?;
    end(writer, "cac:PartyLegalEntity")?;
    end(writer, "cac:Party")?;
    end(writer, container)
}

fn write_address(writer: &mut Writer<Vec<u8>>, address: &InvoiceAddress) -> Result<(), GaebError> {
    start(writer, "cac:PostalAddress")?;
    if !address.street.is_empty() {
        leaf(writer, "cbc:StreetName", &address.street, &[])?;
    }
    if !address.city.is_empty() {
        leaf(writer, "cbc:CityName", &address.city, &[])?;
    }
    if !address.postal_code.is_empty() {
        leaf(writer, "cbc:PostalZone", &address.postal_code, &[])?;
    }
    if let Some(country) = address.country.as_deref().filter(|value| !value.is_empty()) {
        start(writer, "cac:Country")?;
        leaf(writer, "cbc:IdentificationCode", country, &[])?;
        end(writer, "cac:Country")?;
    }
    end(writer, "cac:PostalAddress")
}

fn write_payment(
    writer: &mut Writer<Vec<u8>>,
    invoice: Option<&InvoiceDocument>,
) -> Result<(), GaebError> {
    let bank = invoice
        .and_then(|value| value.contractor.as_ref())
        .and_then(|party| party.address.as_ref())
        .and_then(|address| address.banks.iter().find(|bank| bank.iban.is_some()));
    if let Some(bank) = bank {
        start(writer, "cac:PaymentMeans")?;
        leaf(writer, "cbc:PaymentMeansCode", "58", &[])?;
        start(writer, "cac:PayeeFinancialAccount")?;
        leaf(
            writer,
            "cbc:ID",
            bank.iban.as_deref().unwrap_or_default(),
            &[],
        )?;
        if !bank.label.is_empty() {
            leaf(writer, "cbc:Name", &bank.label, &[])?;
        }
        if let Some(bic) = bank.bic.as_deref().filter(|value| !value.is_empty()) {
            start(writer, "cac:FinancialInstitutionBranch")?;
            leaf(writer, "cbc:ID", bic, &[])?;
            end(writer, "cac:FinancialInstitutionBranch")?;
        }
        end(writer, "cac:PayeeFinancialAccount")?;
        end(writer, "cac:PaymentMeans")?;
    }
    if let Some(term) = invoice.and_then(|value| value.payment_terms.first()) {
        let note = if let Some(days) = term.cash_discount_days {
            format!("{}% discount within {} days", term.discount_pct, days)
        } else if let Some(date) = term.cash_discount_date.as_deref() {
            format!("{}% discount until {}", term.discount_pct, date)
        } else {
            format!("{}% discount", term.discount_pct)
        };
        start(writer, "cac:PaymentTerms")?;
        leaf(writer, "cbc:Note", &note, &[])?;
        end(writer, "cac:PaymentTerms")?;
    }
    Ok(())
}

fn write_invoice_line(
    writer: &mut Writer<Vec<u8>>,
    item: &Item,
    line_number: usize,
    line_total: Decimal,
    currency: &str,
    vat_rate: Decimal,
) -> Result<(), GaebError> {
    start(writer, "cac:InvoiceLine")?;
    leaf(writer, "cbc:ID", &line_number.to_string(), &[])?;
    if let Some(quantity) = item.bill_quantity.or(item.quantity) {
        leaf_attrs(
            writer,
            "cbc:InvoicedQuantity",
            &quantity.to_string(),
            &[("unitCode", ubl_unit(item.unit.as_deref()))],
        )?;
    }
    leaf_currency(writer, "cbc:LineExtensionAmount", line_total, currency)?;
    start(writer, "cac:Item")?;
    let description = item.long_text_plain();
    if !description.is_empty() {
        leaf(writer, "cbc:Description", description, &[])?;
    }
    let name = if item.short_text.is_empty() {
        item.full_oz()
    } else {
        item.short_text.clone()
    };
    leaf(
        writer,
        "cbc:Name",
        &name.chars().take(200).collect::<String>(),
        &[],
    )?;
    start(writer, "cac:SellersItemIdentification")?;
    leaf(writer, "cbc:ID", &item.full_oz(), &[])?;
    end(writer, "cac:SellersItemIdentification")?;
    write_tax_category(writer, vat_rate, "S")?;
    end(writer, "cac:Item")?;
    if let Some(unit_price) = item.unit_price {
        start(writer, "cac:Price")?;
        leaf_currency(writer, "cbc:PriceAmount", unit_price, currency)?;
        leaf(writer, "cbc:BaseQuantity", "1", &[])?;
        end(writer, "cac:Price")?;
    }
    end(writer, "cac:InvoiceLine")
}

fn write_tax_category(
    writer: &mut Writer<Vec<u8>>,
    vat_rate: Decimal,
    category_id: &str,
) -> Result<(), GaebError> {
    start(writer, "cac:TaxCategory")?;
    leaf(writer, "cbc:ID", category_id, &[])?;
    leaf(writer, "cbc:Percent", &vat_rate.to_string(), &[])?;
    start(writer, "cac:TaxScheme")?;
    leaf(writer, "cbc:ID", "VAT", &[])?;
    end(writer, "cac:TaxScheme")?;
    end(writer, "cac:TaxCategory")
}

fn leaf_currency(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    value: Decimal,
    currency: &str,
) -> Result<(), GaebError> {
    leaf_attrs(writer, name, &money(value), &[("currencyID", currency)])
}

fn leaf(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    text: &str,
    attrs: &[(&str, &str)],
) -> Result<(), GaebError> {
    leaf_attrs(writer, name, text, attrs)
}

fn leaf_attrs(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    text: &str,
    attrs: &[(&str, &str)],
) -> Result<(), GaebError> {
    start_attrs(writer, name, attrs)?;
    event(writer, Event::Text(BytesText::new(text)))?;
    end(writer, name)
}

fn start(writer: &mut Writer<Vec<u8>>, name: &str) -> Result<(), GaebError> {
    start_attrs(writer, name, &[])
}

fn start_attrs(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    attrs: &[(&str, &str)],
) -> Result<(), GaebError> {
    let mut element = BytesStart::new(name);
    for (key, value) in attrs {
        element.push_attribute((*key, *value));
    }
    event(writer, Event::Start(element))
}

fn end(writer: &mut Writer<Vec<u8>>, name: &str) -> Result<(), GaebError> {
    event(writer, Event::End(BytesEnd::new(name)))
}

fn event(writer: &mut Writer<Vec<u8>>, event: Event<'_>) -> Result<(), GaebError> {
    writer
        .write_event(event)
        .map_err(|error| GaebError::Xml(error.to_string()))
}

fn party_name(party: Option<&InvoiceParty>) -> Option<&str> {
    party
        .and_then(|party| party.address.as_ref())
        .map(|address| address.name1.as_str())
        .filter(|name| !name.is_empty())
}

fn first_request_id(invoice: Option<&InvoiceDocument>) -> Option<&str> {
    invoice
        .and_then(|invoice| invoice.requesters.first())
        .and_then(|party| party.request_id_no.as_deref())
        .filter(|value| !value.is_empty())
}

fn first_item_vat(document: &crate::ProcurementDocument) -> Option<Decimal> {
    document
        .boq
        .items()
        .filter_map(|item| item.vat)
        .find(|value| *value >= Decimal::ZERO)
}

fn tax_amount(net: Decimal, rate: Decimal) -> Decimal {
    (net * rate / Decimal::new(100, 0)).round_dp(2)
}

fn money(value: Decimal) -> String {
    value.round_dp(2).to_string()
}

fn date_only(value: &str) -> String {
    value.split('T').next().unwrap_or(value).to_owned()
}

fn ubl_unit(unit: Option<&str>) -> &'static str {
    match unit.map(str::trim) {
        Some("m") | Some("lfm") => "MTR",
        Some("m²") | Some("m2") => "MTK",
        Some("m³") | Some("m3") => "MTQ",
        Some("kg") => "KGM",
        Some("t") => "TNE",
        Some("Stk") => "H87",
        Some("h") => "HUR",
        Some("psch") => "LS",
        _ => "C62",
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::{
        Boq, BoqCategory, DocumentMeta, ExchangePhase, Item, ItemType, ProcurementDocument,
        ProjectInfo,
    };

    fn invoice_document() -> Document {
        let mut item = Item {
            oz: "01.0010".to_owned(),
            short_text: "Concrete & steel".to_owned(),
            long_text: "Foundation work".to_owned(),
            quantity: Some(Decimal::new(2, 0)),
            unit: Some("m3".to_owned()),
            unit_price: Some(Decimal::new(100, 0)),
            total_price: Some(Decimal::new(200, 0)),
            item_type: ItemType::Normal,
            vat: Some(Decimal::new(19, 0)),
            ..Item::default()
        };
        item.bill_quantity = Some(Decimal::new(3, 0));
        let mut document = ProcurementDocument {
            meta: DocumentMeta {
                phase: ExchangePhase::X89,
                date: Some("2025-01-02".to_owned()),
                ..DocumentMeta::default()
            },
            project: ProjectInfo {
                id: Some("PROJECT-1".to_owned()),
                currency: Some("EUR".to_owned()),
                ..ProjectInfo::default()
            },
            boq: Boq {
                categories: vec![BoqCategory {
                    items: vec![item],
                    ..BoqCategory::default()
                }],
                ..Boq::default()
            },
            invoice: Some(InvoiceDocument {
                header: Some(crate::InvoiceHeader {
                    invoice_no: Some("INV-1".to_owned()),
                    invoice_date: Some("2025-01-03".to_owned()),
                    service_provision_start_date: "2024-12-01".to_owned(),
                    service_provision_end_date: "2024-12-31".to_owned(),
                    ..crate::InvoiceHeader::default()
                }),
                total_gross: Some(Decimal::new(2380, 1)),
                ..InvoiceDocument::default()
            }),
        };
        document.invoice.as_mut().unwrap().contractor = Some(crate::InvoiceParty {
            address: Some(crate::InvoiceAddress {
                name1: "Seller & Co".to_owned(),
                street: "Main <1>".to_owned(),
                postal_code: "12345".to_owned(),
                city: "Berlin".to_owned(),
                vat_id: Some("DE123".to_owned()),
                banks: vec![crate::InvoiceBank {
                    label: "Seller Bank".to_owned(),
                    iban: Some("DE123".to_owned()),
                    bic: Some("BANKDEFF".to_owned()),
                    ..crate::InvoiceBank::default()
                }],
                ..crate::InvoiceAddress::default()
            }),
            ..crate::InvoiceParty::default()
        });
        document.invoice.as_mut().unwrap().owner = Some(crate::InvoiceParty {
            address: Some(crate::InvoiceAddress {
                name1: "Buyer".to_owned(),
                ..crate::InvoiceAddress::default()
            }),
            ..crate::InvoiceParty::default()
        });
        Document::Procurement(document)
    }

    #[test]
    fn exports_invoice_with_ubl_structure_and_escaped_values() {
        let xml = to_xrechnung_string(
            &invoice_document(),
            &XRechnungOptions {
                leitweg_id: Some("991-123".to_owned()),
                ..XRechnungOptions::default()
            },
        )
        .unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("cbc:CustomizationID"));
        assert!(xml.contains("INV-1"));
        assert!(xml.contains("991-123"));
        assert!(xml.contains("Seller &amp; Co"));
        assert!(xml.contains("Main &lt;1&gt;"));
        assert!(xml.contains("currencyID=\"EUR\""));
        assert!(xml.contains("unitCode=\"MTQ\""));
        assert!(xml.contains("DE123"));
        assert!(xml.contains("BANKDEFF"));
        assert!(xml.contains("TaxInclusiveAmount"));
        assert!(xml.contains("InvoicePeriod"));
    }

    #[test]
    fn writer_api_matches_string_api() {
        let options = XRechnungOptions {
            invoice_number: Some("INV-OVERRIDE".to_owned()),
            issue_date: Some("2025-04-05".to_owned()),
            seller_name: Some("Acme Bau GmbH".to_owned()),
            buyer_name: Some("City Authority".to_owned()),
            leitweg_id: Some("04011000-1234567890-12".to_owned()),
            vat_rate: Some(Decimal::new(7, 0)),
        };
        let expected = to_xrechnung_string(&invoice_document(), &options).unwrap();
        let mut output = Cursor::new(Vec::new());
        to_xrechnung(&invoice_document(), &options, &mut output).unwrap();
        assert_eq!(String::from_utf8(output.into_inner()).unwrap(), expected);
        assert!(expected.contains("INV-OVERRIDE"));
        assert!(expected.contains("2025-04-05"));
        assert!(expected.contains("<cbc:Percent>7</cbc:Percent>"));
    }

    #[test]
    fn emits_invoice_fields_for_lines_and_parties() {
        let xml = to_xrechnung_string(&invoice_document(), &XRechnungOptions::default()).unwrap();
        assert!(xml.contains("<cbc:Description>Foundation work</cbc:Description>"));
        assert!(xml.contains("<cbc:Name>Concrete &amp; steel</cbc:Name>"));
        assert!(xml.contains("<cbc:ID>01.0010</cbc:ID>"));
        assert!(xml.contains("<cac:Price>"));
        assert!(xml.contains("<cac:PartyName>"));
        assert!(xml.contains("<cbc:RegistrationName>Seller &amp; Co</cbc:RegistrationName>"));
    }

    #[test]
    fn accepts_x89b_invoice_phase() {
        let mut document = invoice_document();
        if let Document::Procurement(document) = &mut document {
            document.meta.phase = ExchangePhase::X89B;
        }
        to_xrechnung_bytes(&document, &XRechnungOptions::default()).unwrap();
    }

    #[test]
    fn rejects_non_invoice_phase() {
        let mut document = invoice_document();
        if let Document::Procurement(document) = &mut document {
            document.meta.phase = ExchangePhase::X86;
        }
        let error = to_xrechnung_string(&document, &XRechnungOptions::default()).unwrap_err();
        assert!(error.to_string().contains("requires X89 or X89B"));
    }
}
