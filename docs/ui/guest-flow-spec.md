# Флоу гостя — спецификация по макету

Выгрузка из `reference/dropit-design.pen`. Размеры в логических пикселях,
цвета — токенами (значения в `apps/mobile-guest/lib/theme/tokens.dart`).
Экраны 390×844, боковые отступы 20, верхний 24.

## Общие блоки

**Шапка с брендом** (`BrandBar`): знак «DROP IT» (Inter Bold 18 + наушники
15×15, промежуток 7), справа город: иконка 15 акцентом, название Inter 13,
шеврон 14. Высота 36.

**Строка комнаты** (`JoinedRoomBar`): фото 40 r6, название Inter 16 SemiBold,
подпись Inter 11 muted, галочка 20 акцентом; отступы 16, снизу разделитель 1 px
`#FFFFFF20`.

**Шапка внутреннего экрана** (`NavRow`): стрелка 22 + название Inter 15
SemiBold, промежуток 12.

**Кнопка** (`AppButton`): высота 54, радиус 27, заливка `$drop-accent`, текст
Inter 16 SemiBold `#101813`, справа стрелка 20. Состояния: pressed `#00CC6A`,
disabled `#1B211D`, loading «Processing…» + спиннер 18.

**Закреплённый низ**: линия сверху `#FFFFFF20`, отступы 16–20.

## Guest · 01 · Home

- Бренд-строка (город Berlin) — 24 от верха.
- Герой-блок 184, радиус 20: фон-изображение + вертикальный оверлей
  `#090909D9 → #09090905`; бейдж «12 ROOMS LIVE TONIGHT» (Inter 9 Bold,
  разрядка 1, точка 6 акцентом); заголовок Anton 34/1.08 «Your track.\nTheir
  next move.»; подпись Inter 11 `#DEE5E1`.
- Плитки действий 136: слева «Nearby rooms» (изображение + градиент
  `#10101600 → #101016E6`, внешние углы 22 слева), справа «Scan to enter»
  (заливка акцентом, внешние углы 22 справа, бейдж 48 r16 с иконкой QR 28).
  Заголовки Inter 16 SemiBold, подписи Inter 10.
- Лейбл «YOUR NIGHT STARTS HERE» (Inter 11 SemiBold, разрядка 1).
- Строка заведения 72: фото 40 r6, название Inter 14 Medium, «House · 200 m
  away» Inter 12, «Join» Inter 14 SemiBold акцентом + шеврон 18.
- «Set the mood» (Inter 18 Bold) + «Browse all» (Inter 11 SemiBold акцентом,
  стрелка 14).
- Две рекомендации: обложка 50 r8, название Inter 14 SemiBold, «исполнитель ·
  жанр» Inter 10 muted, круглая кнопка «+» 34 акцентом.
- Подсказка: иконка 12 + Inter 10 muted «Join a room to send your pick to the DJ.»
- Таб-бар 64, радиус 22, отступы 20/12: иконки 21 + подписи Inter 11; активный
  «Listen» — акцент + `#F0F4F1`, остальные `#929A95`.

## Guest · 02 · Scan QR

- Фон-«камера» + затемнение по краям; рамка сканирования 310×300 (радиус 24,
  рамка акцентом 3).
- Шапка: знак + круглая кнопка закрытия 44 (фон `#12121280`, рамка
  `#FFFFFF26`).
- Заголовок Anton 34 «Scan in. Tune in.»; подпись Inter 13 `#D8E3DC`.
- Низ: «Scanning automatically» Inter 14 Medium; «Just hold steady. No photo
  needed.» (иконка 13 + Inter 12 `#BAC5BF`).
- Кнопки-таблетки 52 (`#161C19CC`, рамка `#FFFFFF30`): «Flash», «From photos».
- «Can’t scan the code?» Inter 12 + кнопка 52 с рамкой акцентом «Enter room
  code» (фон `#1B3025`).
- Шторка 04c: ручка 36×4, заголовок Anton 28 «Enter room code», пояснение
  Inter 12/1.5, поле 48 r10 (`#252826`), кнопка «Join room».

## Guest · 03 · Search

- Бренд-строка + строка комнаты.
- Заголовок Anton 34 «Choose your track.» + Inter 12 «Preview a track. Select
  your next drop.».
