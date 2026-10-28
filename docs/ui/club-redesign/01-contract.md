# G1 — контракт дизайна v2: решения лида

> Источник: `docs/ui/club-panel-redesign-prompt.md` §4 (далее «§4»).
> Этот файл фиксирует решения, которые обязан соблюдать System owner (G2) и
> Implementer board (G3). При расхождении §4 и этого файла — приоритет у §4,
> расхождение сообщается лиду.

## 1. Шкала токенов (создаёт G2 в `src/styles/club-tokens.css`)

### Радиусы — ровно 4 значения на всю панель

| Токен | Значение | Где |
|---|---|---|
| `--r-surface` | `16px` | панели, диалоги, крупные контейнеры |
| `--r-tile` | `12px` | вложенные плитки, инпут-группы, QR-рамка |
| `--r-control` | `10px` | кнопки, поля, сегмент-контейнер, иконочные кнопки |
| `--r-chip` | `999px` | только `Chip` и бегунок `Segmented` |

Запрещено: 24/28/32px, а также 18/9/8/5/6/50% на доске. Круг допустим только
у `avatar` / `chart-point` (это не «поверхность», это маркер) — в DOM-метрике
§7.4 они не считаются радиусом поверхности, но в отчёте указываются отдельно.
Статусных точек больше нет: состояние показывает чип со словом (`live-dot`
удалён 2026-10-28 вместе с `chip-dot`, `dj-status-dot` и `dj-presence-dot`).

### Поверхности

| Токен | Значение |
|---|---|
| `--c-canvas` | `#090A0F` |
| `--c-surface` | `#11131A` |
| `--c-raised` | `#171A22` |
| `--c-inset` | `#0D0F15` |
| `--c-hairline` | `rgba(235,240,250,.08)` — только для `input`/`select`/`textarea` и разделителей строк |

### Цвет

| Роль | Значение |
|---|---|
| `--c-accent` | `#00FF87` |
| `--c-accent-hover` | `#00D873` |
| `--c-accent-tint` | `rgba(0,255,135,.08)` |
| `--c-accent-tint-strong` | `rgba(0,255,135,.14)` |
| `--c-ink-on-accent` | `#06130C` |
| `--c-text` | `#F1F4F7` |
| `--c-text-2` | `#B0B8C3` |
| `--c-text-muted` | `#858E9C` (только ≥12px) |
| `--c-text-muted-sm` | `#9AA3B0` (для 11–12px) |
| `--c-warn` | `#FFC65B` (тинт 10%, без рамки) |
| `--c-error` | `#FF6B6B` (тинт 10%, без рамки) |

Удаляются полностью: `#10B981`, `#22C55E`, `#7ee2a8`, `#8ed3f4`, `#f2d089`,
`#666666`, `#00e887`, `#70FFB9` (последние два — из `--ops-*` и Tailwind).

### Типографика

- Семейство: **Inter** — единственное интерфейсное. Загружается **локально**
  (`public/fonts/inter-latin-var.woff2`, вариативный, один файл, self-host).
- Моно: `ui-monospace, SFMono-Regular, Menlo, monospace` — только ID, время,
  деньги, `font-variant-numeric: tabular-nums`.
- Шкала — ровно 6 значений: `28 / 20 / 15 / 13 / 12 / 11`.
  Заголовок страницы = 28, заголовок панели = 15/600, тело = 13, подпись = 12,
  метка = 12, микростатус = 11. **Ничего < 11px.**
- `Geist` из CSS удаляется (шрифт не загружается и не будет).

### Движение

| Токен | Значение |
|---|---|
| `--dur-fast` | `120ms` |
| `--dur-base` | `180ms` |
| `--dur-slow` | `260ms` |
| `--ease-out` | `cubic-bezier(.22,1,.36,1)` |

