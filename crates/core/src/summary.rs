//! «Свод» (PLAN §8) and the month's «Приход / расход / дельта»: spending against the income
//! plan («Оклад»), by months of a year, with the year's sums, shares, averages and the growth
//! against the year before — the «Свод» sheet of the source workbook, computed here rather than by formulas.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cashback::effective_rate_bp;
use crate::money::Kopecks;
use crate::month::Month;
use crate::totals::MonthTotals;

/// The income plan of a month: the latest one set on or before it (a plan carries over to the
/// following months until changed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub amount: Kopecks,
    /// The month the plan was set in.
    pub set_in: Month,
}

pub fn plan_for(plans: &[(Month, Kopecks)], month: Month) -> Option<Plan> {
    plans
        .iter()
        .filter(|(m, _)| *m <= month)
        .max_by_key(|(m, _)| *m)
        .map(|(m, amount)| Plan {
            amount: *amount,
            set_in: *m,
        })
}

/// «Приход / расход / дельта» of a month.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthBalance {
    pub plan: Option<Plan>,
    /// Money that came in (the month's inflows).
    pub inflow: Kopecks,
    pub spent: Kopecks,
    pub cashback: Kopecks,
    /// «Баланс в мес.»: plan − spent.
    pub planned: Option<Kopecks>,
    /// Inflow − spent, when anything came in.
    pub actual: Option<Kopecks>,
    /// «Общий баланс»: plan + cashback − spent.
    pub overall: Option<Kopecks>,
}

pub fn month_balance(totals: &MonthTotals, plan: Option<Plan>, inflow: Kopecks) -> MonthBalance {
    let spent = totals.total;
    let cashback = totals.cashback;
    MonthBalance {
        plan,
        inflow,
        spent,
        cashback,
        planned: plan.map(|p| p.amount - spent),
        actual: (!inflow.is_zero()).then_some(inflow - spent),
        overall: plan.map(|p| p.amount + cashback - spent),
    }
}

/// One column of «Свод»: a category, or a group when grouped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryColumn {
    pub id: i64,
    pub is_group: bool,
    pub label: String,
}

/// A month row of «Свод».
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryRow {
    pub month: Month,
    /// The month has entries; empty months show only the plan.
    pub has_data: bool,
    /// «Сумма».
    pub total: Kopecks,
    /// «Оклад».
    pub plan: Option<Kopecks>,
    /// «Баланс в мес.»: plan − total.
    pub balance: Option<Kopecks>,
    /// «Прирост относительно прошлого года»: against the same month a year before.
    pub growth_bp: Option<i64>,
    /// In the order of [`YearSummary::columns`].
    pub columns: Vec<Kopecks>,
    pub regular: Kopecks,
    pub cashback: Kopecks,
    /// «Общий баланс»: plan + cashback − total.
    pub overall_balance: Option<Kopecks>,
    /// «Общий % кешбека»: cashback / total (D26).
    pub cashback_rate_bp: Option<i64>,
    pub inflow: Kopecks,
}

/// A figure of the footer per column: the categories (or groups), then «Регулярные», then
/// «Кешбек».
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FooterLine<T> {
    pub columns: Vec<T>,
    pub regular: T,
    pub cashback: T,
}

/// The rows under the months of «Свод».
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryFooter {
    /// Months with entries: what the averages divide by.
    pub months: usize,
    /// The year's spending.
    pub total: Kopecks,
    /// «Сумма».
    pub sums: FooterLine<Kopecks>,
    /// «От общ. %»: each sum against the year's spending.
    pub shares_bp: FooterLine<Option<i64>>,
    /// «Среднее» per month with entries.
    pub averages: FooterLine<Option<Kopecks>>,
    /// The average month's spending and its growth against the year before.
    pub average_total: Option<Kopecks>,
    pub average_total_growth_bp: Option<i64>,
    /// «Ср. прирост»: this year's average − last year's, in rubles and in percent.
    pub average_growth: FooterLine<Option<Kopecks>>,
    pub average_growth_bp: FooterLine<Option<i64>>,
    /// The sum of «Общий баланс» of the months that have a plan.
    pub overall_balance: Option<Kopecks>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YearSummary {
    pub year: i32,
    pub grouped: bool,
    pub columns: Vec<SummaryColumn>,
    /// January to December.
    pub months: Vec<SummaryRow>,
    pub footer: SummaryFooter,
}

/// The data of one year for [`year_summary`].
pub struct YearData<'a> {
    /// Totals of every entry of the year: they give the columns, in dictionary order, and the
    /// year's sums.
    pub year: &'a MonthTotals,
    /// Totals of the months that have entries.
    pub months: &'a [(Month, MonthTotals)],
}

