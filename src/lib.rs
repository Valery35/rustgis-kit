// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Общая основа расширений-процессов RustGIS линейки Информ++: Isoliner3D,
//! Isoliner, Topoliner, Routeliner.
//!
//! Продукт описывает инструменты `tools.json` своего модуля QGIS и считает
//! по значениям в формате прогонов модуля (`tool_runs.json`). Остальное
//! делает крейт:
//!
//! - [`host`] - протокол RustGIS: команды, `host.*`, сессии и один запуск;
//! - [`runner`] - расширение целиком: трейт [`runner::Product`] и
//!   [`runner::serve_stdio`] для `main`;
//! - [`catalog`] и [`manifest`] - описания инструментов и раздел `tools`
//!   манифеста `plugin.json` (окно инструмента рисует сам RustGIS);
//! - [`tool`] - запрос RustGIS в значения прогона и выходы прогона в слои
//!   и таблицы проекта;
//! - [`geo`], [`geom`], [`project`], [`shp`] - системы координат: зона
//!   UTM, проекция карты, выгрузка слоёв в систему силами RustGIS,
//!   итог в метрах с кодом EPSG;
//! - [`store`] - высоты итогов: слой RustGIS их не держит;
//! - [`window`] - окно продукта своим процессом, замок окна;
//! - [`relay`] - ретранслятор MCP RustGIS: живая связь без сессий;
//! - [`i18n`] - тексты крейта на двух языках.
//!
//! Упаковка `.rgplugin` - `scripts/package.ps1`. Как сделать расширение -
//! `README.md`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod catalog;
pub mod geo;
pub mod geom;
pub mod host;
pub mod i18n;
pub mod manifest;
pub mod project;
pub mod relay;
pub mod runner;
pub mod shp;
pub mod store;
pub mod tool;
pub mod window;

pub use catalog::{Catalog, Lang, Param, Text, Tool};
pub use host::Call;
pub use runner::{Failure, Product, serve, serve_stdio};
pub use store::Store;
pub use tool::Options;