Разрешено: hover-фон + `translateY(-1px)`, press `scale(.99)`, бегунок
`Segmented`, вход/выход тоста, кросс-фейд значения метрики, `auto-animate` для
списков, отрисовка линии графика по `stroke-dashoffset`. Всё — не дольше
260ms, только `transform`/`opacity`/цвет. `prefers-reduced-motion: reduce`
обнуляет всё.

## 2. Контракт разметки (обязателен для G2, используется автотестом §7.2)

| Атрибут | На чём | Значение |
|---|---|---|
| `data-surface` | каждая панель и каждая вложенная плитка | `panel` \| `tile` \| `dialog` |
| `data-control` | кнопки, поля, сегменты, иконочные кнопки, чипы-кнопки | `button` \| `field` \| `segment` \| `icon` |
| `data-metric` | контейнер каждой метрики | — |
| `data-source` | на `[data-metric]` | `live` \| `preview` \| `unavailable` |
| `data-preview-banner` | общий баннер предпросмотра | — |
| `data-abstract` | фоновый абстрактный слой | — |

Примитивы G2 обязаны сами проставлять эти атрибуты, чтобы страницы их не
дублировали руками.

## 3. Примитивы (`src/components/club/`)

`Panel`, `Metric` (метка + значение + чип источника), `Tile`, `Chip`
(`tone`: `live`/`warn`/`error`/`neutral`), `Segmented` (radiogroup + стрелки),
`ListRow` (≥44px, разделитель 1px ≤6%), `Meter`, `Toast`, `Skeleton`,
`EmptyState`, `ErrorState`, `SourceChip` (`live`/`preview`/`unavailable`).

`Toast`: ≥8000ms, пауза при hover/focus, `role="status"` **один** (без
`aria-live` на контейнере сверху), кнопка закрытия ≥36px, «Отменить» — только
там, где откат реально есть.

## 4. Состав доски Overview (решение лида)

Бюджет: **7 плиток**, три полосы, без заголовков-«eyebrow» между полосами.
Формы правил/музыки и таблиц «Most active tables» на Overview **нет** (§4.2).

| # | Плитка | Спан | Полоса | Данные | Источник |
|---|---|---|---|---|---|
| 1 | Room now (режим/смена/вместимость/поток) | 8 | 1 | `getClubDashboard().session` + `getProfile()` | **live**; occupancy-часть — `preview` |
| 2 | On the decks (кто играет / следующий) | 4 | 1 | `getDjs()` | **live** |
| 3 | Money (net, запросы, чаевые) | 4 | 2 | `getClubDashboard()` (net, requests) + `getEarnings('today')` | **live** |
| 4 | Guest requests (запросы за 24 ч, средний чек, цена) | 4 | 2 | `getClubDashboard()` + `getPricing()` | **live** |
| 5 | Floor zones (занятость, QR-статус) | 4 | 2 | локально | **preview** |
| 6 | Door & QR (реальный QR сессии) | 4 | 3 | `generateSessionQr()` если `session.id`, иначе CTA на /sessions | **live** / `unavailable` |
| 7 | Next actions (что сделать дальше) | 8 | 3 | производное от 1–6 | **live** |

Правило: `data-metric` на каждое число; ни одного `Math.random()`; «Snapshot Ns
ago» — от `lastFetchedAt` из `useAsyncData`, не тикер.

## 5. Ширина

Одно ограничение: **1280px** на `.club-workspace-content` (уже есть),
`club-dashboard-shell` наследует и не задаёт своё. 1320/1360 удаляются.

## 6. Что удаляется из кода (чек-лист G2/G3)

- `useVenueState` как владелец доски: правила, события, столы, зоны-мутации,
  тосты «сохранено» — удалить; остаётся только то, что помечено `preview`.
- `CrowdChart` на случайных данных, `EnergyGauge` на производных от него же —
  удалить (§5.1.4). Абстракция «дуга занятости» строится из реальной
  occupancy-модели (preview) либо отсутствует вовсе.
- `QRBlock` с самодельным `dropit.app/room/...` — удалить, заменить на статус +
  ссылку на `/sessions`.
