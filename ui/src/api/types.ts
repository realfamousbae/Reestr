// Shapes exchanged with the Rust side (crates/app/src/dto.rs, crates/core). Money is integer
// kopecks, rates are integer basis points, dates are ISO strings, months are "YYYY-MM".

export type Kopecks = number;
export type Month = string;

export interface FileInfo {
  path: string;
  fileName: string;
  schemaVersion: number;
  createdAt: string;
  createdWith: string;
  cipherVersion: string;
  /** The copy of the day could not be made when the file was opened (D32). */
  copyFailed: boolean;
}

/** A copy of the file in its copies folder (D32). */
export interface CopyView {
  fileName: string;
  date: string;
  size: number;
}

/** The «О файле и пароле» page. */
export interface AboutView {
  file: FileInfo;
  size: number;
  cipher: {
    version: string;
    kdfIterations: string;
    kdfAlgorithm: string;
    hmacAlgorithm: string;
    pageSize: string;
    provider: string;
  };
  /** Null for a file opened from a copies folder: it makes no copies. */
  copiesDir: string | null;
  /** Oldest first. */
  copies: CopyView[];
  copiesKept: number;
}

// ---------- Dictionaries ----------

export type ProductKind = "debit" | "credit" | "savings" | "deposit" | "cash";
export type Rounding = "floor_rub" | "floor_kop" | "half_up_rub" | "half_up_kop";

export interface Participant {
  id: number;
  name: string;
  sortOrder: number;
  isActive: boolean;
}

export interface Bank {
  id: number;
  name: string;
  shortName: string | null;
}

export interface Product {
  id: number;
  bankId: number | null;
  participantId: number | null;
  kind: ProductKind | null;
  shortName: string;
  fullName: string | null;
  cashbackRounding: Rounding;
  isActive: boolean;
  openedAt: string | null;
  closedAt: string | null;
  notes: string | null;
}

export interface CategoryGroup {
  id: number;
  name: string;
  sortOrder: number;
}

export interface Category {
  id: number;
  name: string;
  groupId: number | null;
  isRegularDefault: boolean;
  sortOrder: number;
  isActive: boolean;
}

export interface RegularTemplate {
  id: number;
  title: string;
  categoryId: number | null;
  participantId: number | null;
  productId: number | null;
  defaultAmount: Kopecks | null;
  sortOrder: number;
  isActive: boolean;
  /** «Не в Ср. %»: journal rows with this title stay out of the rate base (D19, D37). */
  excludeFromRate: boolean;
}

export interface Dictionaries {
  participants: Participant[];
  banks: Bank[];
  products: Product[];
  groups: CategoryGroup[];
  categories: Category[];
  templates: RegularTemplate[];
}

export type DictKind = "participant" | "bank" | "product" | "group" | "category" | "template";

/** One dictionary field change; references are names (missing ones are created, D17). */
export type DictInput =
  | { field: "name"; value: string }
  | { field: "shortName"; value: string | null }
  | { field: "fullName"; value: string | null }
  | { field: "notes"; value: string | null }
  | { field: "isActive"; value: boolean }
  | { field: "isRegularDefault"; value: boolean }
  | { field: "excludeFromRate"; value: boolean }
  | { field: "sortOrder"; value: number }
  | { field: "group"; value: string | null }
  | { field: "bank"; value: string | null }
  | { field: "participant"; value: string | null }
  | { field: "category"; value: string | null }
  | { field: "card"; value: string | null }
  | { field: "kind"; value: ProductKind | null }
  | { field: "rounding"; value: Rounding }
  | { field: "defaultAmount"; value: Kopecks | null }
  | { field: "openedAt"; value: string | null }
  | { field: "closedAt"; value: string | null };

export type DictOp =
  | { op: "edit"; id: number; edit: DictInput }
  | { op: "append"; edits: DictInput[] }
  | { op: "delete"; ids: number[] };

/** A dictionary item created on the fly from a typed name. */
export interface Created {
  kind: DictKind;
  name: string;
}

// ---------- Journal ----------

export interface MonthTab {
  month: Month;
  /** Sheet name, "MM.YY". */
  title: string;
}

export type CashbackSource = "rule" | "manual_percent" | "manual_amount";