/// `a / b` in basis points, half away from zero; `None` when `b` is zero.
fn ratio_bp(a: Kopecks, b: Kopecks) -> Option<i64> {
    effective_rate_bp(b, a)
}

/// `a / b − 1` in basis points: growth from `b` to `a`.
fn growth_bp(a: Kopecks, b: Kopecks) -> Option<i64> {
    ratio_bp(a - b, b)
}

/// `sum / n` in kopecks, half away from zero.
fn average(sum: Kopecks, n: usize) -> Option<Kopecks> {
    let n = i64::try_from(n).ok().filter(|n| *n > 0)?;
    let q = sum.get() / n;
    let r = sum.get() % n;
    let rounded = if r.abs() * 2 >= n {
        q + sum.get().signum()
    } else {
        q
    };
    Some(Kopecks(rounded))
}

fn column_key(c: &crate::totals::TotalColumn) -> (bool, i64) {
    (c.is_group, c.id)
}

fn columns_of(totals: &MonthTotals, grouped: bool) -> &[crate::totals::TotalColumn] {
    if grouped {
        &totals.by_group
    } else {
        &totals.by_category
    }
}

/// The figures of a year's footer, without the comparison with the year before.
struct Averages {
    months: usize,
    total: Option<Kopecks>,
    columns: HashMap<(bool, i64), Kopecks>,
    regular: Option<Kopecks>,
    cashback: Option<Kopecks>,
}

fn averages(data: &YearData<'_>, grouped: bool) -> Averages {
    let n = data.months.len();
    Averages {
        months: n,
        total: average(data.year.total, n),
        columns: columns_of(data.year, grouped)
            .iter()
            .filter_map(|c| average(c.amount, n).map(|a| (column_key(c), a)))
            .collect(),
        regular: average(data.year.regular, n),
        cashback: average(data.year.cashback, n),
    }
}

