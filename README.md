# rustgis-kit

Общая основа расширений-процессов RustGIS для продуктов линейки
Информ++ - Isoliner3D, Isoliner, Topoliner, Routeliner. Крейт берёт на
себя всё, что не зависит от продукта. Продукт описывает свои инструменты
и считает их.

Лицензия Apache 2.0. Версия 0.1.2, проверена с RustGIS 0.6.0, расширения
на крейте требуют RustGIS не ниже 0.5.0 (`minRustgisVersion`).

## Что делает крейт

| Модуль | Что в нём |
|---|---|
| `host` | Протокол RustGIS. Один запуск на команду и сессия (`"session": true`), запросы `host.*` с ответом, события |
| `runner` | Расширение целиком. Трейт `Product` и `serve_stdio` для `main` |
| `catalog` | Описания инструментов из `tools.json` модуля QGIS |
| `manifest` | Раздел `tools` манифеста `plugin.json` и его русские строки. Окно инструмента рисует сам RustGIS |
| `tool` | Запрос RustGIS в значения прогона модуля, выходы прогона в слои и таблицы проекта |
| `geo`, `geom` | Зона UTM (Крюгер 6-го порядка), перевод градусов в метры и обратно, план тела верхними гранями |
| `project` | Слои проекта для окна продукта в системе карты. Выгрузка через `share.exportLayer` с `crs`, чтение Shapefile (`shp`) |
| `store` | Высоты итогов. Слой RustGIS держит только X и Y, тела и линии с высотой хранятся рядом |
| `window` | Окно продукта своим процессом вне задания RustGIS, замок окна |
| `relay` | Ретранслятор MCP RustGIS - живая связь с RustGIS из любого процесса, без сессий |
| `i18n` | Тексты крейта на русском и английском |

Сценарий упаковки `.rgplugin` - `scripts/package.ps1`.

## Граница между крейтом и продуктом

Граница - формат прогонов модуля QGIS (`fixtures/tool_runs.json`,
пишет `tools/make_fixtures.py` модуля). Продукт получает значения
в этом формате и отдаёт выходы в нём же.

Значения (`values` вместе с `input` прогона), координаты - метры зоны
UTM по середине входа или метры системы таблицы:

- линии и объекты - `[{"id", "parts": [[[x, y, z]]], "attrs"}]`, высоты нет - `null`;
- точки и таблица - `[{"id", "xyz": [x, y, z] | null, "attrs"}]`;
- тело - `{"verts": [[x, y, z]], "faces": [[a, b, c]]}`, слой тел - `[{"id", "attrs", "verts", "faces"}]`;
- сведения о слое - `"@КЛЮЧ": {"name", "geometry", "hasz", "fields", "numeric"}`;
- пункт списка - номер, несколько пунктов - список номеров, число, поле, текст, флажок - как есть.

Выходы - объект по ключам выходов:

```json
{"OUTPUT": {
  "name": "имя слоя, если у выхода нет поля в tools.json",
  "geometry": "mesh | line | point | source | none | grid",
  "keep_source": "ключ поля-слоя, чьи объекты берёт source",
  "fields": [{"name": "volume", "type": "double"}],
  "features": [{"source": 0, "values": [1184.4], "geometry": {"verts": [], "faces": []}}]
}}
```

Геометрия объекта - `{"verts", "faces"}`, `{"points"}`, `{"parts"}`
(линия с разрывами, с 0.1.1, в RustGIS - MultiLineString), `{"xyz"}`,
`{"source": id}` (объект входа), `null` (строка таблицы) или `{"grid"}`
(куб). Дальше крейт сам кладёт тело в проект контуром верхних граней
с полями `z_min` и `z_max`, итог с высотой - в хранилище и в окно
продукта, строки без геометрии - таблицей, итог таблицы с X и Y - в её
системе с членом `crs`.

## Как сделать расширение продукта

1. Выгрузить описания инструментов модуля в `tools.json`
   (`tools/export_tools.py` модуля, формат - `toolspec.py`).
2. Подключить крейт зависимостью по пути, пока он не опубликован.

   ```toml
   [dependencies]
   rustgis-kit = { path = "../rustgis-kit" }
   serde_json = "1"
   ```

