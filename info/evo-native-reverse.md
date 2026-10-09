# Реверс нативных библиотек EvoVis (Ghidra headless)

Инструменты: Ghidra 12.1.2 (`/opt/ghidra/support/analyzeHeadless`), radare2, objdump, javap.
Скрипты: `reverse/DumpDecomp.java` (дамп всех функций), `reverse/FocusDecomp.java` (точечная
декомпиляция: точка входа + её вызываемые + все функции, пишущие в поле со смещением `0x53`).

Воспроизведение:

```bash
cd ref/evo/reverse
/opt/ghidra/support/analyzeHeadless "$PWD/proj" EvoRev \
  -import "$PWD/../mod/jar/META-INF/evo/native/win-x64/evoguard.dll" \
  -postScript DumpDecomp.java "$PWD/out/evoguard.c" -scriptPath "$PWD"
/opt/ghidra/support/analyzeHeadless "$PWD/proj" EvoRev \
  -import "$PWD/../mod/evojdk_x/bin/java.dll" \
  -postScript DumpDecomp.java "$PWD/out/java_dll.c" -scriptPath "$PWD"
/opt/ghidra/support/analyzeHeadless "$PWD/proj" EvoJvm \
  -import "$PWD/../mod/evojdk_x/bin/server/jvm.dll" \
  -postScript FocusDecomp.java "$PWD/out/jvm_focus.c" JVM_EvoForeignAgentPresent -scriptPath "$PWD"
```

## Что реверсилось

| файл | размер | секций | содержимое |
|---|---|---|---|
| `mod/jar/META-INF/evo/native/win-x64/evoguard.dll` | 51 321 | 18 | своя маленькая JNI-библиотека (MinGW, PE32+) |
| `mod/evojdk_x/bin/java.dll` | 111 104 | 6 | стандартный `java.dll` из JDK **+ новая функция** |
| `mod/evojdk_x/bin/server/jvm.dll` | 15 463 936 | - | **патченный HotSpot** + новый экспорт |

Отдельно (Java, не натив): `mod/evojdk_x/classes/java/lang/EvoShare.class`,
исходник - `mod/evoshare/sources/java/lang/EvoShare.java` (JADX), вызывающий код -
`mod/agent-src/sources/evo/agent/key/NativeJvmShare.java`.

---

## 1. evoguard.dll - JNI-«нативный шейр»

Экспорт ровно один (плюс mingw-рантайм `_CRT_INIT`, `DllMainCRTStartup`, `atexit`,
`sha256_block/update/final`, `env_flags_debug`, `hmac_sha256.constprop.0`) - **`JNI_OnLoad` нет**,
`nm`-имена не вычищены (COFF-символы остались).

### `Java_evo_systems_security_natives_NativeGuard_nativeShare(JNIEnv*, jclass, jstring hwid)` → `byte[32]`

```c
if (hwid == NULL) return NULL;
str = (*env)->GetStringUTFChars(env, hwid, NULL);
hmac_sha256(str, strlen(str), out32);          // HMAC-SHA256 с фиксированным ключом, 32 байта
(*env)->ReleaseStringUTFChars(env, hwid, str);

debugged = IsDebuggerPresent()
        || CheckRemoteDebuggerPresent(GetCurrentProcess(), &flag) && flag
        || env_flags_debug("JAVA_TOOL_OPTIONS")
        || env_flags_debug("_JAVA_OPTIONS");
if (debugged) out32 ^= 0xA5A5A5A5_A5A5A5A5 ...;   // отравление результата

byte[] res = (*env)->NewByteArray(env, 32);
(*env)->SetByteArrayRegion(env, res, 0, 32, out32);
return res;
```

`env_flags_debug(name)`: читает переменную окружения; если она содержит `jdwp` или `agentlib`
→ считается отладкой. Плюс чек `IsDebuggerPresent` / `CheckRemoteDebuggerPresent`.

**Ключ HMAC (32 байта, `.rdata` @ `0x3b00251c0`):**

```
d42394b1dbe62f2d466fe6f8c4ee3cf5ce46949a032627a9ba00e8c5ef533bd2
```

Итог: `NativeGuard.share(hwid) = HMAC-SHA256(key_eg, hwid_utf8)`, при детекте отладки -
`XOR 0xA5` (не ошибка, а **молчаливо неверный шейр**).

