// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Раздел `tools` манифеста `plugin.json` из `tools.json`.
//!
//! Окно инструмента рисует сам RustGIS (панель «Инструменты») по разделу
//! `tools` манифеста. Раздел строится здесь: подписи, подсказки и пункты
//! списков - те же, что у модуля QGIS, русские строки - в `i18n.ru`.
//!
//! Виды полей модуля в виды RustGIS: линии и объекты - `vectorLayer`,
//! точки - `pointLayer`, таблица - `layer`, тела - `polygonLayer` (слой
//! итога, высоты - из хранилища), поле - `field` или `numericField`,
//! число - `number`, пункт - `choice`, несколько пунктов - `text` через
//! запятую, флажок и необязательный выход - `bool`, строка - `text`.
//! Растра и номера канала у расширений RustGIS нет: такие инструменты
//! не предлагаются.
//!
//! Продукт держит раздел в своём `plugin.json` и сверяет его тестом:
//! [`tools_section`] строит, [`splice`] вставляет в текст манифеста, не
//! трогая порядок и вид остального.

use serde_json::{Map, Value as J, json};

use crate::catalog::{Catalog, Lang, Param};
use crate::tool::{CRS_PARAM, Options};

/// Подпись и подсказка поля кода системы таблицы (en, ru).
pub const CRS_LABEL: (&str, &str) = (
    "Coordinate system of X and Y (EPSG)",
    "Система координат X и Y (EPSG)",
);
pub const CRS_HELP: (&str, &str) = (
    "For a table without points: the EPSG code of its X and Y, for example EPSG:28410. The result goes to the project in that system, RustGIS draws it in place. Points need no code.",
    "Для таблицы без точек: код EPSG её X и Y, например EPSG:28410. Итог ложится в проект в этой системе, RustGIS ставит его на место сам. Точкам код не нужен.",
);

/// Вид поля RustGIS. None - выход или вид, которого у RustGIS нет.
pub fn rg_kind(p: &Param) -> Option<&'static str> {
    Some(match p.kind.as_str() {
        "lines" | "features" => "vectorLayer",
        "points" => "pointLayer",
        "table" => "layer",
        "mesh" | "meshes" => "polygonLayer",
        "field" if p.numeric => "numericField",
        "field" => "field",
        "number" | "integer" => "number",
        "enum" if !p.multiple => "choice",
        "enum" => "text",
        "bool" => "bool",
        "sink" if p.optional => "bool",
        "string" => "text",
        _ => return None,
    })
}

/// Первое предложение подсказки: строка `summary` RustGIS.
pub fn summary(help: &str) -> &str {
    let line = help.lines().next().unwrap_or("").trim();
    match line.find(". ") {
        Some(i) => &line[..=i],
        None => line,
    }
}

/// Предлагается ли инструмент: у каждого обязательного поля есть вид
/// RustGIS.
pub fn fits(t: &crate::catalog::Tool) -> bool {
    t.params
        .iter()
        .all(|p| p.kind == "sink" || p.optional || rg_kind(p).is_some())
}