3. Описать продукт трейтом `Product`. Обязательны пять методов.
   - `catalog` - описания из `tools.json`.
   - `options` - группа панели RustGIS, инструменты в RustGIS по имени
     Processing, таблицы с X и Y своей системы.
   - `store` - папка хранилища высот, обычно `store::temp_home("продукт")`.
   - `compute` - расчёт инструмента по значениям прогона. Ошибка значений -
     `Failure::Values`, ошибка расчёта - `Failure::Run`, текст на языке показа.
   - `command` - свои команды из `commands` манифеста (окно, «О программе»).
     Чужая команда - `runner::no_command`.

   Необязательны `show` (итоги с высотой в своё окно продукта) и
   `event` (события сессии, `layers` и другие).
4. В `main` по ключу `rustgis-plugin` вызвать `rustgis_kit::serve_stdio(&продукт)`.
   Крейт сам различит сессию и один запуск.
5. Собрать `plugin.json`. Раздел `tools` и строки его перевода даёт
   `manifest::tools_section`, вставляет в текст манифеста
   `manifest::splice`. Удобно держать тест, который сверяет раздел
   с файлом и по переменной окружения переписывает его.
6. Упаковать.

   ```powershell
   cargo build --release
   powershell -File C:\Dev\rustgis-kit\scripts\package.ps1 `
       -Manifest plugin.json -Program target\release\my-rustgis.exe -Dist dist
   ```

   В `dist` появятся папка `<id>` для «Add Plugin Folder…» и
   `rustgis-cli` и пакет `<id>-<версия>.rgplugin` для установки из файла.
   Путь программы в пакете берётся из `run.windows` манифеста.

## Минимальное расширение на один инструмент

Полный текст - `examples/minimal.rs`, он собирается и проверяется вместе
с крейтом (`cargo test`). Инструмент «9.01 Длина линий» получает слой
линий и отдаёт тот же слой с полем `length` в метрах.

```rust
use rustgis_kit::catalog::Tool;
use rustgis_kit::runner::no_command;
use rustgis_kit::{Call, Catalog, Failure, Lang, Options, Product, Store};
use serde_json::{Value as J, json};

const OPTIONS: Options = Options {
    toolbox: "Minimal",
    offered: &["line_length"],
    xy_tables: &[],
};

struct Minimal {
    catalog: Catalog,
}

impl Product for Minimal {
    fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    fn options(&self) -> &Options {
        &OPTIONS
    }

    fn store(&self) -> Store {
        Store::new(rustgis_kit::store::temp_home("minimal").join("bodies"))
    }

    fn compute(&self, tool: &Tool, values: &J, lang: Lang) -> Result<(J, Vec<String>), Failure> {
        let Some(lines) = values["INPUT"].as_array() else {
            return Err(Failure::Values("INPUT".into()));
        };
        let features: Vec<J> = lines
            .iter()
            .map(|f| {
                let id = f["id"].as_i64().unwrap_or(0);
                let length = length_of(&f["parts"]); // метры зоны UTM
                json!({"source": id, "values": [length], "geometry": {"source": id}})
            })
            .collect();
        let message = tool
            .translate("Линий: %d.", lang)
            .replace("%d", &features.len().to_string());
        let out = json!({"OUTPUT": {
            "geometry": "source",
            "keep_source": "INPUT",
            "fields": [{"name": "length", "type": "double"}],
            "features": features,
        }});
        Ok((out, vec![message]))
    }

    fn command(&self, _host: &mut dyn Call, req: &J, lang: Lang) -> J {
        match req["id"].as_str().unwrap_or("") {
            "about" => json!({"ok": {"title": "Minimal", "version": env!("CARGO_PKG_VERSION")}}),
            id => no_command(id, lang),
        }
    }
}