export interface EntryView {
  id: number;
  title: string;
  category: string | null;
  isRegular: boolean;
  participant: string | null;
  date: string | null;
  amount: Kopecks;
  rateBp: number | null;
  cashback: Kopecks | null;
  cashbackSource: CashbackSource | null;
  card: string | null;
  mcc: string | null;
}

export interface TotalColumn {
  /** Category id, or group id when `isGroup`. */
  id: number;
  isGroup: boolean;
  label: string;
  amount: Kopecks;
  categoryIds: number[];
}

export interface MonthTotals {
  total: Kopecks;
  byCategory: TotalColumn[];
  byGroup: TotalColumn[];
  uncategorized: Kopecks;
  regular: Kopecks;
  cashback: Kopecks;
  cashbackRateBp: number | null;
}

/** A cashback rule of the month with what is used of it. */
export interface RuleView {
  id: number;
  card: string;
  /** null, and no MCC either, is «на всё». */
  category: string | null;
  mcc: string | null;
  rateBp: number;
  spendCap: Kopecks | null;
  cashbackCap: Kopecks | null;
  adjustment: Kopecks;
  notes: string | null;
  spent: Kopecks;
  cashback: Kopecks;
  entries: number;
  remainingSpend: Kopecks | null;
  remainingCashback: Kopecks | null;
  exhausted: boolean;
}

/** The income plan of a month: its own or carried over from `setIn`. */
export interface Plan {
  amount: Kopecks;
  setIn: Month;
}

/** «Приход / расход / дельта» of a month, computed by the core. */
export interface MonthBalance {
  plan: Plan | null;
  inflow: Kopecks;
  spent: Kopecks;
  cashback: Kopecks;
  /** Plan − spent. */
  planned: Kopecks | null;
  /** Inflow − spent, when anything came in. */
  actual: Kopecks | null;
  /** Plan + cashback − spent. */
  overall: Kopecks | null;
}

export interface InflowView {
  id: number;
  date: string | null;
  source: string;
  participant: string | null;
  card: string | null;
  amount: Kopecks;
  notes: string | null;
}

/** What the export writes: the source workbook layout, or all the data as json. */
export type ExportFormat = "xlsx" | "json";

export interface MonthView {
  month: Month;
  title: string;
  entries: EntryView[];
  totals: MonthTotals;
  rules: RuleView[];
  /** The month the «copy rules» button copies from. */
  copyRulesFrom: MonthTab | null;
  /** Whether the card cell suggests cards (D6). */
  cardSuggestions: boolean;
  balance: MonthBalance;
  inflows: InflowView[];
}

export type InflowInput =
  | { field: "date"; value: string | null }
  | { field: "source"; value: string }
  | { field: "participant"; value: string | null }
  | { field: "card"; value: string | null }
  | { field: "amount"; value: Kopecks }
  | { field: "notes"; value: string | null };

export type InflowOp =
  | { op: "edit"; id: number; edit: InflowInput }
  | { op: "append"; edits: InflowInput[] }
  | { op: "delete"; ids: number[] };

// ---------- «Свод» ----------

export interface SummaryColumn {
  id: number;
  isGroup: boolean;
  label: string;
}

export interface SummaryRow {
  month: Month;
  hasData: boolean;
  total: Kopecks;
  plan: Kopecks | null;
  balance: Kopecks | null;
  growthBp: number | null;
  /** In the order of `YearSummary.columns`. */
  columns: Kopecks[];
  regular: Kopecks;
  cashback: Kopecks;
  overallBalance: Kopecks | null;
  cashbackRateBp: number | null;
  inflow: Kopecks;
}

export interface FooterLine<T> {
  columns: T[];
  regular: T;
  cashback: T;
}

export interface SummaryFooter {
  months: number;
  total: Kopecks;
  sums: FooterLine<Kopecks>;
  sharesBp: FooterLine<number | null>;
  averages: FooterLine<Kopecks | null>;
  averageTotal: Kopecks | null;
  averageTotalGrowthBp: number | null;
  averageGrowth: FooterLine<Kopecks | null>;
  averageGrowthBp: FooterLine<number | null>;
  overallBalance: Kopecks | null;
}