- Поле поиска 54 r12: иконка 20, текст Inter 13, очистка 18.
- Фильтры-таблетки 36: выбранный — заливка `$drop-text`, текст цветом фона;
  невыбранные — `$drop-surface` с текстом `#CED4D0`; отступы 12.
- Лейбл «3 TRACKS · SELECT ONE» (Inter 10 Bold, разрядка 1).
- Строка результата: обложка 50 r8, название Inter 14 SemiBold, мета Inter 10,
  контрол выбора 44 r12 (выбран: рамка `$drop-text` + мятный круг 24 с
  галочкой 15; не выбран: круг с рамкой `#758079`); у выбранной строки —
  «Playing preview» (иконка 15 акцентом + Inter 10 Medium) и тайм-код
  «0:09 / 0:30» (Geist Mono 10), полоса 4 r2 `#343B36` с участком акцентом.
- Закреплённый низ: «SELECTED TRACK», строка выбранного трека, цена сессии
  Inter 13 muted, кнопка «Continue to payment».

## Guest · 04a · Terms before payment

- `NavRow` «Before you pay», заголовок Anton 34 «Know the rules.».
- Строка трека: обложка 50, название Inter 14 SemiBold, «исполнитель ·
  Original mix» Inter 11 muted, цена Inter 12 Bold акцентом.
- Блок правил: линии сверху и снизу `#FFFFFF20`, вертикальные отступы 20,
  промежуток 20; каждое правило — иконка 18 акцентом + Inter 14/1.5, отступ 14.
- Кнопка «I understand — Continue» + ссылка «Read the full terms» (Inter 12
  SemiBold акцентом).

## Guest · 04 · Payment (+ 04b, 04d)

- `NavRow` «Payment», строка комнаты, заголовок Anton 34 «Make it yours.».
- Строка заказа: обложка 50, название, «исполнитель · Original mix», круглая
  «+» 34.
- Сводка: линии сверху и снизу, отступы 20, промежуток 18; «Track request» и
  «Service fee» — Inter 13 muted, «Total» — Inter 18 Bold.
- «PAY WITH» (Inter 10 Bold, разрядка 1.4); выбранная карта — рамка акцентом,
  радиус 16, отступы 16, бейдж 40×28, название Inter 13, галочка 20 акцентом;
  ниже «Change payment method» Inter 12 SemiBold акцентом.
- Условия: Inter 12/1.5 muted, две строки.
- Кнопка с ценой + «Secure payment · No hidden fees» (Inter 11 muted).
- Шторка 04b «Pay your way.»: строки способов оплаты (фон `#252826`, выбранная
  `#00FF870D` с рамкой акцентом, радиус 12, отступы 14) + «Add a new card».
- Шторка 04d «Add a card.»: поля 48 r10 «Card number», «Expiry date», «Security
  code», «Name on card»; «Save card for next time»; кнопка «Save & use card»;
  примечание про шифрование.

## Guest · 05 · Request sent

- Бренд-строка, строка комнаты.
- Праздничный блок 350×128: свечение 228×128 (радиальный градиент
  `#00FF8738 → прозрачный`), рябь 124 и 102 (рамки `#00FF8720` / `#00FF8748`),
  бейдж 80 акцентом с галочкой 38 `#101813`, 8 конфетти (4×10 … 3×5, углы
  от −25° до 65°, цвета акцент / `#BC9AFF` / `#E6FFF1`), 3 искры 18/14/14.
- Заголовок Anton 34 «Your pick is in.» + пояснение по центру Inter 13/1.5.
- Чек: фон `$drop-surface`, рамка `$drop-border`, отступы 16, промежуток 20;
  строка трека с «+» 34; строки «Status» (значение акцентом SemiBold),
  «Payment», «Request ID» (Geist Mono).
- Кнопка «View my request» + «Find another track» (Inter 13 SemiBold).

## Состояния

Из DS · 02 «Screen states»: скелетон (строки 44 r8 `$dj-surface-2`, линии
10/8), ошибка (фон `#FF3B3014`, рамка `#FF3B30`, иконка 24, заголовок Inter 14
Bold, пояснение Inter 12), офлайн (фон `$drop-bg`, рамка `$drop-line`).
В приложении гостя ошибки показаны на экране скана и на оплате; скелетоны —
в «My requests».
