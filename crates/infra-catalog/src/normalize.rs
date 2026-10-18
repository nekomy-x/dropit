//! Нормализация метаданных трека: fold диакритики, канонический ключ,
//! валидация ISRC и политика TTL кеша.

use std::time::Duration;

/// TTL кеша каталога при наличии ISRC — 24 ч (ADR-006, техкарта Часть VI).
///
/// ISRC даёт вторую, серверную точку верификации трека (MusicBrainz), поэтому
/// запись с ISRC кешируется надолго.
pub const CACHE_TTL_WITH_ISRC: Duration = Duration::from_secs(86_400);

/// TTL кеша каталога без ISRC — 1 ч.
///
/// Метаданные без ISRC нуждаются в скором обогащении/перепроверке, поэтому
/// кешируются коротко.
pub const CACHE_TTL_WITHOUT_ISRC: Duration = Duration::from_secs(3_600);

/// Сворачивает распространённые латинские диакритики в ASCII-эквиваленты.
///
/// Покрывает Latin-1 Supplement и основную часть Latin Extended-A: `é`→`e`,
/// `ü`→`u`, `ñ`→`n`, `ß`→`ss`, `æ`→`ae`, `ø`→`o`, `Đ`→`D` и т. п. Символы вне
/// таблицы (в т. ч. кириллица/CJK) остаются как есть — они сохраняются в
/// каноническом ключе дословно и сравниваются посимвольно.
pub fn fold_diacritics(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match fold_char(c) {
            Some(rep) => out.push_str(rep),
            None => out.push(c),
        }
    }
    out
}

/// Возвращает ASCII-замену для одного диакритического символа или `None`.
fn fold_char(c: char) -> Option<&'static str> {
    Some(match c {
        // Latin-1 Supplement
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "A",
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
        'Ç' => "C",
        'ç' => "c",
        'È' | 'É' | 'Ê' | 'Ë' => "E",
        'è' | 'é' | 'ê' | 'ë' => "e",
        'Ì' | 'Í' | 'Î' | 'Ï' => "I",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'Ñ' => "N",
        'ñ' => "n",
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => "O",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => "o",
        'Ù' | 'Ú' | 'Û' | 'Ü' => "U",
        'ù' | 'ú' | 'û' | 'ü' => "u",
        'Ý' => "Y",
        'ý' | 'ÿ' => "y",
        'Ð' => "D",
        'ð' => "d",
        'Ø' => "O",
        'ø' => "o",
        'Þ' => "TH",
        'þ' => "th",
        'ß' => "ss",
        'Æ' => "AE",
        'æ' => "ae",
        // Latin Extended-A
        'Ā' | 'Ă' | 'Ą' => "A",
        'ā' | 'ă' | 'ą' => "a",
        'Ć' | 'Ĉ' | 'Ċ' | 'Č' => "C",
        'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'Ď' | 'Đ' => "D",
        'ď' | 'đ' => "d",
        'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => "E",
        'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => "G",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'Ĥ' | 'Ħ' => "H",
        'ĥ' | 'ħ' => "h",
        'Ĩ' | 'Ī' | 'Ĭ' | 'Į' => "I",
        'ĩ' | 'ī' | 'ĭ' | 'į' => "i",
        'İ' => "I",
        'ı' => "i",
        'Ĵ' => "J",
        'ĵ' => "j",
        'Ķ' => "K",
        'ķ' => "k",
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' => "L",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' => "l",
        'Ł' => "L",
        'ł' => "l",
        'Ń' | 'Ņ' | 'Ň' => "N",
        'ń' | 'ņ' | 'ň' => "n",
        'Ŋ' => "N",
        'ŋ' => "n",
        'Ō' | 'Ŏ' | 'Ő' => "O",
        'ō' | 'ŏ' | 'ő' => "o",
        'Ŕ' | 'Ŗ' | 'Ř' => "R",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'Ś' | 'Ŝ' | 'Ş' | 'Š' => "S",
        'ś' | 'ŝ' | 'ş' | 'š' => "s",
        'Ţ' | 'Ť' | 'Ŧ' => "T",
        'ţ' | 'ť' | 'ŧ' => "t",
        'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => "U",
        'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'Ŵ' => "W",
        'ŵ' => "w",
        'Ŷ' => "Y",
        'ŷ' => "y",
        'Ź' | 'Ż' | 'Ž' => "Z",
        'ź' | 'ż' | 'ž' => "z",
        'Œ' => "OE",
        'œ' => "oe",
        'Ĳ' => "IJ",
        'ĳ' => "ij",
        _ => return None,
    })
}

/// Маркеры feat./prod., по которым обрезается нормализованное поле.
const FEAT_TOKENS: &[&str] = &["feat", "ft", "featuring", "prod", "produced"];

/// Нормализует одно поле (artist или title) в нижний регистр ASCII-fold без
/// пунктуации и feat./prod.-суффиксов, со свёрнутыми пробелами.
fn normalize_part(input: &str) -> String {
    let folded = fold_diacritics(input);
    let lower = folded.to_lowercase();

    // Апострофы выкидываем целиком ("don't" → "dont"); прочую пунктуацию —
    // в пробел ("hip-hop" → "hip hop"); буквенно-цифровые символы (включая
    // не-латиницу) оставляем.
    let mut cleaned = String::with_capacity(lower.len());
    for c in lower.chars() {
        if c.is_alphanumeric() {
            cleaned.push(c);
        } else if c == '\'' || c == '\u{2019}' || c == '`' {
            // drop apostrophe
        } else {
            cleaned.push(' ');
        }
    }

    // Обрезаем всё, начиная с первого feat./prod.-маркера (как суффикса).
    let mut kept: Vec<&str> = Vec::new();
    for token in cleaned.split_whitespace() {
        if FEAT_TOKENS.contains(&token) {
            break;
        }
        kept.push(token);
    }
    kept.join(" ")
}

