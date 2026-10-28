# DROP IT — приложение гостя: экраны и кнопки

Перенос 1:1 из макета pen.dev (`Guest · 01…15`). Для каждого экрана —
описание внешнего вида и назначения каждой интерактивной кнопки.
Реализация — `apps/mobile-guest/lib`.

Легенда размеров: все значения в логических пикселях, экран 390×844.

---

## Глобальная навигация

### Таб-бар (внизу каждого корневого экрана)
- **Внешний вид:** плавающая панель 366×64, радиус 22, фон `#0A0C0B`,
  тонкая обводка `#17FFFFFF`; 12 px от левого/правого края и 12 px от низа.
  Четыре таба: иконка 21 + подпись Inter 12.
  Активный: иконка `#00FF87`, подпись `#F0F4F1` (w600);
  неактивный: иконка `#929A95`, подпись `#A5AEA9` (w500).
- **Что делает:**
  - **Listen** (наушники) — экран Home (01).
  - **Explore** (компас) — экран Explore (11).
  - **Requests** (список) — My requests (06).
  - **Profile** (человек) — Profile (07) или Profile · Guest (07b).

### Шапка «бренд + город» (Home, Explore, Requests, Profile)
- **DROP IT + наушники** — фирменный лук-ап (шейдерная волна `#00FF87→#66FFC2`).
  Не кнопка.
- **Город (Berlin ▾)** — пилюля: пин `#00FF87` 15 + «Berlin» Inter 13 + шеврон 14.
  Кнопка выбора города; в текущей сборке открывает заглушку выбора (демо).

### Шапка «назад + заголовок» (Search, Payment, Terms, Request detail, Help)
- **Стрелка назад** 22 px `#F2F5F3` — возврат на предыдущий экран.
- **Заголовок** Inter 15/600 — название экрана.

---

## Guest · 01 · Home (Listen)

1. **Hero-фотография** 366×184, радиус 8 — клубный свет; поверх — градиент
   `#090909D9→#09090905`. Заголовок Anton 34 «Your track. / Their next move.»,
   подпись Inter 12 «Join a club. Request a track. Own the night.».
   Внизу — подпись **«12 ROOMS LIVE TONIGHT»** Inter 12/700.
2. **Плитка «Nearby rooms»** 183×136 (левая, скругление 28 слева):
   карта, градиент снизу, тексты «Nearby rooms» Inter 16/600 белый,
   «12 live · 0.2 km» Inter 12 белый.
   → **переключает на таб Explore.**
3. **Плитка «Scan to enter»** 183×136 (правая, скругление 28 справа):
   фон `#00FF87`, заголовок Inter 16/600 `#101813`, подпись Inter 12 `#17452F`,
   QR-бейдж 48×48 `#101813` с иконкой QR 28 `#00FF87`.
   → **открывает экран Scan QR (02).**
4. **Строка клуба «Kater Blau»**: фото 40×40 r6, «Kater Blau» Inter 14/500,
   «House · 200 m away» Inter 12, справа **«Join»** Inter 14/600 `#00FF87` + шеврон.
   → **Join:** присоединяет к комнате и открывает Search (03).
5. **«Set the mood»** Inter 18/700 + **«Browse all →»** Inter 12/600 `#00FF87`.
   → **Browse all:** открывает Scan (если не в комнате) — тот же флоу, что плитка QR.
6. **Строки рекомендаций** (Move / One More Time): обложка 50×50 r6,
   название Inter 14/600, «Adam Port, Stryv · Afro house» Inter 12, справа
   **квадратная кнопка «+» 34×34** (r6, фон `#00FF87`, «+» 18 `#101813`).
   → **«+»:** если комната выбрана — выбирает трек и открывает Terms (04a);
   если нет — подсказка «Join a room first» и переход в Scan (02).
7. **Подсказка** «Join a room to send your pick to the DJ.» + иконка ⓘ 12.

---

## Guest · 02 · Scan QR

1. **Полноэкранная камера** (снимок сцены) + затемнение градиентом,
   рамка сканера 310×300 r32 (обводка 2 px белая 70 %), бегущая линия `#00FF87`.