fn main() -> std::process::ExitCode {
    let catalog = Catalog::parse(include_str!("tools.json")).unwrap_or_else(|e| {
        eprintln!("tools.json: {e}");
        std::process::exit(1)
    });
    rustgis_kit::serve_stdio(&Minimal { catalog })
}
```

Описание инструмента в `tools.json` - как у модуля QGIS.

```json
{"format": 1, "tools": [{
  "code": "9.01", "name": "line_length",
  "title": {"ru": "Длина линий", "en": "Line length"},
  "help": {"ru": "Длина каждой линии в метрах. Поле length.",
           "en": "The length of each line in metres. The length field."},
  "params": [
    {"key": "INPUT", "kind": "lines", "label": {"ru": "Линии", "en": "Lines"}},
    {"key": "OUTPUT", "kind": "sink", "label": {"ru": "Длины линий", "en": "Line lengths"}}
  ],
  "messages": [{"ru": "Линий: %d.", "en": "Lines: %d."}]
}]}
```

Что получится в RustGIS. В панели Tools появится группа Minimal
с инструментом «9.01 Длина линий», окно с полем «Линии» нарисует сам
RustGIS. После запуска в проекте появится слой «Длины линий» - линии
входа с полем `length`, в журнале - «Линий: 1.».

Попробовать без RustGIS.

```powershell
cargo run --example minimal -- manifest            # plugin.json с разделом tools
cargo test --example minimal                       # прогон запросом RustGIS
```

## Страницы руководства

Генератор страниц инструментов написан на Python и живёт в модуле QGIS
Isoliner3D - `tools/tool_pages.py`. Страница - заголовок
«## код название», справка и таблица полей из `tools.json`. Текст ниже
первого подзаголовка пишется руками.

```bash
python3 tools/tool_pages.py --fixtures <папка fixtures продукта>   # страницы для расширения
python3 tools/tool_pages.py --manual                               # руководство модуля
python3 tools/tool_pages.py --check                                # руководство не устарело
```

Страницы ложатся в `fixtures/docs/tools/<код>.<язык>.md`, одни и те же
идут в руководство модуля и в расширение, поэтому QGIS и RustGIS говорят
слово в слово. Другому продукту нужен такой же сценарий у своего модуля
или копия этого с путями своего руководства.

## Ретранслятор MCP

Модуль `relay` подключается к ретранслятору MCP RustGIS
(`127.0.0.1:8767`, порт меняет `RUSTGIS_MCP_PORT`, ключ - файл
`rustgis-mcp-<пользователь>/<порт>.token` во временной папке). Он годится
окну продукта или любому процессу, которому нужна живая связь с RustGIS
без сессии расширения. `Relay` реализует `Call`, поэтому `project_layers`
и другие функции крейта работают через него так же, как через сессию.
Событий ретранслятор не шлёт, изменения узнаются опросом.

```rust
if rustgis_kit::relay::available() {
    let mut r = rustgis_kit::relay::Relay::open(
        rustgis_kit::relay::port(), "my-product", std::env::temp_dir())?;
    let layers = r.layers()?;
}
```

## Проверка

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
```

## Что ждут продукты

Нужды Isoliner от 13 ноября 2026 года (разведка модуля Isoliner,
`isoliner-qgis_rust-port-survey.md` в общей папке заметок).

1. Вид поля «охват» (демо-инструменты Isoliner, 1.09). Не решено, чем
   его задавать в окне RustGIS - слоем, по охвату которого строить, или
   числами центра и размера. Сначала вид поля появляется в `toolspec.py`
   модуля, потом здесь (`catalog`, `manifest::rg_kind`, `tool::prepare`).
2. Выход таблицей без геометрии (вариограмма 1.06). Крейт уже отдаёт
   выход `"geometry": "none"` в `tables` ответа. Принимает ли его RustGIS
   в проект, проверить `rustgis-cli run --plugin` до первого такого
   инструмента.
3. Член `crs` с локальной инженерной системой «расстояние - высота» для
   чертежей разреза (группа 4). Сейчас `crs` - только код EPSG
   (`geo::epsg`), локальной системы RustGIS может не знать, нужна проверка.

## Кто пользуется

Isoliner3D для RustGIS (`isoliner-scene`, `apps/isoliner-rustgis`) -
десять инструментов, сцена из слоёв проекта, итоги с высотой в окне сцены.