/// Канонический ключ дедупликации `(artist|title)`.
///
/// Детерминированный порядок: всегда `artist` затем `title`, разделитель `|`.
/// Применяется fold диакритиков, нижний регистр, удаление пунктуации
/// (апострофы выкидываются, прочее → пробел), обрезка feat./prod.-суффиксов
/// и свёртка пробелов. Пустые поля сохраняют своё место, так что
/// `canonical_key("", "Song")` == `"|song"`; оба поля пусты → `""`.
///
/// Используется для свёртки дублей между источниками (iTunes/Deezer) и для
/// счётчика «той же песни» (техкарта, Часть II «КАТАЛОГ»).
pub fn canonical_key(artist: &str, title: &str) -> String {
    let a = normalize_part(artist);
    let t = normalize_part(title);
    if a.is_empty() && t.is_empty() {
        return String::new();
    }
    format!("{a}|{t}")
}

/// Внутренний нормализатор одного токена (для фильтрации в `MockCatalog`).
pub(crate) fn normalize_token(input: &str) -> String {
    normalize_part(input)
}

/// Проверяет структуру ISRC `CC-XXX-YY-NNNNN` (12 символов).
///
/// Принимает как компактную (`USUM71302116`), так и дефисованную
/// (`US-UM7-13-02116`) запись, игнорирует регистр и пробелы. После
/// нормализации требует ровно 12 символов: 2 буквы страны, 3
/// буквенно-цифровых кода регистранта, 2 цифры года, 5 цифр назначения.
pub fn validate_isrc(isrc: &str) -> bool {
    let compact: String = isrc
        .chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .collect();
    let up = compact.to_ascii_uppercase();
    let chars: Vec<char> = up.chars().collect();
    if chars.len() != 12 {
        return false;
    }
    let country = chars[0].is_ascii_alphabetic() && chars[1].is_ascii_alphabetic();
    let registrant = chars[2..5].iter().all(|c| c.is_ascii_alphanumeric());
    let year = chars[5].is_ascii_digit() && chars[6].is_ascii_digit();
    let designation = chars[7..12].iter().all(|c| c.is_ascii_digit());
    country && registrant && year && designation
}

/// Возвращает TTL кеша каталога в зависимости от наличия ISRC.
///
/// С ISRC — [`CACHE_TTL_WITH_ISRC`] (24 ч, есть серверная верификация);
/// без ISRC — [`CACHE_TTL_WITHOUT_ISRC`] (1 ч, требует обогащения).
pub fn cache_ttl(isrc_present: bool) -> Duration {
    if isrc_present {
        CACHE_TTL_WITH_ISRC
    } else {
        CACHE_TTL_WITHOUT_ISRC
    }
}

/// Каноническое поисковое слово для клубного стиля.
///
/// Сопоставляет распространённые написания (включая диакритику и варианты
/// через дефис) одному ключевому слову, которое достраивает поисковый терм
/// iTunes/Deezer. Возвращает `None` для неизвестного стиля — тогда терм не
/// достраивается.
pub fn style_keyword(style: &str) -> Option<&'static str> {
    let folded = fold_diacritics(style);
    let lower = folded.to_lowercase();
    let collapsed: String = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    match collapsed.as_str() {
        "salsa" => Some("salsa"),
        "bachata" => Some("bachata"),
        "merengue" => Some("merengue"),
        "reggaeton" | "regueton" => Some("reggaeton"),
        "kizomba" => Some("kizomba"),
        "zouk" => Some("zouk"),
        "cumbia" => Some("cumbia"),
        "house" => Some("house"),
        "techno" => Some("techno"),
        "electronic" | "edm" => Some("electronic"),
        "hip-hop" | "hiphop" | "hip hop" => Some("hip hop"),
        "r&b" | "rnb" | "r and b" => Some("r&b"),
        "pop" => Some("pop"),
        "rock" => Some("rock"),
        "latin" => Some("latin"),
        "afrobeat" => Some("afrobeat"),
        "disco" => Some("disco"),
        "funk" => Some("funk"),
        "dancehall" => Some("dancehall"),
        "dembow" => Some("dembow"),
        "trap" => Some("trap"),
        _ => None,
    }
}

/// Строит поисковый терм для источника из `q` и опционального стиля.
///
/// `q` обрезается и свёртывается по пробелам; если `style` отображается в
/// ключевое слово ([`style_keyword`]) и его ещё нет в `q` (без учёта
/// регистра), оно дописывается через пробел. Пустое `q` со стилем возвращает
/// только ключевое слово; пустое `q` без стиля — пустую строку.
pub fn build_search_term(q: &str, style: Option<&str>) -> String {
    let collapsed: String = q.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(style) = style {
        if let Some(keyword) = style_keyword(style) {
            // Подстрока в нижнем регистре: «hip hop» в «hip hop mix» совпадает,
            // «r&b» в «r&b song» совпадает — ключевое слово не дублируем.
            if !collapsed.to_lowercase().contains(keyword) {
                if collapsed.is_empty() {
                    return keyword.to_owned();
                }
                return format!("{collapsed} {keyword}");
            }
        }
    }
    collapsed
}
