//! Month totals: the row of sums above the journal (PLAN §8): «Итого», one column per category
//! (or per group), «Регулярные».

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::cashback::effective_rate_bp;
use crate::dict::{Category, CategoryGroup, CategoryId, RegularTemplate, name_key};
use crate::journal::Entry;
use crate::money::Kopecks;

/// One column of the totals band: a category, or a group of categories.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotalColumn {
    /// Category id, or group id when `is_group`.
    pub id: i64,
    pub is_group: bool,
    pub label: String,
    pub amount: Kopecks,
    pub category_ids: Vec<CategoryId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthTotals {
    /// «Итого» — every entry of the month.
    pub total: Kopecks,
    /// One column per category, in dictionary order.
    pub by_category: Vec<TotalColumn>,
    /// Groups instead of their categories; a group stands where its first category stands,
    /// ungrouped categories stay as themselves. The default groups are the merged columns:
    /// «Лечение» = лечение + лекарства, «Красота/косметика», «Одежда/обувь».
    pub by_group: Vec<TotalColumn>,
    /// Entries without a category.
    pub uncategorized: Kopecks,
    /// «Регулярные».
    pub regular: Kopecks,
    /// «Итого кэшбэк».
    pub cashback: Kopecks,
    /// «Ср. %» — cashback relative to the total without the rows kept out of it, like rent
    /// (D19, D37).
    pub cashback_rate_bp: Option<i64>,
}

