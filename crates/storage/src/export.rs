//! Gathering a file for the xlsx and json export (PLAN §9, D29–D31).

use reestr_core::export::{Book, BookMonth, json, xlsx};
use reestr_core::{Month, month_totals};
use time::Date;

use crate::dict::load;
use crate::error::StorageError;
use crate::income::{all_inflows, plans};
use crate::journal::entries_of;
use crate::migrations::SCHEMA_VERSION;
use crate::vault::Vault;

impl Vault {
    /// Everything the export writes. `today` dates the state of the tranches.
    pub fn export_book(&self, today: Date) -> Result<Book, StorageError> {
        let d = load(&self.conn)?;
        let mut months = Vec::new();
        for month in self.months()? {
            let entries = entries_of(&self.conn, month)?;
            months.push(BookMonth {
                month,
                totals: month_totals(&entries, &d.categories, &d.groups, &d.templates),
                balance: self.month_balance(month)?,
                entries,
            });
        }

        let rule_months: Vec<Month> = {
            let mut stmt = self
                .conn
                .prepare("SELECT DISTINCT month FROM cashback_rule ORDER BY month")?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows.iter().filter_map(|m| m.parse().ok()).collect()
        };
        let mut rules = Vec::new();
        for month in rule_months {
            let month_rules = self.month_rules(month)?;
            for rule in month_rules.rules {
                let usage = month_rules
                    .usage
                    .iter()
                    .find(|u| u.rule_id == rule.id)
                    .copied();
                rules.push((rule, usage));
            }
        }

        let products = self.products(today)?;
        let mut years = Vec::new();
        for year in self.summary_years()? {
            years.push(self.year_summary(year, true)?);
        }
        Ok(Book {
            schema_version: SCHEMA_VERSION,
            participants: d.participants,
            banks: d.banks,
            products: d.products,
            groups: d.groups,
            categories: d.categories,
            templates: d.templates,
            months,
            rules,
            inflows: all_inflows(&self.conn)?,
            plans: plans(&self.conn)?,
            credit_terms: products
                .credit
                .iter()
                .map(|c| (c.product_id, c.terms.clone()))
                .collect(),
            tranches: products
                .credit
                .iter()
                .flat_map(|c| c.tranches.iter().cloned().zip(c.states.iter().copied()))
                .collect(),
            repayments: products
                .credit
                .iter()
                .flat_map(|c| c.repayments.iter().cloned())
                .collect(),
            savings_terms: products
                .savings
                .iter()
                .map(|s| (s.product_id, s.terms.clone()))
                .collect(),
            reminders: products.reminders,
            years,
            card_suggestions: self.card_suggestions_enabled()?,
        })
    }

    /// The xlsx export (D29, D30). It is not encrypted: the caller warns before saving it.
    pub fn export_xlsx(&self, today: Date) -> Result<Vec<u8>, StorageError> {
        Ok(xlsx(&self.export_book(today)?)?)
    }

    /// The json export (D31). It is not encrypted either.
    pub fn export_json(&self, today: Date) -> Result<String, StorageError> {
        Ok(json(&self.export_book(today)?)?)
    }
}
