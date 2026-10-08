# Вырезка бэкенда, авторизации и телеметрии (2026-10-02)

Локальный патч **их** клиента (`mod/evovis-dec.jar`) по решению владельца: без их сервера,
без их аккаунта, без отправки данных. Ничего из этого не трогает LoVisual.

## Артефакт

| Файл | Размер | Что это |
|---|---|---|
| `mod/evovis-dec-nobackend.jar` | 47 481 810 B, sha256 `8c1f262687c056b2…` | `evovis-dec.jar` +3 перепакованных класса |

Исходники патча: `patch/src/` (копии из `mod/jar-src/sources/defpackage/`), смоук: `patch/smoke/`.

## Метод

Их классы лежат **в корне джара** (default package; jadx называет его `defpackage`).
Классы v69 = Java 25 → компилировали `/usr/lib/jvm/java-25-openjdk/bin/javac` против
`evovis-dec.jar` + `~/.gradle/caches/fabric-loom/26.2/minecraft-merged.jar` (mojmap) + gson,
затем `jar uf` в корень копии джара.

Важные артефакты jadx, которые пришлось учесть:

- **jadx переименовал их класс `if` → `Cif`** (keyword): класс `Cif.class` в джаре не существует,
  поэтому весь мёртвый код с ссылками на `Cif` вырезан, а не пересобран.
- `?? r1;` (недообфусцированный фрагмент) и `throw th;` вне catch — в вырезанных методах.
- Обфускация `setDaemon((A^B)+((A&B)<<1))` — это int-эквивалент `true` (A+B=1), javac не берёт.

## Что заменено

### `cp` = `Avatar2` (аккаунт)
- `initialize()` → `setUser(new cr("Player", null, "user", 0, null))`:
  **нет** `POST /api/auth/me`, нет чтения `EVO_TOKEN`, нет лаунчер-сессии, нет `refreshMe` (HTTP).
  Профиль — локальный dummy.
- `isAuthorized()` → `return true` — открываются все их шлюзы
  (`Panel`, `Camera2`, `First`, `Friends`, `Crosshair2`, `KeyAssembler`, `SessionHeartbeat`).
- `user()` → lazy-dummy: если профиль не задан, создаёт тот же dummy.
  **`token == null`** — принципиально: `KeyAssembler.token()` и `SessionHeartbeat.token()`
  возвращают null → `AttestClient4` (`evovis-protect.vercel.app`: challenge/attest/heartbeat)
  **недостижим**, поток сборки Shamir-ключа крутится вхолостую (sleep 500 мс).
- Удалён приватный `refreshMe()` (единственный источник HTTP на auth/me).

### `akj` = `Report` (телеметрия)
- `submit(String)` → no-op (единственный внешний вызов — `dn`/`Message2`, их команда отчёта).
- Удалены `feedback`, `buildBody`, `run` (достигались только из `submit`).

### `alr` = `SessionHeartbeat` (сессия/отключение модулей)
- `initialize()` → no-op: поток `evo-session-heartbeat` не стартует → ни `ACTIVE`/`REVOKED`
  каждые 60 с, ни `REVOKED`×2 → «выключить все модули».
- Удалены приватные `run/tick/suspend/restore/reassert/showNotification/manager/token/sleepQuietly`.

## Почему остальное работает без их сервера

- **Обычные строки мода** — `awc.d(String,int)`: статический LCG+XOR, ключа не требует.
- **`SecureStrings`** (5 API-блобов: probe/friends/messages/cosmetic_sync/config_share) —
  `KeyAssembler.key()` всегда null → `get()` возвращает null → их серверные фичи
  (синк друзей, сообщения, косметика, конфиги) **выключаются сами**, без крашей на старте
  (null проверяется в самом `SecureStrings.get`).
- `KeyAssembler`/`SealedBoot`/`RuntimeGuard`/JNI `NativeGuard` — живут только после сборки
  ключа, то есть недостижимы при `token == null`.

## Оставлено (осознанно)

| Что | Почему |
|---|---|
| `Ticket` (тикеты поддержки) | только по явному клику пользователя, с `token==null` отправлять нечего |
| `Evovisuals2` (аватарки `head/*.png`, mc-heads) | косметика, грейсфейл, неактивно без друзей/аккаунта |
| `Route` (зеркала `delivery*.evoproject.xyz`) | бесполезен: запросов к их хостам больше нет |
| Сторонние API (lrclib, mc-heads, yt-dlp, whisper) | их бэкенд, не трогаем |

## Проверки

- `javap -c`: `cp.isAuthorized` → `iconst_1/ireturn`; `cp.initialize` → `new cr(…Player…)`;
  `akj.submit` → `return`; `alr.initialize` → `return`.
- Смоук (`patch/smoke/Smoke.java`, JVM 25, classpath = патченный джар + MC + gson):

```
isAuthorized = true
user = Player token=null
isSuspended = false
OK: auth=local, report=no-op, heartbeat=off
```

## Ограничение

В игре не прогоняли: здесь Linux, а их `evojdk` только win-x64. Запускать патченный джар —
на Windows (их лаунчер или свой fabric-лодер); при проблемах — смотреть лог на загрузке
модулей. Ожидаемо «пустым» (серверное) останется: список друзей, сообщения, косметика синк,
аватарки, тикеты.
