use crate::{Document, ExchangePhase, GaebError};

pub fn tender_to_bid(source: &Document) -> Result<Document, GaebError> {
    if source.phase() != ExchangePhase::X83 {
        return Err(GaebError::Unsupported(
            "tender_to_bid requires an X83 procurement document".to_owned(),
        ));
    }
    let mut document = source.clone();
    let Document::Procurement(document) = &mut document else {
        return Err(GaebError::Unsupported(
            "tender_to_bid requires a procurement document".to_owned(),
        ));
    };
    document.meta.phase = ExchangePhase::X84;
    document.meta.program = Some("gaeb-core".to_owned());
    document.meta.warnings.clear();
    for item in document.boq.items_mut() {
        item.unit_price = None;
        item.total_price = None;
    }
    Ok(Document::Procurement(document.clone()))
}

pub fn contract_to_addendum(
    source: &Document,
    change_order: Option<&str>,
) -> Result<Document, GaebError> {
    if source.phase() != ExchangePhase::X86 {
        return Err(GaebError::Unsupported(
            "contract_to_addendum requires an X86 procurement document".to_owned(),
        ));
    }
    let mut document = source.clone();
    let Document::Procurement(document) = &mut document else {
        return Err(GaebError::Unsupported(
            "contract_to_addendum requires a procurement document".to_owned(),
        ));
    };
    document.meta.phase = ExchangePhase::X88;
    document.meta.program = Some("gaeb-core".to_owned());
    document.meta.warnings.clear();
    if let Some(change_order) = change_order.filter(|value| !value.is_empty()) {
        for item in document.boq.items_mut() {
            item.change_order_number = Some(change_order.to_owned());
        }
    }
    Ok(Document::Procurement(document.clone()))
}

pub fn contract_to_invoice(source: &Document) -> Result<Document, GaebError> {
    if source.phase() != ExchangePhase::X86 {
        return Err(GaebError::Unsupported(
            "contract_to_invoice requires an X86 procurement document".to_owned(),
        ));
    }
    let mut document = source.clone();
    let Document::Procurement(document) = &mut document else {
        return Err(GaebError::Unsupported(
            "contract_to_invoice requires a procurement document".to_owned(),
        ));
    };
    document.meta.phase = ExchangePhase::X89;
    document.meta.program = Some("gaeb-core".to_owned());
    document.meta.warnings.clear();
    Ok(Document::Procurement(document.clone()))
}