export interface YearSummary {
  year: number;
  grouped: boolean;
  columns: SummaryColumn[];
  months: SummaryRow[];
  footer: SummaryFooter;
}

export type EntryEdit =
  | { field: "title"; value: string }
  | { field: "category"; value: string | null }
  | { field: "isRegular"; value: boolean }
  | { field: "participant"; value: string | null }
  | { field: "date"; value: string | null }
  | { field: "amount"; value: Kopecks }
  | { field: "rate"; value: number | null }
  | { field: "cashback"; value: Kopecks | null }
  | { field: "card"; value: string | null }
  | { field: "mcc"; value: string | null };

export type JournalOp =
  { op: "edit"; id: number; edit: EntryEdit } | { op: "append"; edits: EntryEdit[] };

/**
 * One rule field change; the card and the category are names. A rule has an MCC, a category or
 * neither, which is «на всё».
 */
export type RuleInput =
  | { field: "card"; value: string | null }
  | { field: "category"; value: string | null }
  | { field: "mcc"; value: string | null }
  | { field: "rate"; value: number }
  | { field: "spendCap"; value: Kopecks | null }
  | { field: "cashbackCap"; value: Kopecks | null }
  | { field: "adjustment"; value: Kopecks }
  | { field: "notes"; value: string | null };

export type RuleOp =
  | { op: "edit"; id: number; edit: RuleInput }
  | { op: "append"; edits: RuleInput[] }
  | { op: "delete"; ids: number[] }
  | { op: "copyFrom"; from: Month };

/** A card for the card cell, best first. */
export interface SuggestionView {
  card: string;
  rateBp: number;
  /** Cashback for the row's amount with what is left of the limits. */
  expected: Kopecks | null;
  exhausted: boolean;
  remainingSpend: Kopecks | null;
  remainingCashback: Kopecks | null;
}

export interface HistoryState {
  canUndo: boolean;
  canRedo: boolean;
}

export interface ChangeResult {
  view: MonthView;
  appended: number[];
  created: Created[];
  history: HistoryState;
}

export interface ReplayResult {
  /** The undone/redone action, or null when there was nothing to do. */
  label: string | null;
  /** The month the change was in. */
  view: MonthView;
  history: HistoryState;
}

export interface DictResult {
  dictionaries: Dictionaries;
  created: Created[];
  history: HistoryState;
}

// ---------- Products and deadlines ----------

export type TrancheStatus = "active" | "repaid" | "overdue";
export type Urgency = "overdue" | "soon" | "later";

export interface TrancheView {
  id: number;
  start: string;
  end: string;
  adjustment: Kopecks;
  notes: string | null;
  debt: Kopecks;
  repaid: Kopecks;
  remaining: Kopecks;
  purchases: number;
  /** Negative once the end has passed. */
  daysLeft: number;
  status: TrancheStatus;
}

export interface RepaymentView {
  id: number;
  trancheId: number;
  date: string;
  amount: Kopecks;
  notes: string | null;
}

export interface PurchaseView {
  entryId: number;
  month: Month;
  date: string | null;
  title: string;
  amount: Kopecks;
  trancheId: number | null;
  /** Bound by hand rather than by its date. */
  manual: boolean;
}

export interface AvailableCheck {
  on: string;
  entered: Kopecks;
  /** The debt on that day. */
  debt: Kopecks;
  /** Credit limit − debt, from the core. */
  computed: Kopecks;
  /** Entered − computed: above zero when the bank shows more. */
  difference: Kopecks;
}

export interface CreditCardView {
  productId: number;
  card: string;
  bank: string | null;
  participant: string | null;
  creditLimit: Kopecks | null;
  graceDays: number | null;
  /** The available balance the bank showed, with the day it was valid for; set together. */
  manualAvailable: Kopecks | null;
  manualAvailableOn: string | null;
  debt: Kopecks;
  /** The entered balance against the computed one; null when there is nothing to compare. */
  available: AvailableCheck | null;
  /** Purchases in no tranche: before the first one, or without a date. */
  unbound: number;
  /** Where a new repayment goes by default. */
  repaymentTranche: number | null;
  tranches: TrancheView[];
  repayments: RepaymentView[];
  purchases: PurchaseView[];
}

