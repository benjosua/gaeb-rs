use regex::Regex;
use rust_decimal::Decimal;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedTakeoff {
    pub description: String,
    pub dimensions: Vec<Decimal>,
    pub operator: String,
    pub computed_qty: Option<Decimal>,
    pub unit: String,
    pub formula: String,
}

pub fn parse_reb_row(raw: &str) -> ParsedTakeoff {
    let mut result = ParsedTakeoff::default();
    let text = raw.trim();
    if text.is_empty() {
        return result;
    }

    static NUMBER_RE: OnceLock<Regex> = OnceLock::new();
    let number_re = NUMBER_RE.get_or_init(|| Regex::new(r"-?\d+(?:[.,]\d+)?").unwrap());

    static RESULT_RE: OnceLock<Regex> = OnceLock::new();
    let result_re =
        RESULT_RE.get_or_init(|| Regex::new(r"=\s*(-?\d+(?:[.,]\d+)?)\s*(\S*)\s*$").unwrap());

    static MUL_SPLIT_RE: OnceLock<Regex> = OnceLock::new();
    let mul_split_re = MUL_SPLIT_RE.get_or_init(|| Regex::new(r"\s*[*xX\u00d7]\s*").unwrap());

    let known_units = ["m3", "m2", "lfm", "Stk", "psch", "kg", "m"];

    let formula_part = if let Some(caps) = result_re.captures(text) {
        let qty_str = caps.get(1).unwrap().as_str();
        result.computed_qty = to_decimal(qty_str);
        result.unit = caps.get(2).unwrap().as_str().to_string();
        text[..caps.get(0).unwrap().start()].trim()
    } else {
        text
    };

    let numbers: Vec<_> = number_re
        .find_iter(formula_part)
        .map(|m| m.as_str())
        .collect();
    if numbers.is_empty() {
        result.description = text.to_string();
        return result;
    }

    let first_num_match = number_re.find(formula_part).unwrap();
    result.description = formula_part[..first_num_match.start()].trim().to_string();
    let formula_str = formula_part[first_num_match.start()..].trim();

    if !formula_str.is_empty() {
        let mut stripped_formula = formula_str.to_string();
        for unit in known_units {
            if stripped_formula.ends_with(unit) {
                result.unit = unit.to_string();
                let new_len = stripped_formula.len() - unit.len();
                stripped_formula.truncate(new_len);
                stripped_formula = stripped_formula.trim().to_string();
                break;
            }
        }

        result.formula = formula_str.to_string();
        if let Some(caps) = result_re.captures(text) {
            result.formula.push_str(" = ");
            result.formula.push_str(caps.get(1).unwrap().as_str());
            if !result.unit.is_empty() {
                result.formula.push(' ');
                result.formula.push_str(&result.unit);
            }
        }

        for part in mul_split_re.split(&stripped_formula) {
            if let Some(d) = to_decimal(part.trim()) {
                result.dimensions.push(d);
            }
        }

        if result.dimensions.len() > 1 {
            result.operator = "*".to_string();
        }
    }

    if result.computed_qty.is_none() && !result.dimensions.is_empty() {
        let mut product = Decimal::new(1, 0);
        for d in &result.dimensions {
            product *= d;
        }
        result.computed_qty = Some(product);
    }

    if result.dimensions.is_empty()
        && let Some(qty) = numbers.last().and_then(|number| to_decimal(number))
    {
        result.dimensions = vec![qty];
        if result.computed_qty.is_none() {
            result.computed_qty = Some(qty);
        }
    }

    result
}

fn to_decimal(s: &str) -> Option<Decimal> {
    Decimal::from_str_exact(&s.replace(",", ".")).ok()
}
