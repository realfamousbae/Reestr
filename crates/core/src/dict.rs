//! Dictionaries (PLAN §5): participants, banks, products, categories and their groups,
//! regular payment templates. Ids are SQLite row ids.

use serde::{Deserialize, Serialize};
use time::Date;

use crate::cashback::Rounding;
use crate::money::Kopecks;

pub type ParticipantId = i64;
pub type BankId = i64;
pub type ProductId = i64;
pub type CategoryId = i64;
pub type GroupId = i64;
pub type TemplateId = i64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub id: ParticipantId,
    pub name: String,
    pub sort_order: i64,
    pub is_active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bank {
    pub id: BankId,
    pub name: String,
    pub short_name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductKind {
    Debit,
    Credit,
    Savings,
    Deposit,
    Cash,
}

impl ProductKind {
    pub const ALL: [Self; 5] = [
        Self::Debit,
        Self::Credit,
        Self::Savings,
        Self::Deposit,
        Self::Cash,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
            Self::Savings => "savings",
            Self::Deposit => "deposit",
            Self::Cash => "cash",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.code() == code)
    }
}

/// A bank product: a card, a savings account, a deposit, cash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: ProductId,
    pub bank_id: Option<BankId>,
    pub participant_id: Option<ParticipantId>,
    /// `None` until the user says what it is (a card typed into the journal for the first time).
    pub kind: Option<ProductKind>,
    /// As typed in the «Карта» column: «Газпром КР», «ПСБ 7741».
    pub short_name: String,
    pub full_name: Option<String>,
    /// How cashback is rounded for this product (D13).
    pub cashback_rounding: Rounding,
    pub is_active: bool,
    #[serde(with = "crate::date::iso::option")]
    pub opened_at: Option<Date>,
    #[serde(with = "crate::date::iso::option")]
    pub closed_at: Option<Date>,
    pub notes: Option<String>,
}

/// An optional roll-up of categories: «Здоровье» = лечение + лекарства (PLAN §8).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryGroup {
    pub id: GroupId,
    pub name: String,
    pub sort_order: i64,
}

/// A category exactly as the user names it: «еда», «продукты», «лечение».
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub group_id: Option<GroupId>,
    pub is_regular_default: bool,
    pub sort_order: i64,
    pub is_active: bool,
}

/// A regular payment that is entered every month: rent, internet, a club.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegularTemplate {
    pub id: TemplateId,
    pub title: String,
    pub category_id: Option<CategoryId>,
    pub participant_id: Option<ParticipantId>,
    pub product_id: Option<ProductId>,
    pub default_amount: Option<Kopecks>,
    pub sort_order: i64,
    pub is_active: bool,
    /// «Не в Ср. %»: journal rows of this name stay out of the base of «Ср. %», like rent,
    /// which earns no cashback (D19, D37).
    pub exclude_from_rate: bool,
}

/// Which dictionary an edit is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DictKind {
    Participant,
    Bank,
    Product,
    Group,
    Category,
    Template,
}

impl DictKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Participant => "participant",
            Self::Bank => "bank",
            Self::Product => "product",
            Self::Group => "group",
            Self::Category => "category",
            Self::Template => "template",
        }
    }
}

/// A change of one dictionary field as the UI sends it. References to other dictionaries are
/// names; missing ones are created (D17).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum DictInput {
    /// The name; a product's short name; a template's title.
    Name(String),
    /// A bank's short name.
    ShortName(Option<String>),
    FullName(Option<String>),
    Notes(Option<String>),
    IsActive(bool),
    IsRegularDefault(bool),
    ExcludeFromRate(bool),
    SortOrder(i64),
    Group(Option<String>),
    Bank(Option<String>),
    Participant(Option<String>),
    Category(Option<String>),
    Card(Option<String>),
    Kind(Option<ProductKind>),
    Rounding(Rounding),
    DefaultAmount(Option<Kopecks>),
    #[serde(with = "crate::date::iso::option")]
    OpenedAt(Option<Date>),
    #[serde(with = "crate::date::iso::option")]
    ClosedAt(Option<Date>),
}

impl DictInput {
    pub const fn field(&self) -> &'static str {
        match self {
            Self::Name(_) => "name",
            Self::ShortName(_) => "shortName",
            Self::FullName(_) => "fullName",
            Self::Notes(_) => "notes",
            Self::IsActive(_) => "isActive",
            Self::IsRegularDefault(_) => "isRegularDefault",
            Self::ExcludeFromRate(_) => "excludeFromRate",
            Self::SortOrder(_) => "sortOrder",
            Self::Group(_) => "group",
            Self::Bank(_) => "bank",
            Self::Participant(_) => "participant",
            Self::Category(_) => "category",
            Self::Card(_) => "card",
            Self::Kind(_) => "kind",
            Self::Rounding(_) => "rounding",
            Self::DefaultAmount(_) => "defaultAmount",
            Self::OpenedAt(_) => "openedAt",
            Self::ClosedAt(_) => "closedAt",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum DictOp {
    Edit {
        id: i64,
        edit: DictInput,
    },
    /// A new item; `edits` must include the name.
    Append {
        edits: Vec<DictInput>,
    },
    Delete {
        ids: Vec<i64>,
    },
}

/// The key two names are compared by: case-insensitive for Cyrillic too, «ё» equal to «е»,
/// surrounding and repeated spaces ignored. SQLite's `NOCASE` only folds ASCII.
pub fn name_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .replace('ё', "е")
}

/// Categories a new file starts with, in the column order of the month header, with the
/// groups of its merged totals columns. All editable.
/// Each item is (category, group).
pub const DEFAULT_CATEGORIES: &[(&str, Option<&str>)] = &[
    ("еда", None),
    ("продукты", None),
    ("обучение", None),
    ("лечение", Some("Лечение/лекарства")),
    ("лекарства", Some("Лечение/лекарства")),
    ("автомобиль", None),
    ("транспорт", None),
    ("семья", None),
    ("красота", Some("Красота/косметика")),
    ("косметика", Some("Красота/косметика")),
    ("одежда", Some("Одежда/обувь")),
    ("обувь", Some("Одежда/обувь")),
    ("разное", None),
    ("отдых", None),
    ("квартиры", None),
    ("дача", None),
];

/// The category of row 3 of a month sheet — rent (D16).
pub const RENT_CATEGORY: &str = "квартиры";

/// The regular payment of row 3 — rent. New files have it, out of «Ср. %» (D37); in the xlsx
/// layout `M7/(H2−G3)` leaves row 3 out.
pub const RENT_TITLE: &str = "Квартплата";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_keys() {
        assert_eq!(name_key("  Пятёрочка  "), "пятерочка");
        assert_eq!(name_key("Газпром   КР"), "газпром кр");
        assert_eq!(name_key("Транспорт"), name_key("транспорт"));
        assert_eq!(name_key("ЁЛКА"), "елка");
    }

    #[test]
    fn product_kind_codes() {
        for kind in ProductKind::ALL {
            assert_eq!(ProductKind::from_code(kind.code()), Some(kind));
        }
        assert_eq!(ProductKind::from_code("loan"), None);
    }
}
