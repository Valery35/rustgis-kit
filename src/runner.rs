// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Расширение целиком: продукт ([`Product`]) и обслуживание RustGIS
//! ([`serve`]).
//!
//! Продукт даёт описания инструментов, расчёт по значениям прогона
//! модуля, свои команды (окно, «О программе») и, если хочет, окно для
//! итогов с высотой. Остальное делает крейт: разбор запроса, перевод
//! координат, слои и таблицы итога, хранилище высот, сессия и один запуск.

use std::io::{BufRead, Write};

use serde_json::{Value as J, json};

use crate::catalog::{Catalog, Lang, Tool};
use crate::host::{Call, Host, hello, parse_request};
use crate::i18n::tr;
use crate::store::Store;
use crate::tool::{Options, finish, prepare};

/// Отказ расчёта продукта.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Значения не годятся: замечания через запятую.
    Values(String),
    /// Расчёт не удался: текст на языке показа.
    Run(String),
}

/// Продукт: всё, что у расширения своё.
pub trait Product {
    /// Описания инструментов (`tools.json` модуля).
    fn catalog(&self) -> &Catalog;
    /// Инструменты в RustGIS и группа панели.
    fn options(&self) -> &Options;
    /// Хранилище высот итогов.
    fn store(&self) -> Store;
    /// Расчёт инструмента по значениям прогона модуля. Ответ - выходы
    /// в формате `output` модуля (по ключам) и сообщения расчёта на языке
    /// показа.
    fn compute(&self, tool: &Tool, values: &J, lang: Lang) -> Result<(J, Vec<String>), Failure>;
    /// Своя команда продукта (не инструмент). Ответ - `{"ok": ...}` или
    /// `{"error": ...}`. `host` - запросы к RustGIS внутри команды.
    fn command(&self, host: &mut dyn Call, req: &J, lang: Lang) -> J;
    /// Итоги с высотой инструмента `tool` - в окно продукта. Сообщение
    /// для журнала RustGIS, None - окна нет.
    fn show(&self, _tool: &Tool, _solids: &[(String, J)], _lang: Lang) -> Option<String> {
        None
    }
    /// Событие сессии RustGIS (`layers`, `view`, `selection`, `project`).
    fn event(&self, _host: &mut dyn Call, _name: &str) {}
}

/// Инструмент RustGIS: разбор, расчёт продукта, ответ.
pub fn run_tool(product: &dyn Product, req: &J, lang: Lang) -> J {
    let store = product.store();
    let p = match prepare(product.catalog(), product.options(), &store, req, lang) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let (outputs, messages) = match product.compute(p.tool, &p.values, lang) {
        Ok(r) => r,
        Err(Failure::Values(why)) => {
            return json!({"error": tr(lang, "Значения не годятся: %s", &[why])});
        }
        Err(Failure::Run(why)) => {
            return json!({"error": tr(lang, "Расчёт не удался: %s", &[why])});
        }
    };
    let mut done = finish(&p, &outputs, messages, &store, lang);
    if !done.solids.is_empty()
        && let Some(m) = product.show(p.tool, &done.solids, lang)
        && let Some(list) = done.reply["ok"]["messages"].as_array_mut()
    {
        list.push(json!(m));
    }
    done.reply
}

/// Ответ на запрос RustGIS: инструмент - через крейт, команда - продукту.
pub fn answer(product: &dyn Product, host: &mut dyn Call, req: &J, lang: Lang) -> J {
    if req["kind"] == "tool" {
        run_tool(product, req, lang)
    } else {
        product.command(host, req, lang)
    }
}

/// Обслужить RustGIS на вводе и выводе: сессия до закрытия ввода или
/// одна команда. Ошибка - текст для журнала (ввод или вывод отказали).
pub fn serve<R: BufRead, W: Write>(
    product: &dyn Product,
    mut input: R,
    output: W,
) -> Result<(), String> {
    // первая строка - как есть: запрос не JSON получает ответ-ошибку
    let mut first_line = String::new();
    match input.read_line(&mut first_line) {
        Ok(0) => return Err("RustGIS: no request".into()),
        Ok(_) => {}
        Err(e) => return Err(format!("RustGIS: {e}")),
    }
    let mut host = Host::new(input, output);
    // сессия: команды с id, события, запросы к RustGIS с id
    if let Some(lang) = hello(&first_line) {
        while let Some(msg) = host.next_message() {
            if let Some(cmd) = msg.get("command") {
                let mut req = cmd.clone();
                if req.get("lang").is_none() {
                    req["lang"] = json!(lang);
                }
                let l = Lang::of(req["lang"].as_str());
                let mut reply = answer(product, &mut host, &req, l);
                reply["id"] = msg["id"].clone();
                host.send(&reply)?;
            } else if let Some(ev) = msg["event"].as_str() {
                product.event(&mut host, ev);
            }
        }
        return Ok(());
    }
    // один запуск: запрос, запросы к RustGIS без id, ответ
    let reply = match parse_request(&first_line) {
        Ok(req) => {
            let l = Lang::of(req["lang"].as_str());
            answer(product, &mut host, &req, l)
        }
        Err(e) => json!({"error": tr(Lang::En, "Запрос RustGIS не разобран: %s", &[e])}),
    };
    host.send(&reply)
}

/// [`serve`] на стандартных вводе и выводе: `main` расширения при ключе
/// `rustgis-plugin`.
pub fn serve_stdio(product: &dyn Product) -> std::process::ExitCode {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match serve(product, stdin.lock(), stdout.lock()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Ответ «команды нет» на языке показа.
pub fn no_command(id: &str, lang: Lang) -> J {
    json!({"error": tr(lang, "Команды «%s» нет.", &[id.to_string()])})
}