- iconify CDN: `Icon.tsx` переписать на `lucide-react` (уже в зависимостях),
  `<script>` из `index.html` удалить.
- Google Fonts: `index.html` — убрать `preconnect`/`stylesheet`, шрифт локально.
- `Geist` из `index.css`, `operations.css`, `tailwind.config.js`.
- 24/28/32px, `#10B981`, `#22C55E`, `#00e887`, `#70FFB9`, `!important` в новых
  правилах.

## 7. Контракт данных: `useClubBoard()` (G5 поставляет, G3 потребляет)

Файл: `apps/web-club/src/dashboard/useClubBoard.ts`. Форма зафиксирована лидом —
G3 пишет страницу против неё, G5 реализует. Менять форму — только через лида.

```ts
export type MetricSource = 'live' | 'preview' | 'unavailable';

export interface BoardMetric<T> {
  value: T | null;          // null только когда source === 'unavailable'
  source: MetricSource;
  /** Фактический период/фильтр для честной подписи, напр. "all time, settled". */
  caption: string;
}

export interface BoardSession {
  id: string;
  status: 'created' | 'active' | 'paused_acceptance' | 'closed';
}

export interface BoardDj {
  id: string;
  displayName: string;
  email: string;
  status: 'invited' | 'active' | 'suspended' | 'blocked';
  /** false, если клубный профиль/смена не загрузились */
  isOnShift: boolean;
}

export interface BoardZone {
  id: string;
  name: string;
  capacity: number;
  inside: number;
  qrActive: boolean;
}

export interface ClubBoard {
  clubName: string;
  clubNameSource: MetricSource;

  session: BoardSession | null;
  sessionSource: MetricSource;
  /** ms epoch последнего УСПЕШНОГО запроса; null если ни одного */
  lastFetchedAt: number | null;
  refreshing: boolean;
  /** одна честная ошибка на доску; null когда всё ок */
  error: { message: string; code?: string } | null;
  reload: () => void;

  netSettledMinor: BoardMetric<number>;
  requests24h: BoardMetric<number>;
  requestPriceMinor: BoardMetric<number>;
  djs: BoardDj[];
  djsSource: MetricSource;

  zones: BoardZone[];                    // всегда preview (нет endpoint)
  zonesSource: MetricSource;

  /** реальный токен сессии; null если смены нет или ошибка */
  sessionQr: { token: string; expiresAt: string } | null;
  sessionQrSource: MetricSource;
  rotateSessionQr: () => Promise<void>;
  qrBusy: boolean;

  /** сколько preview-метрик на доске; >0 => показывать data-preview-banner */
  previewCount: number;
}

export function useClubBoard(): ClubBoard;
```

Правила реализации:
- Никакого `Math.random()`, никаких тикеров «snapshot». `lastFetchedAt` ставится
  только после успешного `await`; подпись — «updated Ns ago» считается от него и
  обновляется раз в 5 с (не чаще).
- При ошибке: `error` заполнен, `source` у API-метрик → `unavailable`, `value: null`.
  Данные прошлого успешного запроса не подменяют честную метку: если запрос упал,
  метрика помечается `unavailable` (stale-показ запрещён §7.1).
- Чаевые/средний чек/толпа/«floor energy» — удаляются совсем: endpoint'ов нет,
  §5.1.4/§5.1.5 запрещают выдавать их за телеметрию.
- Подписи: `netSettledMinor.caption = 'all time, settled'`,
  `requests24h.caption = 'last 24 h'`, `requestPriceMinor.caption` = текущая
  цена из `getPricing()` (политика с `validTo === null`).
- Зоны — единственный `preview`-блок; он остаётся, потому что endpoint'а нет,
  и помечается `zonesSource = 'preview'`.

## 8. Приёмка в demo-режиме (важно для G6)

`npm run dev:demo` не имеет бэкенда (`localhost:8081` не поднят), поэтому
API-метрики честно показывают `unavailable`. Это **правильное** поведение по
§5.1/§7.1 и не дефект.

