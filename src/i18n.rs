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

/// Спецификатор printf: флаги, ширина, точность, вид.
struct Spec {
    plus: bool,
    space: bool,
    left: bool,
    zero: bool,
    alt: bool,
    width: usize,
    precision: Option<usize>,
    kind: char,
}

impl Spec {
    /// Разбор `spec` без знака `%`: `+.4g`, `-8.2f`, `05d`, `s`.
    fn parse(spec: &str) -> Option<Self> {
        let kind = spec.chars().last()?;
        let body = &spec[..spec.len() - kind.len_utf8()];
        let mut s = Spec {
            plus: false,
            space: false,
            left: false,
            zero: false,
            alt: false,
            width: 0,
            precision: None,
            kind,
        };
        let mut rest = body;
        while let Some(c) = rest.chars().next() {
            match c {
                '+' => s.plus = true,
                ' ' => s.space = true,
                '-' => s.left = true,
                '0' => s.zero = true,
                '#' => s.alt = true,
                _ => break,
            }
            rest = &rest[1..];
        }
        let (w, p) = match rest.split_once('.') {
            Some((w, p)) => (w, Some(p)),
            None => (rest, None),
        };
        // длина у C (l, h) - пропуск
        let digits = |t: &str| -> String { t.chars().filter(char::is_ascii_digit).collect() };
        s.width = digits(w).parse().unwrap_or(0);
        s.precision = p.map(|p| digits(p).parse().unwrap_or(0));
        Some(s)
    }

    /// Число по виду `f`, `e`, `g`, `d` без ширины, со знаком по флагам.
    fn number(&self, x: f64) -> String {
        let body = match self.kind {
            'f' | 'F' => format!("{:.*}", self.precision.unwrap_or(6), x.abs()),
            'e' | 'E' => sci(x.abs(), self.precision.unwrap_or(6), self.kind == 'E'),
            'g' | 'G' => general(
                x.abs(),
                self.precision.unwrap_or(6),
                self.alt,
                self.kind == 'G',
            ),
            _ => format!("{:.0}", x.abs().round()),
        };
        let sign = if x.is_sign_negative() && !x.is_nan() {
            "-"
        } else if self.plus {
            "+"
        } else if self.space {
            " "
        } else {
            ""
        };
        format!("{sign}{body}")
    }

    /// Ширина: справа, слева (`-`) или нулями после знака (`0`).
    fn pad(&self, text: String, numeric: bool) -> String {
        let n = text.chars().count();
        if n >= self.width {
            return text;
        }
        let fill = self.width - n;
        if self.left {
            format!("{text}{}", " ".repeat(fill))
        } else if self.zero && numeric {
            let cut = text
                .char_indices()
                .find(|(_, c)| !matches!(c, '+' | '-' | ' '))
                .map_or(0, |(i, _)| i);
            format!("{}{}{}", &text[..cut], "0".repeat(fill), &text[cut..])
        } else {
            format!("{}{text}", " ".repeat(fill))
        }
    }
}

/// `%e` у C: мантисса с `p` знаками, порядок со знаком и не короче двух цифр.
fn sci(x: f64, p: usize, upper: bool) -> String {
    let s = format!("{x:.p$e}");
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = e.parse().unwrap_or(0);
    let sign = if exp < 0 { '-' } else { '+' };
    let e = if upper { 'E' } else { 'e' };
    format!("{m}{e}{sign}{:02}", exp.abs())
}

/// `%g` у C: `p` значащих цифр, обычная запись при порядке от -4 до p-1,
/// иначе `%e`. Хвостовые нули убираются, если нет флага `#`.
fn general(x: f64, p: usize, alt: bool, upper: bool) -> String {
    let p = p.max(1);
    if x == 0.0 {
        return if alt {
            format!("{:.*}", p - 1, 0.0)
        } else {
            "0".into()
        };
    }
    if !x.is_finite() {
        return format!("{x}");
    }
    // порядок после округления до p цифр
    let exp: i32 = sci(x, p - 1, false)
        .rsplit_once('e')
        .and_then(|(_, e)| e.parse().ok())
        .unwrap_or(0);
    let trim = |s: String| -> String {
        if alt || !s.contains('.') {
            return s;
        }
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let p_i = i32::try_from(p).unwrap_or(i32::MAX);
    if exp < -4 || exp >= p_i {
        let s = sci(x, p - 1, upper);
        let (m, e) = s
            .split_once(if upper { 'E' } else { 'e' })
            .map_or((s.clone(), String::new()), |(m, e)| {
                (m.to_string(), e.to_string())
            });
        let mark = if upper { 'E' } else { 'e' };
        format!("{}{mark}{e}", trim(m))
    } else {
        let decimals = usize::try_from(p_i - 1 - exp).unwrap_or(0);
        trim(format!("{x:.decimals$}"))
    }
}

/// Подставить аргументы в шаблон по порядку. Спецификаторы - как у printf
/// и `%` в Python: `%s`, `%d`, `%f`, `%e`, `%g` с флагами (`+`, `-`, `0`,
/// пробел, `#`), шириной и точностью - `%.4g`, `%+.2f`, `%5d`. Аргумент
/// числового вида, который не число, идёт как есть.
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
            continue;
        }
        let arg = it.next().map_or("", String::as_str);
        let Some(s) = Spec::parse(&spec) else {
            out.push_str(arg);
            continue;
        };
        let x = arg.trim().parse::<f64>().ok();
        let text = match (s.kind, x) {
            ('f' | 'F' | 'e' | 'E' | 'g' | 'G' | 'd' | 'i' | 'u', Some(x)) => {
                s.pad(s.number(x), true)
            }
            // строка с точностью - первые знаки, как у printf
            (_, _) => {
                let t: String = match (s.kind, s.precision) {
                    ('s', Some(p)) => arg.chars().take(p).collect(),
                    _ => arg.to_string(),
                };
                s.pad(t, false)
            }
        };
        out.push_str(&text);
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

    /// Форматы чисел как у printf и `%` Python (сверено с Python 3).
    #[test]
    fn number_formats_like_printf() {
        let f = |t: &str, a: &str| fill(t, &[a.to_string()]);
        assert_eq!(f("%.4g", "1234.5678"), "1235");
        assert_eq!(f("%.4g", "0.000123456"), "0.0001235");
        assert_eq!(f("%.4g", "0.0000123456"), "1.235e-05");
        assert_eq!(f("%.4g", "12345678"), "1.235e+07");
        assert_eq!(f("%.4g", "2.5"), "2.5");
        assert_eq!(f("%g", "100000"), "100000");
        assert_eq!(f("%g", "1000000"), "1e+06");
        assert_eq!(f("%g", "0"), "0");
        assert_eq!(f("%.3g", "9.9999"), "10");
        assert_eq!(f("%+.2f", "3.14159"), "+3.14");
        assert_eq!(f("%+.2f", "-3.14159"), "-3.14");
        assert_eq!(f("%.1f", "2.25"), "2.2");
        assert_eq!(f("%.2e", "12345"), "1.23e+04");
        assert_eq!(f("%d", "42"), "42");
        assert_eq!(f("%5d", "42"), "   42");
        assert_eq!(f("%-5d|", "42"), "42   |");
        assert_eq!(f("%05.1f", "-2.5"), "-02.5");
        assert_eq!(f("%.3s", "abcdef"), "abc");
        assert_eq!(f("%.2f", "нет"), "нет");
        assert_eq!(f("%+.1f", "-0.01"), "-0.0");
    }
}
