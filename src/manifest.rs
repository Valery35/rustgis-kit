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
//! несколько полей - `text` с именами через запятую,
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
        // несколько полей: у RustGIS выбора нескольких полей нет, имена -
        // текстом через запятую, разбор - tool::values
        "field" if p.multiple => "text",
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

/// Текст без HTML: `<br>`, `<p>`, `<li>` и заголовки - переводы строк,
/// прочие теги убираются, сущности (`&amp;`, `&nbsp;`, `&#39;`) -
/// знаками, пробелы внутри строки схлопываются. Справки модулей бывают
/// в HTML (Topoliner), окно RustGIS показывает текст.
pub fn plain_text(s: &str) -> String {
    if !s.contains('<') && !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            out.push_str(&rest[i..]);
            rest = "";
            break;
        };
        let tag = rest[i + 1..i + j]
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(
            tag.as_str(),
            "br" | "p"
                | "div"
                | "li"
                | "ul"
                | "ol"
                | "tr"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
        ) {
            out.push('\n');
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    let mut text = String::new();
    let mut rest = out.as_str();
    while let Some(i) = rest.find('&') {
        text.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(';').filter(|&e| e <= 10);
        let ch = end.and_then(|e| match &tail[1..e] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        });
        match (ch, end) {
            (Some(c), Some(e)) => {
                text.push(c);
                rest = &tail[e + 1..];
            }
            _ => {
                text.push('&');
                rest = &tail[1..];
            }
        }
    }
    text.push_str(rest);
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    // пустые строки подряд - одна, по краям - ни одной
    let mut kept: Vec<String> = Vec::new();
    for l in lines {
        if !(l.is_empty() && kept.last().is_none_or(String::is_empty)) {
            kept.push(l);
        }
    }
    while kept.last().is_some_and(String::is_empty) {
        kept.pop();
    }
    kept.join("\n")
}

/// Первое предложение справки: строка `summary` RustGIS. HTML снимается,
/// строки-заголовки, равные одному из `titles` (название инструмента
/// с кодом и без), пропускаются.
pub fn summary(help: &str, titles: &[&str]) -> String {
    let text = plain_text(help);
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !titles.iter().any(|t| t.trim() == *l))
        .unwrap_or("");
    match line.find(". ") {
        Some(i) => line[..=i].to_string(),
        None => line.to_string(),
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
    tools_section_hiding(catalog, opts, &[])
}

/// Скрыто ли поле `key` инструмента `tool`: пара (инструмент, ключ),
/// инструмент `*` - у всех.
pub fn is_hidden(hidden: &[(&str, &str)], tool: &str, key: &str) -> bool {
    hidden
        .iter()
        .any(|(t, k)| (*t == tool || *t == "*") && *k == key)
}