Поэтому скриншоты «после» снимаются в двух режимах:
1. `honest` — без бэкенда: один баннер, метрики `unavailable`, доска не врёт.
2. `populated` — Playwright перехватывает `**/v1/club/**` и отдаёт фикстуры
   харнеса (это тестовая обвязка, а не продуктовая демо-подмена). В отчёте прямо
   писать «populated = route fixtures в тесте».

Оба режима обязаны проходить числовые пороги §7.2 (радиусы, рамки, высота,
шрифты, цели нажатия, контраст). Высоту мерить в режиме `populated`, потому что
он выше.

## 9. Решения лида по отчёту G0 (00-baseline.md)

Аудитор измерил заново на замороженной копии (`:5199`) и уточнил §3/§7.4.
Принято:

1. **Базовые числа для таблицы «до»** — из `00-baseline.md`
   (`baseline.json` аудитора авторитетен; мой первый проход сохранён как
   `baseline-lead-pass1.json`). Уточнения, которые входят в отчёт:
   рамок **81** (65 вложенных), а не 66/58; размеров шрифта **13**, а не 12;
   радиусов 8 на десктопе и **9 на мобиле**; контраст-минимум по доске
   **5.61:1** (проходит), по странице **3.90:1** (не проходит), и `#666666` в
   отрендеренном DOM **не встречается** — строка §7.4 про 3.06:1 была про
   токен, а не про пиксели. В отчёте это указывается прямо.

2. **Две строки §7.4 уже зелёные ДО работы** — horizontal overflow (0 на всех
   ширинах) и `prefers-reduced-motion` (0 анимаций). Verifier не имеет права
   засчитывать их как «стало лучше»; он указывает «уже было 0».

3. **§7.2 #14 («нет console.error») и demo-режим.** Проверка выполняется
   **только в режиме `populated`** (route fixtures на `**/v1/club/**`), потому
   что в honest-режиме отсутствие бэкенда законно даёт 500 и `console.error`, а
   §5.1 требует показывать это честно. Это не поблажка: в `populated`-режиме
   ошибок быть не должно, и именно это проверяется. Оба режима снимаются в
   отчёте. Двойной вызов `/v1/club/dashboard` из-за StrictMode считается
   отдельно: в `populated` он тоже даёт два запроса, и это допустимо (React dev
   behaviour), но `console.error` он давать не должен.

4. **axe-нарушения живут не на доске, а в оболочке.** Аудитор: по доске
   контраст-минимум 5.61:1, а 7 из 10 серьёзных узлов — в `Layout.tsx`
   (контраст рельса, `definition-list`, `label-content-name-mismatch`). Значит
   ответственность за закрытие axe — на System owner (G2), а не на доске.
   Shell обязан быть вычищен по axe до того, как Verifier начнёт.

5. **Мобильные радиусы.** Сейчас `.club-dashboard-panel-full` держит 28px
   (`operations.css:741`) при 24px в медиа-запросе (`:602`) → автотест прошёл бы
   на десктопе и упал на мобиле. G2 обязан проверить мобилу отдельно.

6. **`/dj` и `/connect` не перекрашиваются.** Они делят `index.css`/
   `operations.css`, но их внешний вид менять нельзя. `--ops-*` можно
   перенаправлять на новые токены только если DJ-консоль после этого визуально
   не меняется (кроме приведения к одному зелёному). Проверка — скриншот /dj до
   и после.

7. **Удалить мёртвые контролы**: `toggleTableQr` и `setTableStatus` в
   `useVenueState` не имеют ни одного вызова. Уходят вместе с остальным моком.

8. **Тост и палитра** (аудит): тост умирает на 3410 мс при hover, кнопка
   закрытия 18×28, фокус после закрытия падает на `<body>`, 4 из 10 Tab уходят
   из палитры, модалка не блокирует скролл. Всё это — требования §5.3 и §7.1,
   закрывает G2 в примитивах и в `Layout.tsx`.
