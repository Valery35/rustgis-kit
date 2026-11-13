// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Тексты крейта на двух языках. Ключ - русский текст, `%s` и `%d` -
//! подстановки по порядку. Тексты продукта - у продукта, тексты
//! инструментов - в `tools.json`.

use crate::catalog::Lang;

pub const TABLE: &[(&str, &str)] = &[
    ("Инструмента «%s» нет.", "No tool '%s'."),
    ("Команды «%s» нет.", "No command '%s'."),
    (
        "Запрос RustGIS не разобран: %s",
        "The RustGIS request was not read: %s",
    ),
    ("Значения не годятся: %s", "The values do not fit: %s"),
    ("%s: значение не того вида", "%s: wrong kind of value"),
    ("Расчёт не удался: %s", "The run failed: %s"),
    (
        "Слой «%s» без тел с высотой. Слой RustGIS высоты не хранит, тела дают итоги инструментов расширения.",
        "Layer '%s' has no bodies with heights. A RustGIS layer keeps no heights, the plugin's tool results give bodies.",
    ),
    (
        "Нужен код системы координат X и Y таблицы, например EPSG:28410.",
        "The table needs the EPSG code of its X and Y, for example EPSG:28410.",
    ),
    (
        "%s: тела в плане, верхними гранями, метры зоны UTM %s.",
        "%s: bodies in plan by their top faces, metres of UTM zone %s.",
    ),
    (
        "%s: куб - в окне сцены, ячейками точками.",
        "%s: the cube is in the scene window, its cells as points.",
    ),
];

/// Подставить аргументы в шаблон: `%s`, `%d` и `%.Nf` - по порядку.
pub fn fill(template: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut it = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        // спецификатор до буквы: %s, %d, %.1f
        let mut spec = String::new();
        while let Some(&n) = chars.peek() {
            spec.push(n);
            chars.next();
            if n.is_ascii_alphabetic() || n == '%' {
                break;
            }
        }
        if spec == "%" {
            out.push('%');
        } else {
            out.push_str(it.next().map_or("", String::as_str));
        }
    }
    out
}

/// Текст крейта на языке показа с подстановкой.
pub fn tr(lang: Lang, ru: &str, args: &[String]) -> String {
    let template = match lang {
        Lang::Ru => ru,
        Lang::En => TABLE.iter().find(|(r, _)| *r == ru).map_or(ru, |(_, e)| *e),
    };
    fill(template, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_unique_and_translated() {
        for (i, (ru, en)) in TABLE.iter().enumerate() {
            assert!(!en.is_empty());
            assert_eq!(ru.matches('%').count(), en.matches('%').count(), "{ru}");
            assert!(TABLE.iter().skip(i + 1).all(|(r, _)| r != ru), "{ru}");
        }
        assert_eq!(
            tr(Lang::En, "Инструмента «%s» нет.", &["x".into()]),
            "No tool 'x'."
        );
        assert_eq!(
            fill("%s и %d, 100%%", &["a".into(), "2".into()]),
            "a и 2, 100%"
        );
    }
}