/// [`tools_section`] без полей `hidden`: пары (инструмент Processing,
/// ключ поля), `*` вместо инструмента - у всех. Поля, бесполезные
/// в RustGIS (M у слоёв без M), в окно не идут, расчёт берёт их умолчание
/// из `tools.json`. Обязательное поле без умолчания скрыть нельзя - оно
/// остаётся в окне.
pub fn tools_section_hiding(
    catalog: &Catalog,
    opts: &Options,
    hidden: &[(&str, &str)],
) -> (Vec<J>, Map<String, J>) {
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
        let titles = |l: Lang| [t.display_name(l), t.title.get(l).to_string()];
        let (te, tr) = (titles(Lang::En), titles(Lang::Ru));
        let sum = summary(t.help.get(Lang::En), &[&te[0], &te[1]]);
        say(&sum, &summary(t.help.get(Lang::Ru), &[&tr[0], &tr[1]]));
        let mut params = Vec::new();
        for p in &t.params {
            let Some(kind) = rg_kind(p) else { continue };
            let can_hide = p.optional || p.default.as_ref().is_some_and(|d| !d.is_null());
            if can_hide && is_hidden(hidden, &t.name, &p.key) {
                continue;
            }
            let label = plain_text(p.label.get(Lang::En));
            say(&label, &plain_text(p.label.get(Lang::Ru)));
            let mut o = json!({"name": p.rg_name(), "label": label, "kind": kind});
            if p.is_layer() && !p.optional {
                o["required"] = json!(true);
            }
            let hint = plain_text(p.hint.get(Lang::En));
            let hint_ru = plain_text(p.hint.get(Lang::Ru));
            // подсказка в окне и её перевод - одна пара, без лишних строк
            let mut help: Option<(String, String)> =
                (!hint.is_empty()).then(|| (hint.clone(), hint_ru.clone()));
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
                let help_ru = format!("{} Через запятую, из списка {}.", hint_ru, all(Lang::Ru));
                help = Some((help_en, help_ru));
                let d = pick(Lang::En);
                if !d.is_empty() {
                    o["default"] = json!(d);
                }
            } else if p.kind == "field" && p.multiple {
                // имена полей слоя через запятую, перечень зависит от слоя
                let tail = |l: Lang| match (l, p.numeric) {
                    (Lang::En, false) => "Field names of the layer, comma-separated.",
                    (Lang::En, true) => "Names of numeric fields of the layer, comma-separated.",
                    (Lang::Ru, false) => "Имена полей слоя через запятую.",
                    (Lang::Ru, true) => "Имена числовых полей слоя через запятую.",
                };
                let join = |a: &str, b: &str| {
                    if a.is_empty() {
                        b.to_string()
                    } else {
                        format!("{a} {b}")
                    }
                };
                help = Some((join(&hint, tail(Lang::En)), join(&hint_ru, tail(Lang::Ru))));
                let d = match &p.default {
                    Some(J::Array(a)) => a
                        .iter()
                        .filter_map(J::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                    Some(J::String(s)) => s.clone(),
                    _ => String::new(),
                };
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
            if let Some((en, r)) = help {
                say(&en, &r);
                o["help"] = json!(en);
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

/// Подпись на двух языках для вкладки, групп и меню.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label {
    pub en: &'static str,
    pub ru: &'static str,
}

/// Размер кнопок: крупные во всю высоту ленты или мелкие по три в столбец.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Large,
    Small,
}

impl Layout {
    fn name(self) -> &'static str {
        match self {
            Layout::Large => "large",
            Layout::Small => "small",
        }
    }
}

/// Кнопка вкладки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// Команда из `commands` манифеста по id.
    Command(&'static str),
    /// Инструмент по имени Processing (`sweep_body`) - `tool:<id RustGIS>`.
    Tool(&'static str),
    /// Панель расширения.
    Panel,
    /// Выпадающее меню.
    Menu(&'static Menu),
}

/// Выпадающее меню: значок - имя значка RustGIS (`polygon_layer`,
/// `table`...), полный список - `ALL` в `crates/ui-egui/src/icons.rs`
/// RustGIS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Menu {
    pub label: Label,
    pub icon: Option<&'static str>,
    pub help: Option<Label>,
    pub layout: Option<Layout>,
    pub items: &'static [Item],
}

/// Группа вкладки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    pub label: Label,
    pub layout: Layout,
    pub items: &'static [Item],
}

/// Своя вкладка ленты RustGIS (docs/plugins.md, «A tab of its own»):
/// вкладка с 0.1.0, меню и мелкие кнопки - с RustGIS 0.5.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tab {
    pub label: Label,
    pub groups: &'static [Group],
}

/// Имена вкладок RustGIS, которые занимать нельзя (и прежние имена).
const RESERVED_TABS: &[&str] = &[
    "Home", "Map", "Insert", "Data", "Style", "Edit", "Analyze", "View", "Imagery", "Share",
    "Publish", "Plugins", "Settings", "Labeling", "Help", "Layout",
];

/// Сборка вкладки: каталог, инструменты продукта, команды, счёт кнопок
/// и русские строки.
struct TabBuild<'a> {
    catalog: &'a Catalog,
    opts: &'a Options,
    commands: &'a [&'a str],
    count: usize,
    ru: Map<String, J>,
}