/// `categories` and `groups` may come in any order; columns follow `sort_order`. Inactive
/// categories get a column only when the month has entries in them. Rows named like a regular
/// payment marked «Не в Ср. %» stay out of the base of «Ср. %» (D37).
pub fn month_totals<'a>(
    entries: impl IntoIterator<Item = &'a Entry>,
    categories: &[Category],
    groups: &[CategoryGroup],
    templates: &[RegularTemplate],
) -> MonthTotals {
    let outside_rate: HashSet<String> = templates
        .iter()
        .filter(|t| t.exclude_from_rate)
        .map(|t| name_key(&t.title))
        .collect();
    let mut by_id: HashMap<CategoryId, Kopecks> = HashMap::new();
    let mut total = Kopecks::ZERO;
    let mut uncategorized = Kopecks::ZERO;
    let mut regular = Kopecks::ZERO;
    let mut cashback = Kopecks::ZERO;
    let mut excluded = Kopecks::ZERO;

    for entry in entries {
        total += entry.amount;
        cashback += entry.cashback_amount();
        if entry.is_regular {
            regular += entry.amount;
        }
        if !outside_rate.is_empty() && outside_rate.contains(&name_key(&entry.title)) {
            excluded += entry.amount;
        }
        match entry.category_id {
            Some(id) if categories.iter().any(|c| c.id == id) => {
                *by_id.entry(id).or_default() += entry.amount;
            }
            _ => uncategorized += entry.amount,
        }
    }

    let mut ordered: Vec<&Category> = categories
        .iter()
        .filter(|c| c.is_active || by_id.contains_key(&c.id))
        .collect();
    ordered.sort_by_key(|c| (c.sort_order, c.id));
    let amount_of = |id: CategoryId| by_id.get(&id).copied().unwrap_or_default();

    let by_category = ordered
        .iter()
        .map(|c| TotalColumn {
            id: c.id,
            is_group: false,
            label: c.name.clone(),
            amount: amount_of(c.id),
            category_ids: vec![c.id],
        })
        .collect();

    let mut by_group: Vec<TotalColumn> = Vec::new();
    for category in &ordered {
        let group = category
            .group_id
            .and_then(|gid| groups.iter().find(|g| g.id == gid));
        match group {
            None => by_group.push(TotalColumn {
                id: category.id,
                is_group: false,
                label: category.name.clone(),
                amount: amount_of(category.id),
                category_ids: vec![category.id],
            }),
            Some(group) => {
                if let Some(column) = by_group.iter_mut().find(|c| c.is_group && c.id == group.id) {
                    column.amount += amount_of(category.id);
                    column.category_ids.push(category.id);
                } else {
                    by_group.push(TotalColumn {
                        id: group.id,
                        is_group: true,
                        label: group.name.clone(),
                        amount: amount_of(category.id),
                        category_ids: vec![category.id],
                    });
                }
            }
        }
    }

    MonthTotals {
        total,
        by_category,
        by_group,
        uncategorized,
        regular,
        cashback,
        cashback_rate_bp: effective_rate_bp(total - excluded, cashback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashback::Rounding;
    use crate::journal::EntryEdit;
    use crate::month::Month;

    fn entry(id: i64, category: Option<i64>, rubles: i64, regular: bool) -> Entry {
        let month: Month = "2026-07".parse().unwrap();
        let mut e = Entry::new(id, month);
        e.category_id = category;
        e.amount = Kopecks::from_rubles(rubles);
        e.is_regular = regular;
        e
    }

    fn category(id: i64, name: &str, group: Option<i64>, sort: i64) -> Category {
        Category {
            id,
            name: name.into(),
            group_id: group,
            is_regular_default: false,
            sort_order: sort,
            is_active: true,
        }
    }

    /// A month header: Еда, Продукты, Лечение(+лекарства), Транспорт, Красота/косметика.
    fn dictionary() -> (Vec<Category>, Vec<CategoryGroup>) {
        let categories = vec![
            category(1, "еда", None, 1),
            category(2, "продукты", None, 2),
            category(3, "лечение", Some(100), 3),
            category(4, "лекарства", Some(100), 4),
            category(5, "транспорт", None, 5),
            category(6, "красота", Some(200), 6),
            category(7, "косметика", Some(200), 7),
        ];
        let groups = vec![
            CategoryGroup {
                id: 200,
                name: "Красота/косметика".into(),
                sort_order: 1,
            },
            CategoryGroup {
                id: 100,
                name: "Лечение/лекарства".into(),
                sort_order: 2,
            },
        ];
        (categories, groups)
    }

    #[test]
    fn sums_by_category_and_by_group() {
        let (categories, groups) = dictionary();
        let mut with_cashback = entry(9, Some(1), 1000, false);
        with_cashback
            .apply(EntryEdit::Cashback(Some(Kopecks::from_rubles(50))))
            .unwrap();
        with_cashback.refresh_cashback(Rounding::FloorRub);
        let entries = [
            entry(1, Some(1), 100, false),
            entry(2, Some(3), 200, true),
            entry(3, Some(4), 300, true),
            entry(4, Some(7), 40, false),
            entry(5, None, 3, false),
            with_cashback,
        ];
        let t = month_totals(&entries, &categories, &groups, &[]);
        assert_eq!(t.total, Kopecks::from_rubles(1643));
        assert_eq!(t.uncategorized, Kopecks::from_rubles(3));
        assert_eq!(t.regular, Kopecks::from_rubles(500));
        assert_eq!(t.cashback, Kopecks::from_rubles(50));
        // 50 / 1643 = 3,043% → 304 bp.
        assert_eq!(t.cashback_rate_bp, Some(304));

        let cats: Vec<_> = t
            .by_category
            .iter()
            .map(|c| (c.label.as_str(), c.amount.get() / 100))
            .collect();
        assert_eq!(
            cats,
            [
                ("еда", 1100),
                ("продукты", 0),
                ("лечение", 200),
                ("лекарства", 300),
                ("транспорт", 0),
                ("красота", 0),
                ("косметика", 40)
            ]
        );

        let grouped: Vec<_> = t
            .by_group
            .iter()
            .map(|c| (c.label.as_str(), c.is_group, c.amount.get() / 100))
            .collect();
        assert_eq!(
            grouped,
            [
                ("еда", false, 1100),
                ("продукты", false, 0),
                ("Лечение/лекарства", true, 500),
                ("транспорт", false, 0),
                ("Красота/косметика", true, 40)
            ]
        );
        assert_eq!(t.by_group[2].category_ids, [3, 4]);
    }

    #[test]
    fn inactive_categories_show_only_with_entries() {
        let (mut categories, groups) = dictionary();
        categories[1].is_active = false; // продукты
        categories[4].is_active = false; // транспорт
        let entries = [entry(1, Some(5), 10, false)];
        let t = month_totals(&entries, &categories, &groups, &[]);
        let labels: Vec<_> = t.by_category.iter().map(|c| c.label.as_str()).collect();
        assert!(!labels.contains(&"продукты"));
        assert!(labels.contains(&"транспорт"));
    }

    #[test]
    fn rate_base_leaves_out_rent_only() {
        // In xlsx terms «Ср. %» = M7 / (H2 − G3), row 3 being rent (D16, D37). The internet in
        // the same category stays in the base.
        let (mut categories, groups) = dictionary();
        categories.push(category(8, "квартиры", None, 8));
        let rent = [RegularTemplate {
            id: 1,
            title: "Квартплата".into(),
            category_id: Some(8),
            participant_id: None,
            product_id: None,
            default_amount: None,
            sort_order: 1,
            is_active: true,
            exclude_from_rate: true,
        }];
        let mut groceries = entry(2, Some(2), 2000, false);
        groceries
            .apply(EntryEdit::Cashback(Some(Kopecks::from_rubles(100))))
            .unwrap();
        let mut flat = entry(1, Some(8), 8000, true);
        flat.title = " квартплата ".into();
        let mut internet = entry(3, Some(8), 1000, true);
        internet.title = "Интернет".into();
        let entries = [flat, groceries, internet];
        let t = month_totals(&entries, &categories, &groups, &rent);
        assert_eq!(t.total, Kopecks::from_rubles(11_000));
        // 100 / 3 000 = 3,33%: rent is out, the internet is in.
        assert_eq!(t.cashback_rate_bp, Some(333));
        // Without the mark rent counts too: 100 / 11 000.
        let t = month_totals(&entries, &categories, &groups, &[]);
        assert_eq!(t.cashback_rate_bp, Some(91));

        // Only rent: no base, no rate.
        let t = month_totals(&entries[..1], &categories, &groups, &rent);
        assert_eq!(t.cashback_rate_bp, None);
    }

    #[test]
    fn empty_month() {
        let (categories, groups) = dictionary();
        let t = month_totals(&[], &categories, &groups, &[]);
        assert_eq!(t.total, Kopecks::ZERO);
        assert_eq!(t.by_category.len(), 7);
        assert_eq!(t.by_group.len(), 5);
        assert_eq!(t.cashback_rate_bp, None);
    }
}