### Платформенный паритет (`evoguard`)

| файл | размер | ключ | вывод |
|---|---|---|---|
| `win-x64/evoguard.dll` | 51 321 | `d42394b1…533bd2` в `.rdata` | базовый (реверс выше) |
| `win-arm64/evoguard.dll` | 66 560 | **тот же**, но лежит как предвычисленный `key^0x36` / `key^0x5c` (ipad/opad) | код идентичен по смыслу |
| `macos-x64/libevoguard.dylib` | 8 936 | `d42394b1…533bd2` (offset 2096) | тот же |
| `macos-arm64/libevoguard.dylib` | 33 936 | `d42394b1…533bd2` (offset 2080) | тот же |

Проверка ARM64: байты по адресу `0x180003050` = `e215a287…d9650de4`, XOR `0x36` →
ровно `d42394b1dbe62f2d466fe6f8c4ee3cf5ce46949a032627a9ba00e8c5ef533bd2`; следующие 32
байта = тот же ключ с XOR `0x5c`. SHA-IV рядом (`0x180003090` = `67e6096a…`).
Во всех сборках есть строки `IsDebuggerPresent`, `jdwp`, `agentlib`, `JAVA_TOOL_OPTIONS`,
`_JAVA_OPTIONS` - логика антиотладки одинаковая. **Linux-сборки натива нет вообще**.

Потребитель: `alm.java` (KeyAssembler) - шейр идёт в 4-of-4 сборку Шамира:

```java
byte[] nativeShare = NativeGuard.share(hwid);
ama.combine(new int[]{share.x(), share.xIntegrity(), share.xNative(), share.xHwid()},
            new byte[][]{share.y(), integrityShare, nativeShare, hwidShare});
```

То есть «отравленный» нативный шейр ломает сборку ключа без единого сообщения об ошибке.

---

## 2. java.dll - `java.lang.EvoShare.computeRaw` (патченный JDK)

Экспорт: `Java_java_lang_EvoShare_computeRaw` (@ `0x180001ca4`, 286 байт) +
импорт `JVM_EvoForeignAgentPresent` **из `jvm.dll`** (см. §3).

```c
longlong Java_java_lang_EvoShare_computeRaw(JNIEnv* env, jclass cls, jbyteArray input) {
  if (input == NULL) return NULL;
  len  = (*env)->GetArrayLength(env, input);
  bytes = (*env)->GetByteArrayElements(env, input, NULL);
  FUN_180001e4c(bytes, len, out32);            // HMAC-SHA256 + антиотладка (см. ниже)
  (*env)->ReleaseByteArrayElements(env, input, bytes, 2/*JNI_ABORT*/);

  if (JVM_EvoForeignAgentPresent() == 1)       // из патченного HotSpot
      out32 ^= 0x3C3C3C3C_3C3C3C3C ...;        // второй уровень порчи

  jbyteArray res = (*env)->NewByteArray(env, 32);
  (*env)->SetByteArrayRegion(env, res, 0, 32, out32);
  return res;
}
```

`FUN_180001e4c(input, len, out32)`:

1. `FUN_180001f1c(&DAT_180010e90, 0x20, input, len, out32)` - **стандартный HMAC-SHA256**
   (внутри явные SHA-IV `0x6a09e667…`, копирование ключа в ipad/opad);
2. затем антиотладка, при срабатывании - `out32 ^= 0xA5…`:
   - `IsDebuggerPresent()`;
   - `CheckRemoteDebuggerPresent()`;
   - загружены модули `jdwp.dll`, `dt_socket.dll`, `dt_shmem.dll` (транспорты JDWP);
   - `JAVA_TOOL_OPTIONS` / `_JAVA_OPTIONS` содержат `jdwp`/`agentlib`.

**Ключ HMAC (32 байта, `.rdata` @ `0x180010e90`):**

```
70ea2f4857869bdc8606a29a6c8cdcedbe92c18bef1aad106024db104a8f25d1
```

Ключи `evoguard.dll` и `java.dll` **разные** - это два независимых шейра.

Константы-маски: `0xA5` - антиотладка (натив), `0x3C` - «чужой агент» из HotSpot,
`0x5A` - Java-уровень (см. §4).

---

## 3. jvm.dll - экспорт `JVM_EvoForeignAgentPresent`