impl TabBuild<'_> {
    fn say(&mut self, l: &Label) {
        if !l.en.is_empty() && l.en != l.ru {
            self.ru.insert(l.en.to_string(), json!(l.ru));
        }
    }

    /// Подпись: одна строка от 1 до 32 знаков.
    fn label(l: &Label, what: &str) -> Result<J, String> {
        let n = l.en.chars().count();
        if !(1..=32).contains(&n) || l.en.contains('\n') {
            return Err(format!("{what} '{}': one line of 1 to 32 characters", l.en));
        }
        Ok(json!(l.en))
    }

    fn item(&mut self, it: &Item, depth: usize) -> Result<J, String> {
        self.count += 1;
        Ok(match it {
            Item::Command(id) => {
                if !self.commands.contains(id) {
                    return Err(format!("tab: no command '{id}'"));
                }
                json!(id)
            }
            Item::Tool(name) => {
                let t = self
                    .catalog
                    .tool(name)
                    .filter(|_| self.opts.offered.contains(name))
                    .ok_or_else(|| format!("tab: tool '{name}' is not offered"))?;
                json!(format!("tool:{}", t.rg_id()))
            }
            Item::Panel => json!("panel"),
            Item::Menu(m) => {
                if depth > 4 {
                    return Err(format!(
                        "tab: menu '{}' nests deeper than 4 levels",
                        m.label.en
                    ));
                }
                if !(1..=32).contains(&m.items.len()) {
                    return Err(format!("tab: menu '{}' needs 1 to 32 items", m.label.en));
                }
                self.say(&m.label);
                let mut o = json!({"menu": Self::label(&m.label, "menu")?});
                if let Some(icon) = m.icon {
                    o["icon"] = json!(icon);
                }
                if let Some(h) = &m.help {
                    self.say(h);
                    o["help"] = json!(h.en);
                }
                if let Some(l) = m.layout {
                    o["layout"] = json!(l.name());
                }
                let items = m
                    .items
                    .iter()
                    .map(|x| self.item(x, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                o["items"] = json!(items);
                o
            }
        })
    }
}

/// Раздел `tab` манифеста и русские строки к нему. `commands` - id команд
/// манифеста. Ошибка - текст о том, что нарушает правила RustGIS: число
/// групп и кнопок, глубина меню, длина подписи, занятое имя вкладки,
/// инструмент вне `opts.offered`, чужая команда.
pub fn tab_section(
    tab: &Tab,
    catalog: &Catalog,
    opts: &Options,
    commands: &[&str],
) -> Result<(J, Map<String, J>), String> {
    if RESERVED_TABS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(tab.label.en))
    {
        return Err(format!("tab '{}': the name of a RustGIS tab", tab.label.en));
    }
    if !(1..=8).contains(&tab.groups.len()) {
        return Err("tab: 1 to 8 groups".into());
    }
    let mut b = TabBuild {
        catalog,
        opts,
        commands,
        count: 0,
        ru: Map::new(),
    };
    b.say(&tab.label);
    let mut groups = Vec::new();
    for g in tab.groups {
        let max = match g.layout {
            Layout::Large => 16,
            Layout::Small => 48,
        };
        if !(1..=max).contains(&g.items.len()) {
            return Err(format!(
                "tab: group '{}' needs 1 to {max} items",
                g.label.en
            ));
        }
        b.say(&g.label);
        let items = g
            .items
            .iter()
            .map(|x| b.item(x, 1))
            .collect::<Result<Vec<_>, _>>()?;
        let mut o = json!({"label": TabBuild::label(&g.label, "group")?, "items": items});
        if g.layout == Layout::Small {
            o["layout"] = json!("small");
        }
        groups.push(o);
    }
    if b.count > 16_384 {
        return Err("tab: more than 16384 items".into());
    }
    let out = json!({"label": TabBuild::label(&tab.label, "tab")?, "groups": groups});
    Ok((out, b.ru))
}