/// Раздел `tools` манифеста и русские строки к нему (английский ключ -
/// русский перевод).
pub fn tools_section(catalog: &Catalog, opts: &Options) -> (Vec<J>, Map<String, J>) {
    let mut ru = Map::new();
    let mut say = |en: &str, r: &str| {
        if !en.is_empty() && en != r {
            ru.insert(en.to_string(), json!(r));
        }
    };
    let mut tools = Vec::new();
    for name in opts.offered {
        let Some(t) = catalog.tool(name) else {
            continue;
        };
        let title = t.display_name(Lang::En);
        say(&title, &t.display_name(Lang::Ru));
        let sum = summary(t.help.get(Lang::En));
        say(sum, summary(t.help.get(Lang::Ru)));
        let mut params = Vec::new();
        for p in &t.params {
            let Some(kind) = rg_kind(p) else { continue };
            let label = p.label.get(Lang::En);
            say(label, p.label.get(Lang::Ru));
            let mut o = json!({"name": p.rg_name(), "label": label, "kind": kind});
            if p.is_layer() && !p.optional {
                o["required"] = json!(true);
            }
            let hint = p.hint.get(Lang::En);
            if !hint.is_empty() {
                o["help"] = json!(hint);
                say(hint, p.hint.get(Lang::Ru));
            }
            if p.kind == "enum" && p.multiple {
                // пункты через запятую, их перечень - в подсказке
                let ix: Vec<usize> = match &p.default {
                    Some(J::Array(a)) => {
                        a.iter().filter_map(J::as_u64).map(|i| i as usize).collect()
                    }
                    Some(J::Number(n)) => n.as_u64().map(|i| vec![i as usize]).unwrap_or_default(),
                    _ => Vec::new(),
                };
                let pick = |l: Lang| -> String {
                    ix.iter()
                        .filter_map(|&i| p.options.get(i))
                        .map(|o| o.get(l).to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let all = |l: Lang| -> String {
                    p.options
                        .iter()
                        .map(|o| o.get(l).to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let help_en = format!("{} Comma-separated, from {}.", hint, all(Lang::En));
                let help_ru = format!(
                    "{} Через запятую, из списка {}.",
                    p.hint.get(Lang::Ru),
                    all(Lang::Ru)
                );
                say(&help_en, &help_ru);
                o["help"] = json!(help_en);
                let d = pick(Lang::En);
                if !d.is_empty() {
                    o["default"] = json!(d);
                }
            } else if p.kind == "enum" {
                let choices: Vec<&str> = p.options.iter().map(|c| c.get(Lang::En)).collect();
                for c in &p.options {
                    say(c.get(Lang::En), c.get(Lang::Ru));
                }
                let i = p.default.as_ref().and_then(J::as_u64).unwrap_or(0) as usize;
                if let Some(d) = choices.get(i) {
                    o["default"] = json!(d);
                }
                o["choices"] = json!(choices);
            } else if let Some(d) = p
                .default
                .as_ref()
                .filter(|d| !d.is_null() && d.as_str() != Some("") && !p.is_layer())
            {
                o["default"] = d.clone();
            }
            params.push(o);
        }
        if opts.xy_tables.iter().any(|(n, _)| *n == t.name) {
            say(CRS_LABEL.0, CRS_LABEL.1);
            say(CRS_HELP.0, CRS_HELP.1);
            params.push(json!({"name": CRS_PARAM, "label": CRS_LABEL.0, "kind": "text", "help": CRS_HELP.0}));
        }
        tools.push(json!({
            "id": t.rg_id(),
            "name": title,
            "summary": sum,
            "toolbox": opts.toolbox,
            "params": params,
        }));
    }
    (tools, ru)
}

/// Вставить раздел `tools` и новые русские строки в текст манифеста, не
/// трогая порядок и вид остального: раздел - перед `"i18n"`, по
/// инструменту на строку, переводы - в начало `"ru"`. Прежний раздел
/// `tools` заменяется. Манифест - объект верхнего уровня с отступом
/// в два пробела, как `serde_json::to_string_pretty`.
pub fn splice(text: &str, tools: &[J], ru: &Map<String, J>) -> Result<String, String> {
    let mut text = text.replace("\r\n", "\n");
    if let Some(s) = text.find("\n  \"tools\": [") {
        let e = text[s..]
            .find("\n  ],")
            .map(|e| s + e + "\n  ],".len())
            .ok_or("plugin.json: tools without an end")?;
        text.replace_range(s..e, "");
    }
    let lines: Vec<String> = tools.iter().map(|t| format!("    {t}")).collect();
    let block = format!("\n  \"tools\": [\n{}\n  ],", lines.join(",\n"));
    let at = text
        .find("\n  \"i18n\": {")
        .ok_or("plugin.json: no i18n section")?;
    text.insert_str(at, &block);
    let m: J = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut add = String::new();
    for (k, v) in ru {
        if m["i18n"]["ru"].get(k).is_none() {
            add.push_str(&format!("      {}: {v},\n", json!(k)));
        }
    }
    let key = "\"ru\": {\n";
    let at = text.find(key).ok_or("plugin.json: no i18n.ru section")? + key.len();
    text.insert_str(at, &add);
    serde_json::from_str::<J>(&text).map_err(|e| e.to_string())?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_and_splice() {
        let cat = Catalog::parse(
            r#"{"tools": [{"code": "9.01", "name": "hello_tool", "title": {"ru": "Привет", "en": "Hello"},
            "help": {"ru": "Делает дело. Подробно.", "en": "Does the job. In detail."},
            "params": [{"key": "INPUT", "kind": "lines", "label": {"ru": "Линии", "en": "Lines"}},
                       {"key": "MODE", "kind": "enum", "default": 1,
                        "options": [{"ru": "Один", "en": "One"}, {"ru": "Два", "en": "Two"}]},
                       {"key": "GRID", "kind": "raster"},
                       {"key": "OUTPUT", "kind": "sink"}]}]}"#,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let opts = Options {
            toolbox: "Hello",
            offered: &["hello_tool"],
            xy_tables: &[],
        };
        let (tools, ru) = tools_section(&cat, &opts);
        assert_eq!(tools[0]["id"], "hello-tool");
        assert_eq!(tools[0]["summary"], "Does the job.");
        assert_eq!(tools[0]["params"][1]["default"], "Two");
        assert_eq!(tools[0]["params"].as_array().map(Vec::len), Some(2));
        assert_eq!(ru["9.01 Hello"], "9.01 Привет");
        assert!(!fits(&cat.tools[0]));
        let manifest = "{\n  \"id\": \"hello\",\n  \"commands\": [],\n  \"i18n\": {\n    \"ru\": {\n      \"x\": \"y\"\n    }\n  }\n}\n";
        let out = splice(manifest, &tools, &ru).unwrap_or_default();
        let m: J = serde_json::from_str(&out).unwrap_or(J::Null);
        assert_eq!(m["tools"], json!(tools));
        assert_eq!(m["i18n"]["ru"]["Lines"], "Линии");
        // повторная вставка заменяет раздел, а не дублирует
        let again = splice(&out, &tools, &ru).unwrap_or_default();
        assert_eq!(again.matches("\"tools\"").count(), 1);
    }
}
