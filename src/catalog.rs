// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Описания инструментов из `tools.json` модуля QGIS: ровно то, что нужно
//! окну инструмента RustGIS и разбору запроса. Формат пишет
//! `tools/export_tools.py` модуля по `toolspec.py` (`Tool`, `Param`).
//!
//! Расчётных типов здесь нет: значения уходят продукту в формате
//! прогонов модуля (`tool_runs.json`, `values` и `input`), продукт сам
//! переводит их в свои типы.

use serde::Deserialize;
use serde_json::Value as J;

/// Язык показа.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Ru,
    En,
}

impl Lang {
    /// Язык запроса RustGIS: `ru...` - русский, иначе английский.
    pub fn of(code: Option<&str>) -> Self {
        match code {
            Some(c) if c.to_lowercase().starts_with("ru") => Lang::Ru,
            _ => Lang::En,
        }
    }
}

/// Строка на двух языках.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct Text {
    #[serde(default)]
    pub ru: String,
    #[serde(default)]
    pub en: String,
}

impl Text {
    pub fn get(&self, lang: Lang) -> &str {
        match lang {
            Lang::Ru if !self.ru.is_empty() => &self.ru,
            Lang::En if !self.en.is_empty() => &self.en,
            _ if self.ru.is_empty() => &self.en,
            _ => &self.ru,
        }
    }
}

/// Поле инструмента. `kind` - вид поля модуля: `lines`, `points`,
/// `features`, `table`, `mesh`, `meshes`, `raster`, `band`, `field`,
/// `string`, `bool`, `number`, `integer`, `enum`, `sink`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Param {
    pub key: String,
    pub kind: String,
    #[serde(default)]
    pub label: Text,
    #[serde(default)]
    pub hint: Text,
    #[serde(default)]
    pub default: Option<J>,
    #[serde(default)]
    pub options: Vec<Text>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub minimum: Option<f64>,
    #[serde(default)]
    pub maximum: Option<f64>,
    #[serde(default)]
    pub numeric: bool,
    #[serde(default)]
    pub multiple: bool,
}

impl Param {
    /// Поле-слой: его значение - имя слоя RustGIS.
    pub fn is_layer(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "lines" | "features" | "points" | "table" | "mesh" | "meshes"
        )
    }

    /// Поле-тело: тела с высотой берутся из итогов (хранилище [`crate::store`]).
    pub fn is_mesh(&self) -> bool {
        matches!(self.kind.as_str(), "mesh" | "meshes")
    }

    /// Имя поля в окне RustGIS: со строчной буквы.
    pub fn rg_name(&self) -> String {
        self.key.to_lowercase()
    }
}

/// Описание инструмента.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Tool {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub title: Text,
    #[serde(default)]
    pub help: Text,
    #[serde(default)]
    pub params: Vec<Param>,
    /// Шаблоны сообщений расчёта и их переводы.
    #[serde(default)]
    pub messages: Vec<Text>,
}

impl Tool {
    /// Название с кодом, как в панели Processing.
    pub fn display_name(&self, lang: Lang) -> String {
        format!("{} {}", self.code, self.title.get(lang))
    }

    pub fn param(&self, key: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.key == key)
    }

    /// Имя инструмента RustGIS: строчные буквы, цифры и дефис.
    pub fn rg_id(&self) -> String {
        self.name.to_lowercase().replace('_', "-")
    }

    /// Перевод русского шаблона или слова на язык показа по `messages`
    /// и пунктам списков. Незнакомое остаётся как есть.
    pub fn translate<'a>(&'a self, template: &'a str, lang: Lang) -> &'a str {
        self.messages
            .iter()
            .chain(self.params.iter().flat_map(|p| p.options.iter()))
            .find(|m| m.ru == template)
            .map_or(template, |m| m.get(lang))
    }
}

/// `tools.json` целиком.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub format: u32,
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub version: String,
    pub tools: Vec<Tool>,
}

impl Catalog {
    /// Прочитать `tools.json`.
    pub fn parse(data: impl AsRef<[u8]>) -> Result<Self, String> {
        serde_json::from_slice(data.as_ref()).map_err(|e| e.to_string())
    }

    /// Инструмент по имени Processing (`sweep_body`).
    pub fn tool(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.name == name)
    }

    /// Инструмент по имени RustGIS (`sweep-body`).
    pub fn by_rg_id(&self, id: &str) -> Option<&Tool> {
        self.tool(&id.replace('-', "_"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_a_small_catalog() {
        let c = Catalog::parse(
            r#"{"format": 1, "tools": [{"code": "9.01", "name": "hello_tool",
            "title": {"ru": "Привет", "en": "Hello"},
            "params": [{"key": "INPUT", "kind": "lines", "label": {"ru": "Линии", "en": "Lines"}},
                       {"key": "MODE", "kind": "enum", "options": [{"ru": "Один", "en": "One"}]}],
            "messages": [{"ru": "Готово: %d.", "en": "Done: %d."}]}]}"#,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let t = c.by_rg_id("hello-tool").unwrap_or_else(|| unreachable!());
        assert_eq!(t.display_name(Lang::En), "9.01 Hello");
        assert_eq!(t.rg_id(), "hello-tool");
        assert_eq!(t.translate("Готово: %d.", Lang::En), "Done: %d.");
        assert_eq!(t.translate("Один", Lang::En), "One");
        assert!(t.params[0].is_layer() && !t.params[1].is_layer());
        assert_eq!(Lang::of(Some("ru_RU")), Lang::Ru);
        assert_eq!(Lang::of(None), Lang::En);
    }
}