export type SavingsState =
  | { state: "open" }
  | { state: "waiting"; from: string; daysLeft: number }
  | { state: "canOpen"; since: string };

export interface SavingsView {
  productId: number;
  card: string;
  bank: string | null;
  participant: string | null;
  promoRateBp: number | null;
  closedAt: string | null;
  reopenAfterDays: number | null;
  state: SavingsState;
}

export type DeadlineSource =
  | { kind: "graceEnd"; gracePeriodId: number; remaining: Kopecks }
  | { kind: "savingsReopen" }
  | { kind: "custom"; id: number; title: string };

export interface DeadlineView {
  date: string;
  card: string | null;
  daysLeft: number;
  urgency: Urgency;
  source: DeadlineSource;
}

export interface ReminderView {
  id: number;
  date: string;
  title: string;
  card: string | null;
  isDone: boolean;
}

export interface ProductsView {
  credit: CreditCardView[];
  savings: SavingsView[];
  /** Debt on all credit cards. */
  debt: Kopecks;
  deadlines: DeadlineView[];
  reminders: ReminderView[];
}

export type CreditInput =
  | { field: "creditLimit"; value: Kopecks | null }
  | { field: "graceDays"; value: number | null }
  | { field: "manualAvailable"; value: { amount: Kopecks | null; on: string | null } };

export type SavingsInput =
  | { field: "promoRate"; value: number | null }
  | { field: "closedAt"; value: string | null }
  | { field: "reopenAfterDays"; value: number | null };

export type TrancheInput =
  | { field: "start"; value: string }
  | { field: "end"; value: string }
  | { field: "adjustment"; value: Kopecks }
  | { field: "notes"; value: string | null };

export type RepaymentInput =
  | { field: "date"; value: string }
  | { field: "amount"; value: Kopecks }
  | { field: "tranche"; value: number }
  | { field: "notes"; value: string | null };

export type DeadlineInput =
  | { field: "date"; value: string }
  | { field: "title"; value: string }
  | { field: "card"; value: string | null }
  | { field: "done"; value: boolean };

export type ProductOp =
  | { op: "creditTerms"; product: number; edit: CreditInput }
  | { op: "savingsTerms"; product: number; edit: SavingsInput }
  | { op: "addTranche"; product: number; edits: TrancheInput[] }
  | { op: "editTranche"; id: number; edit: TrancheInput }
  | { op: "deleteTranches"; ids: number[] }
  | { op: "addRepayment"; product: number; edits: RepaymentInput[] }
  | { op: "editRepayment"; id: number; edit: RepaymentInput }
  | { op: "deleteRepayments"; ids: number[] }
  | { op: "bind"; entry: number; tranche: number | null }
  | { op: "addDeadline"; edits: DeadlineInput[] }
  | { op: "editDeadline"; id: number; edit: DeadlineInput }
  | { op: "deleteDeadlines"; ids: number[] };

export interface ProductsResult {
  view: ProductsView;
  created: Created[];
  history: HistoryState;
}

// ---------- Import ----------

export interface Compared {
  label: string;
  expected: Kopecks;
  actual: Kopecks;
}

export interface MonthCheck {
  month: Month;
  sheet: string;
  rows: number;
  compared: Compared[];
  matches: boolean;
}

export interface PreviewMonth {
  month: Month;
  sheet: string;
  rows: number;
  /** Rows imported before; they are skipped. */
  already: number;
  check: MonthCheck;
}

export interface CardGuess {
  bank: string | null;
  kind: ProductKind | null;
  participant: string | null;
}

export interface PreviewCard {
  shortName: string;
  rows: number;
  exists: boolean;
  guess: CardGuess;
}

export interface PreviewName {
  name: string;
  isNew: boolean;
}

export interface ImportWarning {
  sheet: string;
  row: number;
  message: string;
}

export interface ImportPreview {
  months: PreviewMonth[];
  categories: PreviewName[];
  groups: PreviewName[];
  participants: PreviewName[];
  cards: PreviewCard[];
  warnings: ImportWarning[];
}

export interface CardSetup {
  shortName: string;
  bank: string | null;
  kind: ProductKind | null;
  participant: string | null;
}

export interface ImportReport {
  added: number;
  skipped: number;
  created: Created[];
  months: Month[];
}