`mod/evojdk_x/bin/server/jvm.dll` (15,4 МБ) - тот же HotSpot, что и в `java.dll`, но с
добавленным экспортом (в таблице экспортов: `[4281] +base[4282] 10b9 JVM_EvoForeignAgentPresent`).
`java.dll` импортирует его и вызывает в `computeRaw`.

Адрес: `0x1803a2340`. Декомпиляция: `reverse/out/jvm_focus.c` (функция + доступы к полю `0x53`),
`reverse/out/jvm_more.c` (итератор), `reverse/out/jvm_list.c` (источник списка),
`reverse/out/jvm_agent.c` (конструктор элемента).

### Что делает

```c
undefined1 JVM_EvoForeignAgentPresent(void) {
  int instrument_cnt = 0;   // iVar2
  int native_cnt     = 0;   // iVar5
  build_iterator(&it, &DAT_180e2c160, mode=3);   // копия глобального списка с фильтром
  while (hasNext(&it)) {
    void* e = pop(&it);
    if (byte_at(e, 0x53)) instrument_cnt++; else native_cnt++;
  }
  destroy_iterator(&it);
  if (instrument_cnt < 2 && native_cnt == 0) return 0;
  return 1;
}
```

### Откуда берётся список и что такое `+0x53`

- `DAT_180e2c160` - **глава глобального связного списка JVMTI-агентов** (stock-механизм HotSpot,
  `+0x20` = `next`, `+0x58` размер элемента). Доказательство: рядом лежит функция с литералом
  `"Disabled all JVMTI agents during -XX:AOTMode=create"`, которая делает `DAT_180e2c160 = 0`;
  push-функция `FUN_1805f40e0(name, path, is_absolute)` аллоцирует `0x58` байт, инициализирует
  и lock-free пушит в голову; `FUN_1805f44b0` выдаёт
  `"Dynamic agent loading is only permitted in the live phase"`.
- Конструктор элемента `FUN_1805f30d0` (см. `jvm_agent.c`):

```c
*(bool *)(obj + 0x53) = (strcmp(name, "instrument") == 0);   // «это javaagent/instrument»
*(obj + 0x54) = param_5;      // is_dynamic
*(obj + 0x55) = 0;            // фильтр итератора (mode 3 берёт только == 0)
```

- Фильтр итератора (`FUN_1805f3880`) = `byte[e + 0x55] == 0`, `next` = `*(e + 0x20)`.

### Итоговая семантика

`JVM_EvoForeignAgentPresent()` возвращает `1` («чужой агент присутствует»), если в
JVMTI-списке есть **хотя бы один не-instrument агент** (`-agentlib:`/`-agentpath:`, в т.ч.
`-agentlib:jdwp`), **либо два и более instrument-агента** (второй `-javaagent:`).
Единственный свой `-javaagent:evo-agent.jar` считается нормой.

Вызов из `EvoShare.computeRaw` → `XOR 0x3C` на HMAC-шейре (см. §2, §5).

Также в `jvm_focus.c` - две функции, содержащие доступы к полю `0x53`
(`FUN_180268640`, `FUN_1804a7920`), - это чтения в фильтрах классов/загрузчика, к
логике агентов отношения не имеют. Прямой записи `mov byte [reg+0x53]` в программе нет:
поле инициализируется в конструкторе широкой записью через `strcmp`-результат.

---

## 4. `java.lang.EvoShare` (Java-сторона, живёт в патченном JDK)

```java
public static byte[] compute(byte[] input) {
    byte[] out = computeRaw(input);              // натив, §2
    if (out != null && out.length == 32 && agentMismatch())
        for (int i = 0; i < out.length; i++)
            out[i] = (byte) (out[i] ^ 90);       // 0x5A - третий уровень порчи
    return out;
}
```

`agentMismatch()` - сканирует `VM.getRuntimeArguments()`:

- аргумент `-agentpath:` или `-agentlib:` (любой нативный агент, в т.ч. `-agentlib:jdwp`) → `true`;
- `-javaagent:<jar>[=opts]`: берёт путь jar, читает файл, считает SHA-256 и сравнивает с
  `EXPECTED_AGENT_SHA256`; если не совпало → `true`;
- `Throwable` → `false` (не портит).

Константы:

```
SHARE_LENGTH           = 32
AGENT_CORRUPT_MASK     = 90            // 0x5A
EXPECTED_AGENT_SHA256  = 374ace507bd0cc2d0ce6dbbe07e50ddb3173ff819e92470271e2bf0fccff2038
```

Проверено: `sha256sum mod/evo-agent.jar` = `374ace50…` - **пиннинг на свой агент
`evo-agent.jar`**. То есть JVM должен быть запущен либо вообще без `-javaagent`,
либо именно с их агентом; чужой агент тихо ломает шейр.

Потребитель - `evo.agent.key.NativeJvmShare` (в `evo-agent.jar`):

```java
Method compute = Class.forName("java.lang.EvoShare").getMethod("compute", byte[].class);
byte[] share = (byte[]) compute.invoke(null, hwid.getBytes(UTF_8));
// ClassNotFoundException → KeyProviderException("not running on the EvoVis custom JVM")
```

То есть **payload-ключ требует их кастомного JDK** - на стоковом `java` шейр получить нельзя.

---

## 5. Сводка: три независимых уровня порчи ключа

| уровень | где | условие | порча |
|---|---|---|---|
| натив evoguard | `NativeGuard.nativeShare` | debugger / `JAVA_TOOL_OPTIONS`/`_JAVA_OPTIONS` (`jdwp`,`agentlib`) | `XOR 0xA5` |
| натив JDK | `EvoShare.computeRaw` → `FUN_180001e4c` | debugger / модули `jdwp.dll`,`dt_socket.dll`,`dt_shmem.dll` / те же env | `XOR 0xA5` |
| натив JDK | `EvoShare.computeRaw` | `JVM_EvoForeignAgentPresent() == 1`: есть `-agentlib:`/`-agentpath:` агент либо ≥2 `-javaagent:` | `XOR 0x3C` |
| Java JDK | `EvoShare.compute` | `-agentpath:`/`-agentlib:` или `-javaagent:` с чужим sha256 | `XOR 0x5A` |

Во всех случаях результат остаётся валидным `byte[32]` - ошибка не сигнализируется,
ключ просто собирается неверно (в `alm.java` дальше идёт `combine` Шамира и `alz.wipe`).

Две разные схемы HMAC (разные ключи) дают два разных шейра:

- `NativeGuard.share(hwid)` - в сборку ключа мода (`alm.java`, 4-of-4 Шамир);
- `EvoShare.compute(hwid)` - в цепочку ключа payload'а агента (`NativeJvmShare`, `EvoAgent`).

---

## 6. `evovis-audiotap` (macOS, Swift)

`mod/jar/assets/evolution/native/macos/evovis-audiotap` - **Mach-O universal (x86_64 + arm64),
174 848 байт, исполняемый файл** (`_main` + `__mh_execute_header`, не dylib).

Факты по `strings` / `r2`:

- написан на **Swift** (секции `__swift5_typeref/capture/entry/reflstr/fieldmd/types`),
  класс `main.AudioTap` (`_TtC4main8AudioTap`);
- зависимости: **ScreenCaptureKit**, CoreMedia, AVFoundation, CoreAudio (swiftCoreAudio),
  Foundation, IOKit - то есть захват системного звука/видеопотока через ScreenCaptureKit;
- делегат `stream(_:didOutputSampleBuffer:of:)` (`SCStream` + `CMSampleBuffer`) и
  `stream(_:didStopWithError:)`; поля `output: NSFileHandle`, `mono: [Float]`;
- логи/метки `evovis.audiotap.audio`, `evovis.audiotap.screen`;
- **сетевых строк нет** (ни `http`, ни сокетов, ни доменов) - данные пишутся в
  `NSFileHandle` (pipe/stdout), то есть процесс-потребитель мод запускает локально.

Вывод: это локальный helper для тапа аудио (и экрана) на macOS, результат уходит пайпом
в родительский процесс; в аудио/сеть наружу ничего не отдаётся.

Декомпиляция Ghidra для него не запускалась (предмет - Swift + macOS-фреймворки, на
уровне строк/символов картина уже ясна); запускается по необходимости:

```bash
/opt/ghidra/support/analyzeHeadless "$PWD/proj" EvoTap \
  -import "$PWD/../mod/jar/assets/evolution/native/macos/evovis-audiotap" \
  -postScript DumpDecomp.java "$PWD/out/audiotap.c" -scriptPath "$PWD"
```