/// «Свод» of `year`: twelve month rows and the footer. `previous` is the year before, for the
/// growth columns; `plan` and `inflow` give each month's income plan and inflows.
pub fn year_summary(
    year: i32,
    data: &YearData<'_>,
    previous: Option<&YearData<'_>>,
    plan: impl Fn(Month) -> Option<Kopecks>,
    inflow: impl Fn(Month) -> Kopecks,
    grouped: bool,
) -> YearSummary {
    let columns: Vec<SummaryColumn> = columns_of(data.year, grouped)
        .iter()
        .map(|c| SummaryColumn {
            id: c.id,
            is_group: c.is_group,
            label: c.label.clone(),
        })
        .collect();
    let previous_month = |m: Month| {
        previous.and_then(|p| {
            p.months
                .iter()
                .find(|(pm, _)| pm.month() == m.month())
                .map(|(_, t)| t.total)
        })
    };

    let months = (1..=12)
        .filter_map(|m| Month::new(year, m).ok())
        .map(|month| {
            let plan = plan(month);
            let totals = data
                .months
                .iter()
                .find(|(m, _)| *m == month)
                .map(|(_, t)| t);
            let Some(totals) = totals else {
                return SummaryRow {
                    month,
                    has_data: false,
                    total: Kopecks::ZERO,
                    plan,
                    balance: None,
                    growth_bp: None,
                    columns: vec![Kopecks::ZERO; columns.len()],
                    regular: Kopecks::ZERO,
                    cashback: Kopecks::ZERO,
                    overall_balance: None,
                    cashback_rate_bp: None,
                    inflow: inflow(month),
                };
            };
            let own: HashMap<(bool, i64), Kopecks> = columns_of(totals, grouped)
                .iter()
                .map(|c| (column_key(c), c.amount))
                .collect();
            SummaryRow {
                month,
                has_data: true,
                total: totals.total,
                plan,
                balance: plan.map(|p| p - totals.total),
                growth_bp: previous_month(month).and_then(|prev| growth_bp(totals.total, prev)),
                columns: columns
                    .iter()
                    .map(|c| own.get(&(c.is_group, c.id)).copied().unwrap_or_default())
                    .collect(),
                regular: totals.regular,
                cashback: totals.cashback,
                overall_balance: plan.map(|p| p + totals.cashback - totals.total),
                cashback_rate_bp: effective_rate_bp(totals.total, totals.cashback),
                inflow: inflow(month),
            }
        })
        .collect::<Vec<_>>();

    let this = averages(data, grouped);
    let before = previous.map(|p| averages(p, grouped));
    let year_columns = columns_of(data.year, grouped);
    let total = data.year.total;
    let growth = |now: Option<Kopecks>, then: Option<Kopecks>| now.zip(then).map(|(a, b)| a - b);
    let growth_rel = |now: Option<Kopecks>, then: Option<Kopecks>| {
        now.zip(then).and_then(|(a, b)| growth_bp(a, b))
    };
    let before_column =
        |key: (bool, i64)| before.as_ref().and_then(|b| b.columns.get(&key).copied());
    let before_regular = before.as_ref().and_then(|b| b.regular);
    let before_cashback = before.as_ref().and_then(|b| b.cashback);
    let overall: Vec<Kopecks> = months.iter().filter_map(|r| r.overall_balance).collect();

    let footer = SummaryFooter {
        months: this.months,
        total,
        sums: FooterLine {
            columns: year_columns.iter().map(|c| c.amount).collect(),
            regular: data.year.regular,
            cashback: data.year.cashback,
        },
        shares_bp: FooterLine {
            columns: year_columns
                .iter()
                .map(|c| ratio_bp(c.amount, total))
                .collect(),
            regular: ratio_bp(data.year.regular, total),
            cashback: ratio_bp(data.year.cashback, total),
        },
        averages: FooterLine {
            columns: year_columns
                .iter()
                .map(|c| this.columns.get(&column_key(c)).copied())
                .collect(),
            regular: this.regular,
            cashback: this.cashback,
        },
        average_total: this.total,
        average_total_growth_bp: growth_rel(this.total, before.as_ref().and_then(|b| b.total)),
        average_growth: FooterLine {
            columns: year_columns
                .iter()
                .map(|c| {
                    growth(
                        this.columns.get(&column_key(c)).copied(),
                        before_column(column_key(c)),
                    )
                })
                .collect(),
            regular: growth(this.regular, before_regular),
            cashback: growth(this.cashback, before_cashback),
        },
        average_growth_bp: FooterLine {
            columns: year_columns
                .iter()
                .map(|c| {
                    growth_rel(
                        this.columns.get(&column_key(c)).copied(),
                        before_column(column_key(c)),
                    )
                })
                .collect(),
            regular: growth_rel(this.regular, before_regular),
            cashback: growth_rel(this.cashback, before_cashback),
        },
        overall_balance: (!overall.is_empty()).then(|| overall.iter().sum()),
    };

    YearSummary {
        year,
        grouped,
        columns,
        months,
        footer,
    }
}

pub type InflowId = i64;

/// Money that came in: a salary, a transfer (PLAN §5 `inflow`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inflow {
    pub id: InflowId,
    pub month: Month,
    #[serde(with = "crate::date::iso::option")]
    pub date: Option<time::Date>,
    pub participant_id: Option<crate::dict::ParticipantId>,
    pub product_id: Option<crate::dict::ProductId>,
    /// «Зарплата», «Аванс», «Перевод от …».
    pub source: String,
    pub amount: Kopecks,
    pub notes: Option<String>,
}