export interface ImportResult {
  report: ImportReport;
  history: HistoryState;
}

/** What a failed command rejects with. */
export interface CommandError {
  code: string;
  message: string;
  detail: string | null;
}

/** Which option of «Калькулятор» brings more money. */
export type CalcBetter = "cashback" | "savings" | "equal";

/** The answer of «Калькулятор»; every number comes from the core. */
export interface CalcView {
  cashback: Kopecks;
  interest: Kopecks;
  /** Cashback minus interest: positive when the card wins. */
  difference: Kopecks;
  /** The yearly savings rate (basis points) at which interest equals cashback; null if undefined. */
  breakEvenBp: number | null;
  better: CalcBetter;
}

export interface Backend {
  vaultCreate(path: string, password: string): Promise<FileInfo>;
  /** Opens a file; the first opening of the day copies it (D32). `today` is "YYYY-MM-DD". */
  vaultOpen(path: string, password: string, today: string): Promise<FileInfo>;
  fileAbout(): Promise<AboutView>;
  changePassword(current: string, next: string): Promise<void>;
  vaultClose(): Promise<void>;
  dictionaries(): Promise<Dictionaries>;
  dictEdit(kind: DictKind, ops: DictOp[]): Promise<DictResult>;
  journalMonths(current: Month): Promise<MonthTab[]>;
  journalAddMonth(month: Month, current: Month): Promise<MonthTab[]>;
  journalMonth(month: Month): Promise<MonthView>;
  journalTitles(): Promise<string[]>;
  journalApply(month: Month, ops: JournalOp[]): Promise<ChangeResult>;
  journalDelete(month: Month, ids: number[]): Promise<ChangeResult>;
  journalAddTemplates(month: Month, ids: number[]): Promise<ChangeResult>;
  /** How many rows would get another cashback from these rule changes. Changes nothing. */
  rulesImpact(month: Month, ops: RuleOp[]): Promise<number>;
  rulesApply(month: Month, ops: RuleOp[], recompute: boolean): Promise<ChangeResult>;
  /** Cards for the card cell of a row; empty when the month has no rules. */
  cardSuggestions(month: Month, entry: number | null): Promise<SuggestionView[]>;
  setCardSuggestions(enabled: boolean): Promise<void>;
  inflowsApply(month: Month, ops: InflowOp[]): Promise<ChangeResult>;
  /** The income plan from `month` on; null returns to the plan of an earlier month. */
  planSet(month: Month, amount: Kopecks | null): Promise<ChangeResult>;
  /** Cashback against savings interest for a term. Stateless: reads and writes no file. */
  calculatorCompare(
    amount: Kopecks,
    cashbackRate: number,
    savingsRate: number,
    days: number,
  ): Promise<CalcView>;
  summaryYears(): Promise<number[]>;
  summaryYear(year: number, grouped: boolean): Promise<YearSummary>;
  /** `today` is the user's date, "YYYY-MM-DD". */
  productsView(today: string): Promise<ProductsView>;
  productsApply(ops: ProductOp[], today: string): Promise<ProductsResult>;
  historyUndo(month: Month): Promise<ReplayResult>;
  historyRedo(month: Month): Promise<ReplayResult>;
  importPreview(path: string): Promise<ImportPreview>;
  importApply(cards: CardSetup[]): Promise<ImportResult>;
  importCancel(): Promise<void>;
  /** Writes the export of the open file to `path`; `today` is "YYYY-MM-DD". */
  exportFile(path: string, format: ExportFormat, today: string): Promise<void>;
  /** Native "save as" dialog for an export; null when cancelled. */
  pickExportFile(format: ExportFormat, defaultName: string): Promise<string | null>;
  /** Native "open" dialog for an Excel workbook; null when cancelled. */
  pickWorkbook(): Promise<string | null>;
  /** Native "save as" dialog for a new file; null when cancelled. */
  pickNewFile(): Promise<string | null>;
  /** Native "open" dialog; null when cancelled. */
  pickExistingFile(): Promise<string | null>;
  /** A yes/no question; `labels` name the two buttons. */
  confirm(
    message: string,
    title: string,
    labels?: { ok: string; cancel: string },
  ): Promise<boolean>;
}