2. **Закрыть (×)** 44×44, фон `#121212` 50 %, иконка 20 — **возврат на Home.**
3. **Заголовок** Anton 34 «Scan in. Tune in.» + подпись Inter 13 `#D8E3DC`.
4. **Подсказка** «Scanning automatically» Inter 14/500 + строка
   «Just hold steady. No photo needed.» Inter 12 `#BAC5BF`.
5. **Flash** — круглая кнопка 52 (стекло `#FFFFFF12`), иконка 22.
   → **включает/выключает вспышку** (label «Flash» ↔ «Flash on»).
6. **From photos** — такая же круглая кнопка.
   → **распознаёт QR из галереи** (демо: мгновенно присоединяет к ближайшей
   комнате) и открывает Search (03).
7. **«Can’t scan the code?»** Inter 12 + **«Type room code manually»**
   (иконка клавиатуры 18 + Inter 13/500 `#D8E3DC`).
   → **открывает шторку ввода кода комнаты (04c).**

### Guest · 04c · Room code (шторка)
- **Внешний вид:** шторка 390×340, фон `#1C1E1D`, скругление 28 сверху,
  ручка 36×4 `#606762`, заголовок Anton 28 «Enter room code», описание Inter 12,
  поле «Room code» (h48, `#252826`, иконка # 18, плейсхолдер «Enter code»),
  кнопка **«Join room»** (h54, `#00FF87`, pill, стрелка).
- **Кнопки:**
  - **× (закрыть)** — закрывает шторку без действия.
  - **Join room** — при непустом коде **присоединяет к комнате** и открывает Search.
  - Тап по фону (scrim) — закрыть.

---

## Guest · 03 · Search

1. **Карточка комнаты**: фото 40×40 r6, «Kater Blau» Inter 16/600,
   «Main floor · DJ Markus · Live now» Inter 12, галочка verified 20 `#00FF87`.
2. **Заголовок** Anton 34 «Choose your track.» + подпись Inter 12.
3. **Поле поиска** h54, pill, фон `#141815`: лупа 20, плейсхолдер «Move» Inter 13,
   **× (очистить)** 18 при введённом тексте — **сбрасывает запрос.**
   Ввод — **фильтрует треки** по названию/исполнителю в реальном времени.
4. **Чипы жанров** (All · House · Afro house), r999, Inter 12/600:
   - активный **All** — светлый фон `#F2F5F3`, текст `#0A0C0B`;
   - активный жанр — фон `#00FF87`, текст `#101813`; неактивные — `#222623`/`#BDC5BF`.
   → **тап фильтрует список** по жанру.
5. **Счётчик** «3 TRACKS · SELECT ONE» Inter 12/700.
6. **Строка трека**: обложка 50×50 r6, название Inter 14/600,
   «Adam Port, Stryv · Afro house» Inter 12; справа **круглый выбор 44×44**:
   выбран — круг 24 `#00FF87` с галочкой 15 `#092116`; не выбран — контур 24.
   → **тап по строке/кругу выбирает трек** (подсветка карточки).
7. **Preview** (под строкой): ▶ 15 + «Preview» Inter 12/500 `#DCE3DE`.
   → **запускает/останавливает превью** (label «Playing preview», таймер
   «0:09 / 0:30» и полоса прогресса `#00FF87`).
8. **Нижний блок выбранного трека** (появляется после выбора):
   «SELECTED TRACK» Inter 12/700, обложка 50, название Inter 14/600,
   исполнитель Inter 12, цена «3,00 €» Inter 13, кнопка
   **«Continue to payment»** (h54, `#00FF87`, pill, стрелка).
   → **открывает Terms before payment (04a).**

---

## Guest · 04a · Terms before payment

1. **Стрелка назад** — возврат в Search.
2. **Заголовок** Anton 34 «Know the rules.».
3. **Карточка заказа**: обложка 50, «Move» Inter 14/600,
   «Adam Port · Original mix» Inter 12, цена «3,00 EUR» Inter 12/700 `#00FF87`.
4. **Три правила** (иконка ✓ 18 `#00FF87` + Inter 14/1.5):
   оплата 3,00 EUR; каждый запрос оплачивается отдельно; трек играет раз за ночь.
5. **«I understand — Continue»** (h54, `#00FF87`, pill, стрелка)
   → **открывает Payment (04).**
6. **«Read the full terms»** Inter 12/600 `#00FF87`
   → **открывает шторку с полными условиями** (кнопка «Close» внутри).

---

## Guest · 04 · Payment

1. **Стрелка назад** — возврат на Terms.
2. **Карточка комнаты** (как в Search).
3. **Заголовок** Anton 34 «Make it yours.».
4. **Строка заказа**: обложка 50, «Move» Inter 14/600,
   «Adam Port, Stryv · Original mix» Inter 12, справа **круглая «+» 34**
   (декоративная, `#00FF87`).
5. **Состав заказа**: «Track request — 3,00 €», «Service fee — 0,00 €»,
   разделитель, **Total — 3,00 €** Inter 18/700.
6. **«PAY WITH»** Inter 12/700 + **выбранная карта**: бейдж Visa 40×28 (белый r4,
   «VISA» `#1434CB`), «Visa ending in 4242» Inter 13, галочка 20 `#00FF87`.
7. **«Change payment method»** Inter 12/600 `#00FF87`
   → **открывает шторку выбора способа оплаты (04b).**
8. **Текст условий** «Payment sends your request to the DJ…» Inter 12.
9. **Кнопка оплаты «3,00 €»** (h54, `#00FF87`, pill, стрелка).
   → **списывает оплату** (состояние «Processing…» + спиннер), создаёт запрос
   (в бэкенд при доступности; иначе демо-локально) и открывает
   **Request sent (05).**

### Guest · 04b · Choose payment method (шторка)
- **Внешний вид:** шторка 390×510 `#1C1E1D`, ручка, заголовок Anton 28
  «Pay your way.», × справа; строки способов (бейдж 40×28 + название Inter 13/600
  + подпись Inter 12 + галочка 20); активная строка — фон `#00FF870D`.
  Ниже — «Add a new card» (+ 22 `#00FF87`, текст Inter 13/600 `#00FF87`, шеврон),
  кнопка **«Use this method»** (h54).
- **Кнопки:**
  - **Строка Visa/Apple Pay/Google Pay** — **выбирает способ оплаты**
    (галочка переезжает, кнопка оплаты сохраняет выбор).
  - **«Add a new card»** — закрывает шторку и открывает **форму карты (04d).**
  - **«Use this method»** — подтверждает выбор и закрывает шторку.
  - **× (закрыть)** — закрывает без изменений.

### Guest · 04d · Add a card (шторка)
- **Внешний вид:** шторка 390×544 `#1C1E1D`, ручка, шапка (назад ←, Anton 28
  «Add a card.», ×), поля: Card number (h48 `#252826`, плейсхолдер
  «1234 5678 9012 3456», иконка карты), Expiry (MM / YY) + CVC, Name on card
  (Full name); чекбокс «Save card for next time» (галочка `#00FF87`);
  кнопка **«Save & use card»** (h54); подпись «Your card details are encrypted…».
- **Кнопки:**
  - **← (назад)** / **×** — закрыть форму.
  - **Чекбокс «Save card»** — включить/выключить сохранение.
  - **«Save & use card»** — **сохраняет карту** («Visa ending in NNNN»),
    делает её выбранной и показывает снекбар «Card saved».

---

## Guest · 05 · Request sent

1. **Празднование**: изумрудное свечение (radial `#00FF8738→0`), два
   расходящихся круга, бейдж 80×80 `#00FF87` с галочкой 38 `#101813`,
   8 конфетти (4×10…3×5) и 3 искры (`#00FF87`, `#D8C5FF`), анимация.
2. **Заголовок** Anton 34 «Your pick is in.» + подпись Inter 13/1.5
   «Your request is with DJ Markus. / Follow its status in My requests.».
3. **Чек** (карточка `#141815` r16): обложка 50, «Move» Inter 14/600,
   «Adam Port, Stryv» Inter 12; строки Status — **«Sent to DJ»** `#00FF87`,
   Payment — «3,00 €», Request ID — «#DI-0284»; шеврон справа.
   → **тап по чеку открывает neon-квитанцию** (перфорированный чек
   с выемками, пунктиром и деталями: Requested at / Room / DJ / Payment method).
4. **«View my request»** (h54, `#00FF87`, pill, стрелка)
   → **возврат на корень и таб Requests** (список запросов).
5. **«Find another track»** (h44, прозрачная, текст Inter 13/600)
   → **возврат к Search** в той же комнате.

---

## Guest · 06 · My requests (таб Requests)

1. **Заголовок** Anton 34 «My requests» + подпись Inter 12
   «Your picks. From the queue to the dance floor.».
2. **Фильтры комнаты и даты**: пилюли «All rooms ▾» и «Tonight ▾»
   (иконка 14 + Inter 12 + шеврон).
   → **открывают шторку выбора** (список опций с галочкой);
   выбор **фильтрует список** по комнате (дата — демо-переключатель).
3. **Чипы статусов**: «All · 5», «Upcoming · 2», «Played · 2», «Declined · 1»
   (r999; активный `#00FF87`/`#101813`, неактивные `#222623`/`#BDC5BF`).
   → **тап фильтрует запросы** (Upcoming = в очереди + ожидает DJ) и меняет
   пояснение под списком.
4. **Сводка**: «5 REQUESTS» Inter 12/700 + «Newest first ↓» Inter 12.
5. **Карточки запросов** (`#141815` r18): обложка 42, название Inter 13/600,
   исполнитель Inter 12, справа статус Inter 12/600:
   - «In queue · #3» — `#00FF87`;
   - «Awaiting DJ» — `#F4CD7A`;
   - «Played · 00:24» — `#A5AEA9`;
   - «Declined» — `#FF969B`.
   Мета: «Kater Blau · 00:42» и «3,00 € · Paid/Refunded».
   У отклонённого — блок **«A note from the DJ»** (фон `#251C1E`, заголовок
   `#FFB1B5`, текст `#E3CCCF`).
   → **тап открывает Request detail (13).**
6. **Пояснение под списком** (ⓘ + Inter 12/1.5) — зависит от фильтра.

---

## Guest · 07 · Profile (авторизованный)

1. **Заголовок** Anton 34 «Your profile».
2. **Карточка статистики** (h132, r16): 18 Played · 6 Rooms visited ·
   24 Requests (значения Inter 30/600, подписи Inter 12 `#D2DAD5`).
3. **ACCOUNT**: строки h46 (иконка 18 `#A5AEA9`, название Inter 13, значение
   Inter 12, шеврон 15):
   - **Personal details** → Identity (15);
   - **Payment methods** («Visa ·· 4242») → шторка способов оплаты (04b);
   - **Notifications** («On») → демо-переключатель;
   - **Language** («English») → демо.
4. **HERE TO HELP**: **Help & support** → Help (14); **Privacy & terms** →
   диалог с текстом политики (кнопка «Close»).
5. **Sign out** (иконка выхода + Inter 13 `#A5AEA9`) — **выходит из аккаунта**
   (возврат на Profile · Guest, снекбар «Signed out»).

---

## Guest · 07b · Profile · Guest (не авторизован)

1. **Заголовок** Anton 34 «Your night. / Your profile.» + подпись Inter 12.
2. **«Sign in»** (h54, `#00FF87`, pill, стрелка) → **Sign in (08).**
3. **«Create an account»** (h52, фон `#0A0C0B`, r16, текст Inter 16/600 `#F2F5F3`,
   стрелка `#00FF87`) → **Create account (09).**
4. **Подпись** «Quick sign-in with Google or Apple available» Inter 12.
5. **Преимущества** (✓ 18 `#00FF87` + Inter 13 `#A5AEA9`): все запросы в одном
   месте, быстрый повторный чекаут, свои комнаты и музыка.

---

## Guest · 08 · Sign in

1. **← назад** + бренд-локуп.
2. **Заголовок** Anton 34 «Sign in» + описание Inter 14/1.5
   «Welcome back. Pick up where you left off.».
3. **Google** и **Apple** (h52, pill, фон `#0A0C0B`, белые логотипы 20,
   подпись Inter 13/600) → **вход через провайдера** (демо: входит сразу).
4. **«or with email»** — разделитель с линиями `#FFFFFF26`.
5. **Email** (h52 `#1D1F1E` r8, иконка письма 18, «you@example.com»).
6. **Password** (h52, иконка замка, «Enter your password»; иконка глаза —
   показать/скрыть пароль).
7. **«Forgot password?»** Inter 13 `#00FF87` → **Reset password (10).**
8. **«Sign in»** (h54, `#00FF87`) — **валидирует** email и пароль (≥8 символов,
   ошибка под полями) и **входит** (возврат на Profile).
9. **«New here? Create an account»** `#00FF87` → **Create account (09).**
10. **Подпись** «Your details stay private. Your music gets heard.».

---

## Guest · 09 · Create account

Тот же каркас, что Sign in: **Google/Apple** → вход; **Email**, **Password**
(подсказка «Use at least 8 characters.»); **«Create account»** (h54) —
валидирует и создаёт аккаунт; **«Already a member? Sign in»** → назад;
подпись про Terms of Service и Privacy Policy.

---

## Guest · 10 · Reset password

1. **Заголовок** Anton 34 «Forgot your password?» + описание.
2. **Email** (h52 `#1D1F1E`).
3. **«Send reset link»** (h54) — **проверяет email** и показывает диалог
   «Reset link sent» (кнопка «OK» возвращает к Sign in).
4. **«Back to sign in»** `#00FF87` → **возврат на Sign in.**
5. Подпись «Remember to check your spam folder.».

---

## Guest · 11 · Explore (таб Explore)

1. **Заголовок** Anton 34 «Explore» + подпись Inter 12.
2. **Поле поиска комнат** (h48, pill, фон `#141815`, лупа 16,
   «Search rooms, clubs or DJs») — **фильтрует комнаты** по названию/диджею/жанру.
3. **Чипы жанров** (House · Techno · Afro house · Disco · Trance, r8):
   в макете подсвечен House; по умолчанию показаны все комнаты,
   **тап по чипу фильтрует**, повторный тап снимает фильтр.
4. **Сводка** «5 ROOMS WITHIN 2 KM» + **«Distance ↓»** (сортировка, демо).
5. **Featured-карточка** 350×132 (r8, фото, градиент снизу):
   бейдж **«LIVE NOW»** (фон `#101813` 50 %, точка 6 `#00FF87`),
   **круглая кнопка «→» 36×36** `#00FF87` справа; название Anton 24 белый;
   «Techno · 200 m · DJ Monark behind the decks» Inter 12 `#CDD5D0`.
   → **тап по карточке/кнопке открывает Room (12).**
6. **Строки комнат**: название Inter 13/600, «House · 450 m» Inter 12,
   «DJ · DJ Linnea» Inter 12/500 `#00FF87`, справа «NOW PLAYING» Inter 12/700
   + трек Inter 12/600, шеврон 14.
   → **тап открывает Room (12).**

---

## Guest · 12 · Room · Now playing

1. **Шапка «бренд + город»**.
2. **Карточка комнаты** (h~72): фото 48×48 r6, название Inter 17/600,
   «Main floor · DJ Markus» Inter 12, бейдж **«LIVE NOW»** (фон `#00FF871A`,
   текст `#00FF87`); ниже «Requests open · set till 06:00» (иконка 14 + Inter 12/500).
3. **Now playing**: «NOW PLAYING» (иконка диска 16 `#00FF87` + Inter 12/700),
   справа «Sunrise set»; трек **Anton 44** «Move», исполнитель Inter 13/500;
   **эквалайзер** 42 полосы (активные `#00FF87`, спящие `#FFFFFF2E`, r2,
   анимация); полоса прогресса сета 4 px `#00FF87` на `#FFFFFF1F`;
   время «1:42» / «4:10».
4. **UP NEXT** (Inter 12/700) + «3 in queue»; строки очереди: номер Anton 12/600,
   трек Inter 13/600, «Corona · by @lena_k» Inter 12, шеврон 16.
5. **«Request a track»** (h54, `#00FF87`, pill, иконка +) → **открывает Search (03)**
   в этой комнате (при необходимости присоединяет).
6. **Подпись** «Requests are voted on by the room · DJ has final say» Inter 12.

---

## Guest · 13 · Request detail

1. **← назад** + «Request» Inter 14/600 + город.
2. **Герой**: обложка 56×56 r6, название Anton 22, исполнитель Inter 12,
   бейдж статуса («IN QUEUE #3» / «AWAITING DJ» / «PLAYED» / «DECLINED»,
   фон `#00FF871A`, текст `#00FF87`); мета: пин 13, «Kater Blau» Inter 12/600,
   точка, «#DI-0284» Inter 12.
3. **Таймлайн**: 4 шага (Sent to DJ → Accepted → In queue #3 → Played):
   выполненные — круг 24 `#00FF87` с галочкой + коннектор `#00FF87`;
   текущий — кольцо `#00FF87`; ожидающий — контур 24; подписи Inter 13/600 + Inter 12.
4. **Чек оплаты** (r16): «Payment» Inter 12 + «3,00 €» Inter 14/600 `#00FF87`
   + чип **PAID**; строки Requested at / Room / DJ / Payment method (Inter 12).
5. **«Share request»** (h54, `#00FF87`, иконка share) — **копирует детали
   запроса в буфер** (снекбар «Request details copied»).
6. **«Report an issue»** (h44, иконка флага 15 + Inter 13/600) —
   **открывает шторку обращения** с кнопкой «Contact support».

---

## Guest · 14 · Help & support (таб Profile)

1. **← назад** + «Help & support» Inter 14/600 + город.
2. **Заголовок** Anton 34 «How can we help?».
3. **Поиск помощи** (h48 r12 `#141815`, лупа 16, «Search help topics») —
   **фильтрует FAQ** по вопросу.
4. **FAQ** (вопрос Inter 14/500 + шеврон 16): тап **раскрывает ответ**
   (Inter 12/1.5); открытый — шеврон `#00FF87`.
5. **Карточка «Need a human?»**: иконка поддержки 34×34 r6 (фон tint),
   «Real people, replies in ~5 min» Inter 12, бейдж **LIVE** (`#00FF8714`,
   точка `#00FF87`), кнопка **«Chat with us»** (h54, иконка чата) —
   **открывает чат поддержки** (демо-ответ в ~5 мин), разделитель,
   строка почты **support@dropit.club** + **кнопка копирования 28×28** —
   **копирует email** (снекбар «Email copied»).
6. **«DROP IT v1.4.2»** Inter 12.

---

## Guest · 15 · Identity

1. **← назад** + бренд-локуп.
2. **Заголовок** Anton 34 «Personal details».
3. **YOUR DETAILS** (Inter 12/600): строки h52 (иконка 18 `#A5AEA9`,
   подпись Inter 12 + значение Inter 13, **карандаш 14**):
   - **Email** — mara@nightmail.de;
   - **Member code** — MK-4821;
   - **Home city** — Berlin, DE.
   → **карандаш открывает диалог редактирования** (поле + Cancel/Save).
4. **FAVOURITE GENRES**: чипы House (активный) / Afro house / Techno / Disco
   (r999) → **выбор жанра**.
5. **«Save changes»** (h54, `#00FF87`, иконка ✓) — **сохраняет** (снекбар
   «Details saved»).
6. **Подпись** «No ID, no name check. Rooms you join only ever see your handle.».

---

## Receipt — Neon ticket (внутри Request sent)

- **Внешний вид:** карточка `#141815` r8: шапка «PAYMENT RECEIPT» Inter 12/700,
  сумма Inter 13, чип **PAID** (`#00FF871A`, галочка 11 + текст `#00FF87`);
  **перфорация** — две боковые выемки 12×12 цветом фона и пунктир `#FFFFFF26`;
  строки: Requested at / Room / DJ / Payment method (иконка 13 + Inter 12,
  значение Inter 12/600).
- **Кнопки:** внутри модалки — **«Done»** (h54, `#00FF87`) — закрыть.
