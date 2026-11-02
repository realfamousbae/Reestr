// The live month sheet in the hero: a small, self-contained imitation of Reestr's journal.
// The real app computes money in its Rust core; this page only shows the idea with sample
// data. Amounts are integer kopecks, rates are basis points, as in the app.
"use strict";

(() => {
  const sheet = document.querySelector("[data-demo]");
  if (!sheet) return;

  // Cashback rules of the sample month: card, category (null: "на всё"), rate, cashback limit.
  const RULES = [
    { card: "Райф А", purpose: "продукты", rateBp: 500, limit: 100000 },
    { card: "Газпром КР", purpose: "транспорт", rateBp: 500, limit: 50000 },
    { card: "ПСБ 3170", purpose: "еда", rateBp: 300, limit: null },
    { card: "Почта А", purpose: "одежда", rateBp: 500, limit: 30000 },
    { card: "Совком А", purpose: null, rateBp: 100, limit: null },
  ];
  const CARDS = [...new Set(RULES.map((r) => r.card))];

  const rows = [
    {
      name: "Пятёрочка",
      purpose: "продукты",
      who: "Андрей",
      day: 3,
      amount: 715719,
      card: "Райф А",
    },
    {
      name: "Такси",
      purpose: "транспорт",
      who: "Марина",
      day: 4,
      amount: 249622,
      card: "Газпром КР",
    },
    { name: "Кафе", purpose: "еда", who: "Андрей", day: 9, amount: 120012, card: "ПСБ 3170" },
    { name: "Куртка", purpose: "одежда", who: "Марина", day: 12, amount: 600000, card: "Почта А" },
    { name: "Лента", purpose: "продукты", who: "Андрей", day: 15, amount: 534400, card: "Райф А" },
    { name: "Семена", purpose: "дача", who: "Марина", day: 18, amount: 328200, card: "Совком А" },
    {
      name: "Метро",
      purpose: "транспорт",
      who: "Марина",
      day: 20,
      amount: 17375,
      card: "Газпром КР",
    },
  ];

  // Autocomplete of «Наименование» from history fills in the purpose.
  const HISTORY = {
    Пятёрочка: "продукты",
    Лента: "продукты",
    Такси: "транспорт",
    Метро: "транспорт",
    Кафе: "еда",
    Куртка: "одежда",
    Семена: "дача",
  };

  const $ = (sel) => sheet.querySelector(sel);
  const body = $("[data-rows]");
  const nameInput = $('[data-field="name"]');
  const purposeInput = $('[data-field="purpose"]');
  const amountInput = $('[data-field="amount"]');
  const advice = $("[data-advice]");
  const scroller = $(".grid-scroll");
  const today = 25; // the sample month is 11.26, so new rows get a fixed day in it

  const NBSP = " ";
  function money(kop) {
    const rub = Math.floor(kop / 100);
    const k = kop % 100;
    const int = String(rub).replace(/\B(?=(\d{3})+(?!\d))/g, NBSP);
    return k ? `${int},${String(k).padStart(2, "0")}` : int;
  }
  function percent(bp) {
    return `${(bp / 100).toString().replace(".", ",")}%`;
  }
  function parseAmount(text) {
    const clean = text.replace(/[\s ₽]/g, "").replace(",", ".");
    if (!/^\d+(\.\d{1,2})?$/.test(clean)) return null;
    const [rub, kop = ""] = clean.split(".");
    return Number(rub) * 100 + Number(kop.padEnd(2, "0"));
  }

  function ruleFor(card, purpose) {
    return (
      RULES.find((r) => r.card === card && r.purpose === purpose) ||
      RULES.find((r) => r.card === card && r.purpose === null) ||
      null
    );
  }
  // Cashback is rounded down to whole rubles, as most banks do.
  function rawCashback(amount, rule) {
    return Math.floor((amount * rule.rateBp) / 10000 / 100) * 100;
  }

  // Limits are spent in the order of the journal rows.
  function compute(list) {
    const used = new Map(RULES.map((r) => [r, { spent: 0, cashback: 0 }]));
    const out = list.map((row) => {
      const rule = ruleFor(row.card, row.purpose);
      if (!rule) return { ...row, rule: null, cashback: 0 };
      const u = used.get(rule);
      let cb = rawCashback(row.amount, rule);
      if (rule.limit !== null) cb = Math.max(0, Math.min(cb, rule.limit - u.cashback));
      u.spent += row.amount;
      u.cashback += cb;
      return { ...row, rule, cashback: cb };
    });
    return { rows: out, used };
  }

  function bestCard(purpose, amount, used) {
    let best = null;
    for (const card of CARDS) {
      const rule = ruleFor(card, purpose);
      if (!rule) continue;
      let cb = rawCashback(amount, rule);
      if (rule.limit !== null) cb = Math.max(0, Math.min(cb, rule.limit - used.get(rule).cashback));
      if (!best || cb > best.cashback) best = { card, rule, cashback: cb };
    }
    return best;
  }

  function cell(text, cls) {
    const td = document.createElement("td");
    if (cls) td.className = cls;
    td.textContent = text;
    return td;
  }

  let lastTotals = {};
  function render(addedIndex) {
    const { rows: done, used } = compute(rows);
    body.replaceChildren(
      ...done.map((r, i) => {
        const tr = document.createElement("tr");
        if (i === addedIndex) tr.className = "added";
        tr.append(
          cell(String(i + 1), "n"),
          cell(r.name),
          cell(r.purpose),
          cell(r.who, "opt"),
          cell(`${String(r.day).padStart(2, "0")}.11.2026`, "opt"),
          cell(money(r.amount), "num"),
          cell(r.rule && r.cashback ? percent(r.rule.rateBp) : "", "num"),
          cell(r.cashback ? money(r.cashback) : "", "num"),
          cell(r.card),
        );
        return tr;
      }),
    );

    const totals = { all: 0 };
    for (const r of rows) {
      totals.all += r.amount;
      totals[r.purpose] = (totals[r.purpose] || 0) + r.amount;
    }
    sheet.querySelectorAll("[data-total]").forEach((el) => {
      const key = el.dataset.total;
      const value = totals[key] || 0;
      el.textContent = money(value);
      if (lastTotals[key] !== undefined && lastTotals[key] !== value) {
        el.classList.remove("bump");
        void el.offsetWidth;
        el.classList.add("bump");
      }
    });
    lastTotals = totals;

    const list = $("[data-limits]");
    list.replaceChildren(
      ...RULES.map((rule) => {
        const u = used.get(rule);
        const li = document.createElement("li");
        const title = document.createElement("b");
        title.textContent = `${rule.card} · ${rule.purpose ?? "на всё"} ${percent(rule.rateBp)}`;
        li.append(title, `потрачено ${money(u.spent)} ₽ · кэшбэк ${money(u.cashback)} ₽`);
        if (rule.limit !== null) {
          const meter = document.createElement("span");
          meter.className = "meter";
          const fill = document.createElement("i");
          fill.style.width = `${Math.min(100, (u.cashback / rule.limit) * 100)}%`;
          meter.append(fill);
          li.append(meter);
          const left = rule.limit - u.cashback;
          if (left <= 0) {
            const gone = document.createElement("span");
            gone.className = "gone";
            gone.textContent = "лимит исчерпан";
            li.append(gone);
          } else {
            li.append(`осталось кэшбэка ${money(left)} ₽`);
          }
        } else {
          li.append(document.createElement("br"), "без лимита");
        }
        return li;
      }),
    );
    return used;
  }

  let used = render();

  function preview() {
    const amount = parseAmount(amountInput.value);
    const purpose = purposeInput.value;
    const set = (key, text) => ($(`[data-preview="${key}"]`).textContent = text);
    if (!amount) {
      set("rate", "");
      set("cashback", "");
      set("card", "");
      advice.className = "advice";
      advice.textContent =
        "Введите наименование и сумму — здесь появится подсказка, какой картой платить.";
      return null;
    }
    const best = bestCard(purpose, amount, used);
    set("rate", best.cashback ? percent(best.rule.rateBp) : "");
    set("cashback", best.cashback ? money(best.cashback) : "");
    set("card", best.card);

    const exhausted = RULES.find(
      (r) => r.purpose === purpose && r.limit !== null && used.get(r).cashback >= r.limit,
    );
    advice.className = "advice hint";
    advice.replaceChildren();
    if (exhausted)
      advice.append(`Лимит кэшбэка ${exhausted.card} в категории «${purpose}» уже выбран. `);
    const b = document.createElement("b");
    b.textContent = best.card;
    advice.append(
      "Выгоднее картой ",
      b,
      `: ${percent(best.rule.rateBp)}, кэшбэк ${money(best.cashback)} ₽.`,
    );
    if (best.rule.limit !== null) {
      const left = best.rule.limit - used.get(best.rule).cashback - best.cashback;
      advice.append(
        left > 0
          ? ` После покупки останется ${money(left)} ₽ лимита.`
          : " Лимит этой карты на месяц будет выбран.",
      );
    }
    advice.append(" Нажмите Enter, чтобы внести.");
    return best;
  }

  function commit() {
    const name = nameInput.value.trim();
    const amount = parseAmount(amountInput.value);
    if (!name) {
      nameInput.focus();
      return;
    }
    if (!amount) {
      amountInput.focus();
      return;
    }
    const best = bestCard(purposeInput.value, amount, used);
    rows.push({
      name,
      purpose: purposeInput.value,
      who: "Марина",
      day: today,
      amount,
      card: best.card,
    });
    if (rows.length > 40) rows.splice(0, rows.length - 40);
    used = render(rows.length - 1);
    nameInput.value = "";
    amountInput.value = "";
    preview();
    advice.className = "advice";
    advice.textContent = `Внесено: ${name}, ${money(amount)} ₽ картой ${best.card}${best.cashback ? `, кэшбэк ${money(best.cashback)} ₽` : ""}. Итоги и лимиты уже пересчитаны.`;
    scroller.scrollTop = scroller.scrollHeight;
    nameInput.focus({ preventScroll: true });
  }

  nameInput.addEventListener("input", () => {
    const purpose = HISTORY[nameInput.value.trim()];
    if (purpose) purposeInput.value = purpose;
    preview();
  });
  purposeInput.addEventListener("change", preview);
  amountInput.addEventListener("input", preview);
  for (const input of [nameInput, purposeInput, amountInput]) {
    input.addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      e.preventDefault();
      if (input === nameInput && !amountInput.value) amountInput.focus();
      else commit();
    });
  }

  $("[data-today]").textContent = `${String(today).padStart(2, "0")}.11.2026`;
  preview();
  sheet.hidden = false;
  document.querySelector("[data-fallback]")?.remove();
})();
