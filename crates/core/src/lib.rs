//! Reestr core: domain model and business logic.
//!
//! Rules (PLAN §4.2, §15):
//! - no SQLite and no Tauri here: the crate stays pure logic, independent of the app shell;
//! - money is integer kopecks ([`money::Kopecks`]), never floats;
//! - every calculation is a pure function with tests.

pub mod cashback;
pub mod date;
pub mod dict;
pub mod export;
pub mod import;
pub mod journal;
pub mod money;
pub mod month;
pub mod products;
pub mod rules;
pub mod summary;
pub mod totals;

pub use cashback::{Cashback, CashbackSource, RateBp, Rounding};
pub use dict::{
    Bank, BankId, Category, CategoryGroup, CategoryId, DictInput, DictKind, DictOp, GroupId,
    Participant, ParticipantId, Product, ProductId, ProductKind, RegularTemplate, TemplateId,
    name_key,
};
pub use journal::{EditError, EditInput, Entry, EntryEdit, EntryId, JournalOp};
pub use money::Kopecks;
pub use month::Month;
pub use products::{
    AvailableCheck, CreditInput, CreditTerms, CustomDeadline, Deadline, DeadlineId, DeadlineInput,
    DeadlineSource, GraceId, GracePeriod, ProductError, ProductOp, Repayment, RepaymentId,
    RepaymentInput, SavingsInput, SavingsState, SavingsTerms, TrancheInput, TrancheState,
    TrancheStatus, Urgency,
};
pub use rules::{
    Allocation, CardSuggestion, CashbackRule, RuleError, RuleId, RuleInput, RuleOp, RuleUsage,
    allocate, rule_for, rule_of_entry, suggest_cards,
};
pub use summary::{
    Inflow, InflowId, InflowInput, InflowOp, MonthBalance, Plan, SummaryColumn, SummaryFooter,
    SummaryRow, YearData, YearSummary, month_balance, plan_for, year_summary,
};
pub use totals::{MonthTotals, TotalColumn, month_totals};
