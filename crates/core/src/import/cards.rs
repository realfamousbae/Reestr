//! Guesses about card names of the workbook (PLAN §9): «Газпром КР», «Райф М», «Д ВТБ».
//! The import wizard shows them as suggestions; the user confirms or corrects.

use serde::Serialize;

use crate::dict::{ProductKind, name_key};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardGuess {
    pub bank: Option<String>,
    pub kind: Option<ProductKind>,
    pub participant: Option<String>,
}

/// Known short names of banks, by how card names in the workbook start.
const BANKS: &[(&str, &str)] = &[
    ("отп", "ОТП Банк"),
    ("мтс", "МТС Банк"),
    ("алф", "Альфа-Банк"),
    ("альфа", "Альфа-Банк"),
    ("тиньк", "Т-Банк"),
    ("т-банк", "Т-Банк"),
    ("втб", "ВТБ"),
    ("сбер", "Сбербанк"),
    ("гпб", "Газпромбанк"),
    ("газпром", "Газпромбанк"),
    ("якарта", "Яндекс Банк"),
    ("яндекс", "Яндекс Банк"),
    ("озон", "Озон Банк"),
    ("райф", "Райффайзенбанк"),
    ("почта", "Почта Банк"),
    ("совком", "Совкомбанк"),
    ("халва", "Совкомбанк"),
    ("росб", "Росбанк"),
    ("псб", "ПСБ"),
];

/// Suggests the bank, the type and the owner of a card from its short name:
/// a last token «А»/«М» is the first letter of a participant, «КР»/«КК» means a credit card,
/// a leading «Д» a debit card; the bank comes from the beginning of a token.
pub fn guess_card(short_name: &str, participants: &[String]) -> CardGuess {
    let key = name_key(short_name);
    let tokens: Vec<&str> = key.split(' ').collect();

    let kind = if tokens.iter().any(|t| matches!(*t, "кр" | "кк")) {
        Some(ProductKind::Credit)
    } else if tokens.first() == Some(&"д") && tokens.len() > 1 {
        Some(ProductKind::Debit)
    } else {
        None
    };

    let bank = tokens.iter().find_map(|t| {
        BANKS
            .iter()
            .find(|(prefix, _)| t.starts_with(prefix))
            .map(|(_, bank)| (*bank).to_owned())
    });

    let participant = tokens
        .last()
        .filter(|t| tokens.len() > 1 && t.chars().count() == 1)
        .and_then(|letter| {
            let mut matching = participants
                .iter()
                .filter(|p| name_key(p).starts_with(*letter));
            match (matching.next(), matching.next()) {
                (Some(only), None) => Some(only.clone()),
                _ => None,
            }
        });

    CardGuess {
        bank,
        kind,
        participant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> Vec<String> {
        vec!["Андрей".into(), "Марина".into()]
    }

    fn guess(name: &str) -> (Option<&'static str>, Option<ProductKind>, Option<String>) {
        let g = guess_card(name, &people());
        let bank = g
            .bank
            .and_then(|b| BANKS.iter().find(|(_, n)| *n == b).map(|(_, n)| *n));
        (bank, g.kind, g.participant)
    }

    #[test]
    fn names_of_the_workbook() {
        use ProductKind::{Credit, Debit};
        assert_eq!(
            guess("Газпром КР"),
            (Some("Газпромбанк"), Some(Credit), None)
        );
        assert_eq!(
            guess("ВТБ КР А"),
            (Some("ВТБ"), Some(Credit), Some("Андрей".into()))
        );
        assert_eq!(
            guess("Райф М"),
            (Some("Райффайзенбанк"), None, Some("Марина".into()))
        );
        assert_eq!(guess("Д ВТБ"), (Some("ВТБ"), Some(Debit), None));
        assert_eq!(guess("КР Сбер"), (Some("Сбербанк"), Some(Credit), None));
        assert_eq!(guess("ПСБ 3170"), (Some("ПСБ"), None, None));
        assert_eq!(guess("Росб 4512"), (Some("Росбанк"), None, None));
        assert_eq!(
            guess("Почта А"),
            (Some("Почта Банк"), None, Some("Андрей".into()))
        );
        assert_eq!(
            guess("Совком А"),
            (Some("Совкомбанк"), None, Some("Андрей".into()))
        );
        assert_eq!(guess("Кошелёк"), (None, None, None));
        assert_eq!(guess("СС+"), (None, None, None));
    }

    #[test]
    fn ambiguous_letter_gives_no_participant() {
        let people = vec!["Марина".into(), "Максим".into()];
        assert_eq!(guess_card("Райф М", &people).participant, None);
    }
}
