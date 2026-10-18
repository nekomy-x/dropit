//! Модели данных каталога: один命中 поиска и параметры запроса.

use crate::normalize::canonical_key;

/// Один результат поиска трека во внешнем каталоге.
///
/// Носитель-агностическое представление, в которое нормализуются ответы
/// iTunes Search и Deezer. Поле `source` хранит нижнерегистровое имя
/// источника (`"itunes"` / `"deezer"`), `source_id` — идентификатор трека
/// в этом источнике (числовой, как строка). Вместе они образуют ключ
/// дедупликации на уровне хранилища (`UNIQUE(source, source_id)`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CatalogHit {
    /// Имя источника в нижнем регистре (`"itunes"` / `"deezer"`).
    pub source: String,
    /// Идентификатор трека в источнике (числовой, как строка).
    pub source_id: String,
    /// Название трека.
    pub title: String,
    /// Исполнитель (основной).
    pub artist: String,
    /// Альбом, если источник его сообщил.
    pub album: Option<String>,
    /// Длительность в миллисекундах (0, если источник не сообщил).
    pub duration_ms: i64,
    /// ISRC `CC-XXX-YY-NNNNN`, если источник его сообщил.
    pub isrc: Option<String>,
    /// URL обложки наибольшего доступного разрешения, если есть.
    pub artwork_url: Option<String>,
    /// URL 30-сек превью, если есть.
    pub preview_url: Option<String>,
    /// Каноническая страница трека в источнике (например `trackViewUrl`).
    pub source_url: Option<String>,
    /// Признак explicit-контента ( parental advisory ).
    pub explicit: bool,
}

impl CatalogHit {
    /// Стабильный ключ дедупликации `(artist|title)` без учёта регистра,
    /// диакритики, пунктуации и feat./prod.-суффиксов — делегирует
    /// [`canonical_key`].
    ///
    /// Используется для свёртки дублей между источниками и счётчика «той же
    /// песни» (техкарта, Часть II «КАТАЛОГ»).
    pub fn canonical_key(&self) -> String {
        canonical_key(&self.artist, &self.title)
    }
}

/// Параметры поискового запроса к каталогу.
///
/// `limit` ограничивает число возвращаемых хитов; фактический лимит,
/// отправляемый источнику, получается через [`SearchQuery::normalized_limit`]
/// и зажимается в диапазон `1..=25`. `style` — опциональный клубный стиль
/// (salsa, bachata, …), который при наличии достраивает поисковый терм через
/// [`build_search_term`](crate::build_search_term).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchQuery {
    /// Текст поискового запроса (название/исполнитель).
    pub q: String,
    /// Желаемое число результатов (зажимается в `1..=25`).
    pub limit: u8,
    /// Опциональный клубный стиль для уточнения терма.
    pub style: Option<String>,
}

impl SearchQuery {
    /// Создаёт запрос с дефолтным лимитом 10 и без стиля.
    pub fn new(q: impl Into<String>) -> Self {
        Self {
            q: q.into(),
            limit: 10,
            style: None,
        }
    }

    /// Устанавливает лимит результатов (chainable).
    pub fn with_limit(mut self, limit: u8) -> Self {
        self.limit = limit;
        self
    }

    /// Устанавливает стиль (chainable).
    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Фактический лимит, зажатый в `1..=25` (см. [`clamp_limit`](crate::clamp_limit)).
    pub fn normalized_limit(&self) -> u8 {
        crate::clamp_limit(self.limit)
    }
}