/// A change of one inflow field; the participant and the account are names (D17).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum InflowInput {
    Date(#[serde(with = "crate::date::iso::option")] Option<time::Date>),
    Source(String),
    Participant(Option<String>),
    Card(Option<String>),
    Amount(Kopecks),
    Notes(Option<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum InflowOp {
    Edit { id: InflowId, edit: InflowInput },
    Append { edits: Vec<InflowInput> },
    Delete { ids: Vec<InflowId> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::{Category, CategoryGroup};
    use crate::journal::{Entry, EntryEdit};
    use crate::totals::month_totals;

    fn m(s: &str) -> Month {
        s.parse().unwrap()
    }

    fn rub(r: i64) -> Kopecks {
        Kopecks::from_rubles(r)
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

    fn dictionary() -> (Vec<Category>, Vec<CategoryGroup>) {
        (
            vec![
                category(1, "еда", None, 1),
                category(2, "лечение", Some(10), 2),
                category(3, "лекарства", Some(10), 3),
            ],
            vec![CategoryGroup {
                id: 10,
                name: "Лечение/лекарства".into(),
                sort_order: 1,
            }],
        )
    }

    fn entry(
        id: i64,
        month: &str,
        category: i64,
        rubles: i64,
        cashback: i64,
        regular: bool,
    ) -> Entry {
        let mut e = Entry::new(id, m(month));
        e.category_id = Some(category);
        e.amount = rub(rubles);
        e.is_regular = regular;
        if cashback != 0 {
            e.apply(EntryEdit::Cashback(Some(rub(cashback)))).unwrap();
        }
        e
    }

    /// Totals of a year: the whole year and each month with entries.
    fn year(entries: &[Entry]) -> (MonthTotals, Vec<(Month, MonthTotals)>) {
        let (categories, groups) = dictionary();
        let mut months: Vec<Month> = entries.iter().map(|e| e.month).collect();
        months.sort();
        months.dedup();
        let per_month = months
            .into_iter()
            .map(|month| {
                let of_month: Vec<&Entry> = entries.iter().filter(|e| e.month == month).collect();
                (
                    month,
                    month_totals(of_month.iter().copied(), &categories, &groups, &[]),
                )
            })
            .collect();
        (month_totals(entries, &categories, &groups, &[]), per_month)
    }

    #[test]
    fn plan_carries_over() {
        let plans = [(m("2026-01"), rub(60_000)), (m("2026-07"), rub(45_000))];
        assert_eq!(plan_for(&plans, m("2025-12")), None);
        assert_eq!(
            plan_for(&plans, m("2026-03")),
            Some(Plan {
                amount: rub(60_000),
                set_in: m("2026-01")
            })
        );
        assert_eq!(plan_for(&plans, m("2026-07")).unwrap().amount, rub(45_000));
        assert_eq!(plan_for(&plans, m("2027-02")).unwrap().set_in, m("2026-07"));
    }

    #[test]
    fn month_balance_against_plan_and_inflows() {
        let (_, months) = year(&[entry(1, "2026-07", 1, 30_000, 500, false)]);
        let totals = &months[0].1;
        let plan = Some(Plan {
            amount: rub(60_000),
            set_in: m("2026-01"),
        });
        let b = month_balance(totals, plan, rub(52_000));
        assert_eq!(b.planned, Some(rub(30_000)));
        assert_eq!(b.actual, Some(rub(22_000)));
        assert_eq!(b.overall, Some(rub(30_500)));
        let none = month_balance(totals, None, Kopecks::ZERO);
        assert_eq!(
            (none.planned, none.actual, none.overall),
            (None, None, None)
        );
    }

    #[test]
    fn a_year_like_his_sheet() {
        let this_year = [
            entry(1, "2026-01", 1, 1000, 30, false),
            entry(2, "2026-01", 2, 500, 0, true),
            entry(3, "2026-02", 3, 300, 0, false),
            entry(4, "2026-02", 1, 1201, 0, false),
            entry(5, "2026-04", 1, 999, 10, false),
        ];
        let last_year = [
            entry(6, "2025-01", 1, 1200, 0, false),
            entry(7, "2025-02", 1, 1000, 0, false),
            entry(8, "2025-02", 2, 400, 0, false),
        ];
        let (y, months) = year(&this_year);
        let (py, prev_months) = year(&last_year);
        let data = YearData {
            year: &y,
            months: &months,
        };
        let prev = YearData {
            year: &py,
            months: &prev_months,
        };
        let plans = [(m("2026-01"), rub(4000))];
        let s = year_summary(
            2026,
            &data,
            Some(&prev),
            |month| plan_for(&plans, month).map(|p| p.amount),
            |month| {
                if month == m("2026-02") {
                    rub(5000)
                } else {
                    Kopecks::ZERO
                }
            },
            false,
        );
        let labels: Vec<_> = s.columns.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["еда", "лечение", "лекарства"]);
        assert_eq!(s.months.len(), 12);

        let jan = &s.months[0];
        assert_eq!(jan.total, rub(1500));
        assert_eq!(jan.plan, Some(rub(4000)));
        assert_eq!(jan.balance, Some(rub(2500)));
        // 1 500 / 1 200 − 1 = 25%.
        assert_eq!(jan.growth_bp, Some(2500));
        assert_eq!(jan.columns, [rub(1000), rub(500), Kopecks::ZERO]);
        assert_eq!(jan.regular, rub(500));
        assert_eq!(jan.overall_balance, Some(rub(2530)));
        // 30 / 1 500 = 2%: the plain ratio (D26).
        assert_eq!(jan.cashback_rate_bp, Some(200));

        let feb = &s.months[1];
        // 1 501 / 1 400 − 1 = 7,21%.
        assert_eq!(feb.growth_bp, Some(721));
        assert_eq!(feb.inflow, rub(5000));
        // March is empty; it shows the plan only.
        let mar = &s.months[2];
        assert!(!mar.has_data);
        assert_eq!((mar.plan, mar.balance), (Some(rub(4000)), None));
        // April had no April a year before.
        assert_eq!(s.months[3].growth_bp, None);

        let f = &s.footer;
        assert_eq!(f.months, 3);
        assert_eq!(f.total, rub(4000));
        assert_eq!(f.sums.columns, [rub(3200), rub(500), rub(300)]);
        // 3 200 / 4 000 = 80%.
        assert_eq!(f.shares_bp.columns[0], Some(8000));
        assert_eq!(f.shares_bp.cashback, Some(100));
        // 3 200 / 3 months = 1 066,666… → 1 066,67.
        assert_eq!(f.averages.columns[0], Some(Kopecks(106_667)));
        assert_eq!(f.average_total, Some(Kopecks(133_333)));
        // Last year: 2 600 over 2 months = 1 300 a month → 1 333,33 / 1 300 − 1 = 2,56%.
        assert_eq!(f.average_total_growth_bp, Some(256));
        // еда last year: 2 200 / 2 = 1 100 → +(1 066,67 − 1 100) = −33,33.
        assert_eq!(f.average_growth.columns[0], Some(Kopecks(-3_333)));
        assert_eq!(f.average_growth_bp.columns[0], Some(-303));
        // лекарства had nothing last year: no growth.
        assert_eq!(f.average_growth_bp.columns[2], None);
        // Overall balance of the months with a plan and entries: 2 530 + 2 499 + 3 011.
        assert_eq!(f.overall_balance, Some(rub(8040)));

        // By groups: лечение and лекарства are one column.
        let g = year_summary(2026, &data, None, |_| None, |_| Kopecks::ZERO, true);
        let labels: Vec<_> = g.columns.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["еда", "Лечение/лекарства"]);
        assert_eq!(g.months[1].columns, [rub(1201), rub(300)]);
        assert_eq!(g.footer.average_total_growth_bp, None);
        assert_eq!(g.footer.overall_balance, None);
    }

    #[test]
    fn averages_round_half_away_from_zero() {
        assert_eq!(average(Kopecks(5), 2), Some(Kopecks(3)));
        assert_eq!(average(Kopecks(-5), 2), Some(Kopecks(-3)));
        assert_eq!(average(Kopecks(4), 3), Some(Kopecks(1)));
        assert_eq!(average(Kopecks(5), 0), None);
        assert_eq!(growth_bp(rub(90), rub(100)), Some(-1000));
        assert_eq!(growth_bp(rub(1), Kopecks::ZERO), None);
    }
}
