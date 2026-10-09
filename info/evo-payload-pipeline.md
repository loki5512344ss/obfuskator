# Payload-раннер EvoVis: как мод попадает в игру

Разбор по коду (байт-код `evo-agent.jar`, `payload.json`, `runtime.json`, структура
`evojdk_x/`). Источник: `javap -p -constants -cp mod/evo-agent.jar …`.

## Участники

| артефакт | роль |
|---|---|
| `mod/payload.json` | манифест: `version`, `file`, `sha256`, `size` |
| `mod/evovis.payload` (47.5 MB) | зашифрованный jar мода (AES-GCM) |
| `mod/evo-agent.jar` (60 KB) | Java-agent: расшифровывает payload и подсовывает его Fabric |
| `mod/evojdk-win-x64.zip` → `bin/java.dll`, `classes/…/java/lang/EvoShare.class` | **подменённый JVM**: класс `java.lang.EvoShare.compute(...)` отдаёт вторую долю ключа |
| `mod/jar/META-INF/jars/*.jar` (7 шт.) | вложенные зависимости, распаковываются агентом |
| `mod/jar/META-INF/evo/native/{win-x64,win-arm64,macos-x64,macos-arm64}/evoguard.dll\|dylib` | нативная защита мода (внутри уже загруженного мода) |

## Манифест (`payload.json`)

```json
{ "version": "2026.10.1.994", "file": "evovis.payload",
  "sha256": "e118274a92181e2a8ef04d2f6c45c4f6d4a10b87119d9d41c6f8e39b1bd5f485",
  "size": 47491101 }
```

Лаунчер качает `payload.json` → сверяет sha256 → качает `evovis.payload`,
`evo-agent.jar`, `evojdk-*.zip` → поднимает JVM с агентом.

## Конфигурация агента (из констант `EvoAgent`)

| константа | значение |
|---|---|
| `MOUNT_NAME` | `"evovis"` |
| payload | свойство `evo.agent.payload` / env `EVO_AGENT_PAYLOAD` |
| ключ | свойство `evo.agent.key` / env `EVO_AGENT_KEY` (hex, 32 байта) |
| токен / hwid | env `EVO_TOKEN`, `EVO_HWID` |
| вложенные библиотеки | `evo.agent.libraries` / `EVO_AGENT_LIBRARIES`, каталог `libraries` |

Манифест агента: `Premain-Class: evo.agent.EvoAgent`,
`Launcher-Agent-Class: evo.agent.EvoAgent`,
`Can-Redefine-Classes: true`, `Can-Retransform-Classes: true`.

## Цепочка запуска

```
laунчер
  ├─ sha256(payload.json) → качает evovis.payload / evo-agent.jar / evojdk
  ├─ env: EVO_AGENT_PAYLOAD, EVO_AGENT_KEY, EVO_TOKEN, EVO_HWID, EVO_AGENT_LIBRARIES
  └─ java -javaagent:evo-agent.jar …  (свой evojdk, в нём java.lang.EvoShare)
        │
        ▼
EvoAgent.premain/agentmain(args, inst)
  ├─ keyProvider(): LocalKeyProvider(hex-ключ из env)  +  JvmShare.share(hwid)
  │     └─ NativeJvmShare → reflect вызов java.lang.EvoShare.compute(...) → 32 байта
  ├─ PayloadCipher.decrypt(bytes(payload), key)   // AES/GCM/NoPadding, IV 12B, tag 128b
  ├─ NestedLibraries.extract(payloadBytes, dir)   // META-INF/jars/*.jar → <dir>/libraries
  ├─ PreparedMod(mount="evovis", candidate=Path, classpathUrl, libraries)
  └─ bootstrap(inst)
        ├─ appendToSystemClassLoaderSearch(classpathUrl)  → классы мода видны JVM
        └─ inst.addTransformer(FabricDiscoveryInjector)
              │
              ▼
FabricDiscoveryInjector.transform(...)  // java.lang.classfile API (байткод-в-байткоде)
  ├─ цель: net/fabricmc/loader/impl/discovery/ArgumentModCandidateFinder.findCandidates
  ├─ вставляет вызов EvoAgentHook.contribute(Consumer)  (слот CONSUMER_SLOT = 1)
  └─ consume.accept(PreparedMod) → Fabric видит мод-кандидат, которого нет на диске
        │
        ▼
Fabric загружает мод → мод сам распаковывает evoguard.dll из META-INF/evo/native/
  → SealedBoot / KeyAssembler / SessionHeartbeat / NativeGuard
```

Ключевые свойства:

1. **Мод не лежит на диске в открытом виде** - расшифровка в памяти агента,
   `candidate` - временный mount-файл.
2. **Ключ собирается из двух мест**: hex из env (лаунчер) + `java.lang.EvoShare.compute(hwid)`
   в **подменённом JVM** - поэтому мод гоняется только на их evojdk.
3. **Инъекция в Fabric** - не мод в `mods/`, а байткод-трансформация внутреннего
   класса Fabric (`ArgumentModCandidateFinder`) через `java.lang.classfile` API.
   Классы: `EvoAgent`, `EvoAgentHook`, `FabricDiscoveryInjector`, `PreparedMod`,
   `NestedLibraries`.
4. Далее внутри мода - свой слой: `KeyAssembler` (Shamir: сервер + integrity + native + hwid),
   `SealedBoot` (sealed.bin в этом билде отсутствует → fail-open), `SessionHeartbeat`.

## Что здесь не проверено

- Тело `java.dll` / `EvoShare` - натив не реверсился (план в `todo.md`).
- Реальное поведение при запуске (нужен MC 26.2 + Fabric 0.19.5 + их evojdk + дисплей).
- `evoguard.dll` - не реверсился.
