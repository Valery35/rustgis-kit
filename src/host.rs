// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Связь с RustGIS расширения-процесса (docs/plugins.md RustGIS):
//! строки JSON на вводе и выводе программы.
//!
//! - Один запуск на команду (RustGIS 0.6.0 и раньше): первая строка -
//!   запрос `{"abi", "kind", "id", "params", "lang", ...}`, запрос
//!   к RustGIS - `{"call": {"op", "params"}}`, ответ RustGIS - строка
//!   `{"ok": ...}` или `{"error": "..."}` без `id`, в конце - свой ответ.
//! - Сессия (`"session": true`, RustGIS с сессиями расширений): первая
//!   строка - `{"session": {"abi", "plugin", "rustgis", "lang"}}`, команды -
//!   `{"command": запрос, "id": n}`, ответ - с тем же `id`, запросы
//!   к RustGIS - с `id`, события - `{"event": "layers"}` и другие.
//!
//! [`Host`] держит обе формы, [`Call`] - запрос к RustGIS для любого
//! канала (сессия, один запуск, ретранслятор MCP [`crate::relay`]).

use std::io::{BufRead, Write};

use serde_json::{Value as J, json};

/// Запрос к RustGIS: `host.layers`, `host.features`, `host.view`,
/// `host.addLayer`, `host.notify` или любая команда (`share.exportLayer`).
pub trait Call {
    fn call(&mut self, op: &str, params: J) -> Result<J, String>;
}

/// Строка RustGIS: ввод, вывод, счётчик запросов, отложенные сообщения.
pub struct Host<R: BufRead, W: Write> {
    input: R,
    output: W,
    next: u64,
    /// Команды и события, пришедшие, пока ждали ответа RustGIS.
    queued: Vec<J>,
}

impl<R: BufRead, W: Write> Host<R, W> {
    pub fn new(input: R, output: W) -> Self {
        Self {
            input,
            output,
            next: 0,
            queued: Vec::new(),
        }
    }

    /// Строку JSON - на вывод.
    pub fn send(&mut self, v: &J) -> Result<(), String> {
        writeln!(self.output, "{v}")
            .and_then(|()| self.output.flush())
            .map_err(|e| e.to_string())
    }

    /// Следующая строка JSON ввода. Ввод закрыт - None. Строка не JSON -
    /// чужой вывод, пропускается.
    fn read(&mut self) -> Option<J> {
        loop {
            let mut line = String::new();
            match self.input.read_line(&mut line) {
                Ok(0) | Err(_) => return None,
                Ok(_) => {}
            }
            let line = line.trim_start_matches('\u{feff}').trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<J>(line) {
                return Some(v);
            }
        }
    }

    /// Следующее сообщение RustGIS: сначала отложенные.
    pub fn next_message(&mut self) -> Option<J> {
        if self.queued.is_empty() {
            self.read()
        } else {
            Some(self.queued.remove(0))
        }
    }
}

impl<R: BufRead, W: Write> Call for Host<R, W> {
    /// Запрос к RustGIS и его ответ. Пришедшее до ответа запоминается.
    fn call(&mut self, op: &str, params: J) -> Result<J, String> {
        self.next += 1;
        let id = format!("kit{}", self.next);
        self.send(&json!({"call": {"op": op, "params": params}, "id": id}))?;
        loop {
            let Some(v) = self.read() else {
                return Err("RustGIS closed the plugin session".to_string());
            };
            // один запуск: ответ - строка без id
            let bare = v.get("id").is_none() && (v.get("ok").is_some() || v.get("error").is_some());
            if bare || v["id"].as_str() == Some(id.as_str()) {
                return match v.get("error") {
                    Some(e) => Err(e.as_str().unwrap_or("error").to_string()),
                    None => Ok(v["ok"].clone()),
                };
            }
            self.queued.push(v);
        }
    }
}

/// Первая строка - начало сессии? Тогда её код языка.
pub fn hello(line: &str) -> Option<String> {
    let v: J = serde_json::from_str(line.trim_start_matches('\u{feff}').trim()).ok()?;
    let s = v.get("session")?.as_object()?;
    Some(
        s.get("lang")
            .and_then(J::as_str)
            .unwrap_or("en")
            .to_string(),
    )
}

/// Запрос RustGIS из строки. Не JSON - ошибка с текстом разбора.
pub fn parse_request(line: &str) -> Result<J, String> {
    serde_json::from_str(line.trim_start_matches('\u{feff}').trim()).map_err(|e| e.to_string())
}

/// Параметры запроса: объект или строка JSON с объектом.
pub fn params_of(request: &J) -> J {
    match &request["params"] {
        J::String(s) => serde_json::from_str(s).unwrap_or(J::Null),
        p => p.clone(),
    }
}

/// Путь для RustGIS: косая черта вместо обратной.
pub fn slash(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_and_requests() {
        assert_eq!(
            hello(r#"{"session":{"abi":1,"plugin":"x","rustgis":"0.6.0","lang":"ru"}}"#),
            Some("ru".to_string())
        );
        assert_eq!(hello(r#"{"id":"tools"}"#), None);
        assert_eq!(hello("not json"), None);
        assert!(parse_request("{").is_err());
        let r = json!({"params": "{\"a\": 1}"});
        assert_eq!(params_of(&r)["a"], 1);
    }

    /// Один запуск: ответ RustGIS без id. Сессия: ответ с id, события
    /// до ответа откладываются.
    #[test]
    fn calls_in_both_forms() {
        let input = "{\"ok\": [1]}\n";
        let mut out = Vec::new();
        let mut h = Host::new(input.as_bytes(), &mut out);
        assert_eq!(h.call("host.layers", json!({})).ok(), Some(json!([1])));
        let input = "{\"event\": \"layers\"}\n{\"ok\": 5, \"id\": \"kit1\"}\n";
        let mut out = Vec::new();
        let mut h = Host::new(input.as_bytes(), &mut out);
        assert_eq!(h.call("host.view", json!({})).ok(), Some(json!(5)));
        assert_eq!(h.next_message(), Some(json!({"event": "layers"})));
        assert!(h.call("host.view", json!({})).is_err());
    }
}