/// Все строки значения JSON, вглубь.
fn strings(v: &J, out: &mut std::collections::HashSet<String>) {
    match v {
        J::String(s) => {
            out.insert(s.clone());
        }
        J::Array(a) => a.iter().for_each(|x| strings(x, out)),
        J::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/// Вставить раздел `tools` и русские строки в текст манифеста, не трогая
/// порядок и вид остального: раздел - перед `"i18n"`, по инструменту на
/// строку, новые переводы - в начало `"ru"`. Прежний раздел `tools`
/// заменяется. Перевод, чей ключ был только в прежнем разделе, удаляется,
/// изменённый перевод заменяется на месте (с 0.1.5). Манифест - объект
/// верхнего уровня с отступом в два пробела, как
/// `serde_json::to_string_pretty`, переводы - по строке на ключ.
pub fn splice(text: &str, tools: &[J], ru: &Map<String, J>) -> Result<String, String> {
    splice_all(text, tools, None, ru)
}

/// [`splice`] вместе со своей вкладкой ленты `tab` ([`tab_section`],
/// с 0.1.9): вкладка - одной строкой перед `"tools"`, прежняя заменяется,
/// None - прежняя убирается. Переводы прежней вкладки чистятся так же,
/// как переводы прежнего раздела `tools`.
pub fn splice_all(
    text: &str,
    tools: &[J],
    tab: Option<&J>,
    ru: &Map<String, J>,
) -> Result<String, String> {
    let mut text = text.replace("\r\n", "\n");
    let old: J = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut old_tool_strings = std::collections::HashSet::new();
    strings(&old["tools"], &mut old_tool_strings);
    strings(&old["tab"], &mut old_tool_strings);
    // прежняя вкладка - одна строка `  "tab": {...},`
    if let Some(s) = text.find("\n  \"tab\": ") {
        let e = text[s + 1..].find('\n').map_or(text.len(), |e| s + 1 + e);
        text.replace_range(s..e, "");
    }
    if let Some(s) = text.find("\n  \"tools\": [") {
        let e = text[s..]
            .find("\n  ],")
            .map(|e| s + e + "\n  ],".len())
            .ok_or("plugin.json: tools without an end")?;
        text.replace_range(s..e, "");
    }
    let lines: Vec<String> = tools.iter().map(|t| format!("    {t}")).collect();
    let mut block = format!("\n  \"tools\": [\n{}\n  ],", lines.join(",\n"));
    if let Some(tab) = tab {
        block.insert_str(0, &format!("\n  \"tab\": {tab},"));
    }
    let at = text
        .find("\n  \"i18n\": {")
        .ok_or("plugin.json: no i18n section")?;
    text.insert_str(at, &block);
    let m: J = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    // строки манифеста вне tools и i18n (описание, команды): их переводы
    // не трогаются
    let mut other = std::collections::HashSet::new();
    if let Some(o) = m.as_object() {
        for (k, v) in o {
            if k != "tools" && k != "tab" && k != "i18n" {
                strings(v, &mut other);
            }
        }
    }
    let key = "\"ru\": {\n";
    let start = text.find(key).ok_or("plugin.json: no i18n.ru section")? + key.len();
    // пустой ru - закрывающая скобка сразу
    let empty = text[start..].starts_with("    }");
    let len = if empty {
        0
    } else {
        text[start..]
            .find("\n    }")
            .ok_or("plugin.json: i18n.ru without an end")?
    };
    let mut entries: Vec<String> = ru
        .iter()
        .filter(|(k, _)| m["i18n"]["ru"].get(*k).is_none())
        .map(|(k, v)| format!("      {}: {v}", json!(k)))
        .collect();
    for line in text[start..start + len].lines() {
        let body = line.trim().trim_end_matches(',');
        let parsed: Option<(String, J)> =
            serde_json::from_str::<Map<String, J>>(&format!("{{{body}}}"))
                .ok()
                .and_then(|o| o.into_iter().next());
        let Some((k, v)) = parsed else {
            // не строка перевода - как есть
            if !body.is_empty() {
                entries.push(line.trim_end_matches(',').to_string());
            }
            continue;
        };
        match ru.get(&k) {
            Some(new) if *new != v => entries.push(format!("      {}: {new}", json!(k))),
            Some(_) => entries.push(line.trim_end_matches(',').to_string()),
            None if old_tool_strings.contains(&k) && !other.contains(&k) => {}
            None => entries.push(line.trim_end_matches(',').to_string()),
        }
    }
    let mut block = entries.join(",\n");
    if empty && !block.is_empty() {
        block.push('\n');
    }
    text.replace_range(start..start + len, &block);
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
        assert_eq!(again, out);
        // справка сменилась: старый перевод уходит, изменённый - заменяется,
        // чужие строки остаются
        let mut ru2 = ru.clone();
        ru2.remove("Does the job.");
        ru2.insert("Does it better.".into(), json!("Делает лучше."));
        ru2.insert("Lines".into(), json!("Линии слоя"));
        let mut tools2 = tools.clone();
        tools2[0]["summary"] = json!("Does it better.");
        let third = splice(&out, &tools2, &ru2).unwrap_or_default();
        let m3: J = serde_json::from_str(&third).unwrap_or(J::Null);
        assert!(m3["i18n"]["ru"].get("Does the job.").is_none(), "{third}");
        assert_eq!(m3["i18n"]["ru"]["Does it better."], "Делает лучше.");
        assert_eq!(m3["i18n"]["ru"]["Lines"], "Линии слоя");
        assert_eq!(m3["i18n"]["ru"]["x"], "y");
        // пустой ru
        let bare = "{\n  \"id\": \"b\",\n  \"i18n\": {\n    \"ru\": {\n    }\n  }\n}\n";
        let b: J =
            serde_json::from_str(&splice(bare, &tools, &ru).unwrap_or_default()).unwrap_or(J::Null);
        assert_eq!(b["i18n"]["ru"]["Lines"], "Линии");
        // скрытое поле с умолчанием уходит из окна, обязательный слой - нет
        let (hid, _) = tools_section_hiding(
            &cat,
            &opts,
            &[("hello_tool", "MODE"), ("*", "INPUT"), ("other", "X")],
        );
        let names: Vec<&str> = hid[0]["params"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["name"].as_str())
            .collect();
        assert_eq!(names, ["input"]);
        assert!(is_hidden(&[("*", "USE_M")], "any", "USE_M"));
    }

    /// Несколько полей: в окне текст через запятую, в значениях - список.
    #[test]
    fn multiple_fields() {
        let cat = Catalog::parse(
            r#"{"tools": [{"code": "1.07", "name": "group_check", "title": {"ru": "Сборка", "en": "Build"},
            "params": [{"key": "INPUT", "kind": "lines", "label": {"ru": "Слой", "en": "Layer"}},
                       {"key": "GROUP", "kind": "field", "multiple": true, "parent": "INPUT",
                        "default": ["a", "b"],
                        "label": {"ru": "Группы", "en": "Groups"},
                        "hint": {"ru": "Поля группировки.", "en": "Grouping fields."}}]}]}"#,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let opts = Options {
            toolbox: "T",
            offered: &["group_check"],
            xy_tables: &[],
        };
        assert!(fits(&cat.tools[0]));
        let (tools, ru) = tools_section(&cat, &opts);
        let g = &tools[0]["params"][1];
        assert_eq!(g["kind"], "text");
        assert_eq!(g["default"], "a, b");
        assert_eq!(
            g["help"],
            "Grouping fields. Field names of the layer, comma-separated."
        );
        assert_eq!(
            ru["Grouping fields. Field names of the layer, comma-separated."],
            "Поля группировки. Имена полей слоя через запятую."
        );
        // подсказка без приписки в переводы не идёт - её нет в окне
        assert!(ru.get("Grouping fields.").is_none());
        let store = crate::store::Store::new(crate::store::temp_home("rustgis-kit-mf"));
        let fc = json!({"type": "FeatureCollection", "features": [{"type": "Feature",
            "properties": {"a": 1}, "geometry": {"type": "LineString", "coordinates": [[56.0, 54.7], [56.001, 54.7]]}}]});
        let req = json!({"id": "group-check", "params": {"input": "L", "group": "a; b ,c"}, "layers": {"L": fc}});
        let p = crate::tool::prepare(&cat, &opts, &store, &req, Lang::Ru)
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(p.values["GROUP"], json!(["a", "b", "c"]));
    }

    /// Своя вкладка: инструменты в меню, команды кнопками, переводы, пределы
    /// RustGIS, вставка в манифест и замена прежней.
    #[test]
    fn own_tab() {
        let cat = Catalog::parse(
            r#"{"tools": [{"code": "9.01", "name": "hello_tool", "title": {"ru": "Привет", "en": "Hello"},
            "params": [{"key": "INPUT", "kind": "lines", "label": {"ru": "Линии", "en": "Lines"}}]}]}"#,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let opts = Options {
            toolbox: "T",
            offered: &["hello_tool"],
            xy_tables: &[],
        };
        static MENU: Menu = Menu {
            label: Label {
                en: "Lines",
                ru: "Линии",
            },
            icon: Some("line_layer"),
            help: None,
            layout: None,
            items: &[Item::Tool("hello_tool")],
        };
        static TAB: Tab = Tab {
            label: Label {
                en: "Hello",
                ru: "Привет",
            },
            groups: &[
                Group {
                    label: Label {
                        en: "Window",
                        ru: "Окно",
                    },
                    layout: Layout::Large,
                    items: &[Item::Command("about")],
                },
                Group {
                    label: Label {
                        en: "Tools",
                        ru: "Инструменты",
                    },
                    layout: Layout::Small,
                    items: &[Item::Menu(&MENU)],
                },
            ],
        };
        let (tab, ru) =
            tab_section(&TAB, &cat, &opts, &["about"]).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(tab["groups"][0]["items"][0], "about");
        assert_eq!(tab["groups"][1]["layout"], "small");
        assert_eq!(tab["groups"][1]["items"][0]["menu"], "Lines");
        assert_eq!(tab["groups"][1]["items"][0]["items"][0], "tool:hello-tool");
        assert_eq!(ru["Tools"], "Инструменты");
        // ошибки: чужая команда, занятое имя вкладки
        assert!(tab_section(&TAB, &cat, &opts, &[]).is_err());
        static HOME: Tab = Tab {
            label: Label {
                en: "Home",
                ru: "Главная",
            },
            groups: &[],
        };
        assert!(tab_section(&HOME, &cat, &opts, &["about"]).is_err());
        // вставка, повтор без изменений, замена и снятие
        let manifest = "{\n  \"id\": \"hello\",\n  \"commands\": [],\n  \"i18n\": {\n    \"ru\": {\n      \"x\": \"y\"\n    }\n  }\n}\n";
        let (tools, mut all) = tools_section(&cat, &opts);
        all.extend(ru);
        let one = splice_all(manifest, &tools, Some(&tab), &all).unwrap_or_default();
        let m: J = serde_json::from_str(&one).unwrap_or(J::Null);
        assert_eq!(m["tab"], tab);
        assert_eq!(m["i18n"]["ru"]["Window"], "Окно");
        assert_eq!(
            splice_all(&one, &tools, Some(&tab), &all).unwrap_or_default(),
            one
        );
        let (_, plain) = tools_section(&cat, &opts);
        let gone = splice_all(&one, &tools, None, &plain).unwrap_or_default();
        let m: J = serde_json::from_str(&gone).unwrap_or(J::Null);
        assert!(m.get("tab").is_none());
        assert!(m["i18n"]["ru"].get("Window").is_none(), "{gone}");
        assert_eq!(m["i18n"]["ru"]["x"], "y");
    }

    /// Справка в HTML (Topoliner): теги сняты, заголовок пропущен.
    #[test]
    fn html_help() {
        let help = "<b>1.01 Polygon topology check</b><br><br>The layer is not modified. \
                    Errors go to a new layer.<ul><li>gaps &amp; overlaps</li></ul>";
        assert_eq!(
            summary(
                help,
                &["1.01 Polygon topology check", "Polygon topology check"]
            ),
            "The layer is not modified."
        );
        assert_eq!(
            plain_text(help),
            "1.01 Polygon topology check\n\nThe layer is not modified. Errors go to a new layer.\n\ngaps & overlaps"
        );
        assert_eq!(
            plain_text("a &lt; b &#8212; c&nbsp;d, 5 & 6"),
            "a < b \u{2014} c d, 5 & 6"
        );
        assert_eq!(plain_text("Plain. Text."), "Plain. Text.");
    }
}
