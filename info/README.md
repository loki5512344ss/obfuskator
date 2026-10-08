# info/ — материал по обфускации EvoVis (для нашего обфускатора)

Источник: `LoVisual/ref/evo/` (реверс EvoVis / EvolutionDLC, Minecraft-клиент).
Сюда перенесено только то, что касается защиты/обфускации кода.

| файл | что это |
|---|---|
| `evo-obfuscation.md` | базовое описание трёх уровней обфускации (перенесено as-is из `ref/evo/obfuscation.md`) |
| `evo-obfuscation-full.md` | **сводный разбор**: все уровни + .NET-лаунчер + натив/антиотладка, с кодом и проверенными цифрами |
| `evo-weaknesses.md` | где защита провалилась при разборе — чего нам делать нельзя (+ опровержения) |
| `evo-improvements.md` | что сделать в своём обфускаторе, чтобы этих дыр не было |
| `evo-payload-pipeline.md` | доставка: AES-256-GCM payload + java-agent + подменённый JVM |
| `evo-native-reverse.md` | реверс `evoguard.dll` / `java.dll`: HMAC-шейры, антиотладка, порча ключа |
| `evo-nobackend.md` | локальный патч их клиента (как вырезали бэкенд/телеметрию) |

Инструменты деобфускации (Rust, zero-dep) лежат в `LoVisual/ref/evo/scripts/`:
`awc.rs` (LCG+XOR шифр строк), `expr.rs` (оценка opaque-констант),
`deobfuscate_mod.rs`, `deobfuscate_launcher.rs`, `make_plain.rs`, `restructure.rs`.
