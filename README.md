<div align="center">

# noxis

Обфускатор JVM/Java-байткода на Rust.

![Rust](https://img.shields.io/badge/Rust-2024_edition-orange?style=flat-square&logo=rust&logoColor=white)
![MSRV](https://img.shields.io/badge/MSRV-1.97-informational?style=flat-square)
![License](https://img.shields.io/badge/license-AGPL--3.0-blue?style=flat-square&logo=gnu&logoColor=white)
![Status](https://img.shields.io/badge/status-в_разработке-yellow?style=flat-square)

</div>

---

Читает JAR, переписывает классовые файлы и ресурсы, собирает обратно: детерминированно,
с маппингами для stack trace и с проверкой, что выход действительно загружается настоящей JVM.

## Статус

Ядро построено, большая часть техник реализована.

| блок | состояние |
|---|---|
| IR классовых файлов, метки вместо смещений | ✅ |
| конфиг, детерминированный PRNG, маппинги, отчёт | ✅ |
| анализ: иерархия, entrypoint'ы, keep-правила | ✅ |
| `format::parse`/`write` + кадры `StackMapTable` (backend `ristretto_classfile`) | ✅ |
| пассы: rename, resources, strings, constants, noise, debug | ✅ |
| верификация: red-паттерны, `java -Xverify:all`, метрики | ✅ |
| CLI (`obfuscate` / `analyze` / `patterns`) | ✅ |
| пасс flow (кадры готовы, 4 схемы: state-spread, computed-switch, butterfly, exception-goto) | ✅ |

Подробности в [`docs/architecture.md`](docs/architecture.md), план в [`todo.md`](todo.md).

## Быстрый старт

```sh
./scripts/check.sh     # fmt + clippy -D warnings + тесты

# обфускация jar: конвейер, маппинги, JSON-отчёт
cargo run -p noxis -- obfuscate input.jar -o output.jar --seed 42

# что внутри jar: классы, entrypoint'ы, keep-правила (без изменений)
cargo run -p noxis -- analyze input.jar

# красный список паттернов: показать или прогнать по jar'у
cargo run -p noxis -- patterns --list
cargo run -p noxis -- patterns --input output.jar
```

`obfuscate` по умолчанию включает все готовые пассы. Флаги `--no-rename`,
`--no-strings`, `--no-constants`, `--no-noise`, `--no-resources`, `--no-debug-info`,
`--no-flow` выключают их по отдельности.

## Структура

```text
noxis/
├── Cargo.toml              workspace, lints, release-профиль
├── scripts/check.sh        structure → fmt → clippy → test
├── docs/                   архитектура, техники, стек, уроки
├── info/                   исходный материал разбора (вне правил)
└── crates/
    ├── noxis-classfile/        IR + парсер/запись + StackMapTable
    ├── noxis-core/             конфиг, seed, маппинги, контракт Pass
    ├── noxis-analysis/         иерархия, entrypoint'ы, keep
    ├── noxis-passes/           сами трансформации + конвейер
    ├── noxis-runtime/          схемы шифрования, ABI декодера
    ├── noxis-jar/              ZIP, ресурсы, срез подписей
    ├── noxis-verify/           red-паттерны, java -Xverify, метрики
    ├── noxis/                  CLI: obfuscate, analyze, patterns
    └── noxis-structure-check/  автогейт структуры
```

## Документация

| документ | что внутри |
|---|---|
| [`docs/README.md`](docs/README.md) | индекс и структурные правила проекта |
| [`docs/architecture.md`](docs/architecture.md) | слои, крейты, IR, конвейер |
| [`docs/techniques.md`](docs/techniques.md) | как работает каждая техника, плюсы и минусы |
| [`docs/stack.md`](docs/stack.md) | почему свой IR, backend, `StackMapTable`, PRNG |
| [`docs/learnings/lessons.md`](docs/learnings/lessons.md) | уроки разбора: чего делать нельзя |
| [`docs/learnings/reference.md`](docs/learnings/reference.md) | разбор референсного обфускатора |
| [`docs/learnings/priorities.md`](docs/learnings/priorities.md) | что улучшаем и в каком порядке |
| [`docs/learnings/ai-resistance.md`](docs/learnings/ai-resistance.md) | новые техники (MBA, виртуализация) и устойчивость к LLM-анализу |

Исходный материал разбора в [`info/README.md`](info/README.md).

## Лицензия

AGPL-3.0-only, см. [`LICENSE`](LICENSE).

Проект остаётся полностью открытым: его можно изучать, использовать (в том числе
коммерчески) и изменять. Условие AGPL: любые производные работы, которые
распространяются или доступны пользователям по сети, обязаны публиковаться под
той же лицензией. Это защищает `noxis` от закрытых форков, которые перепродают
его как готовый продукт.

Сторонние зависимости и их лицензии в [`NOTICE.md`](NOTICE.md).

## Ответственное использование

`noxis` это инструмент двойного назначения, одинаково полезный для защиты
собственного байткода и способен усложнять анализ вредоносного ПО. Лицензия
это не регламентирует, но это норма сообщества проекта:

- используйте обфускацию, чтобы защищать **свой** код и код тех, кто вас об этом попросил;
- не применяйте инструмент для упаковки вредоносов, обхода средств защиты
  и антивирусов или иной деятельности, запрещённой законом.

Нарушение этих норм не влечёт юридических последствий по лицензии, но приведёт
к отказу в поддержке и закрытию issue/PR в репозитории.
