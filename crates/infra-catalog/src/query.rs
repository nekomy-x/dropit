//! Построение и нормализация строк запроса: percent-encoding и лимиты.

/// Зажимает лимит результатов в диапазон `1..=25`.
///
/// iTunes Search и Deezer принимают `limit` в этом диапазоне; 0 или значения
/// выше 25 приводятся к границам, чтобы запрос оставался детерминированным и
/// не падал с ошибкой источника.
pub fn clamp_limit(limit: u8) -> u8 {
    limit.clamp(1, 25)
}

/// Percent-кодирует компонент query по RFC 3986 (unreserved set).
///
/// Символы `A–Z a–z 0–9 - . _ ~` оставляются как есть, пробел кодируется как
/// `%20` (не `+`), все прочие байты — как `%XX` в верхнем регистре. Реализовано
/// вручную, без внешних URL-крейтов, чтобы избежать новых зависимостей.
pub fn encode_query_component(value: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(value.len() * 3);
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push(DIGITS[(byte >> 4) as usize] as char);
                out.push(DIGITS[(byte & 0x0f) as usize] as char);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_unreserved() {
        assert_eq!(encode_query_component("A-z.0_9~"), "A-z.0_9~");
    }

    #[test]
    fn encodes_space_and_specials() {
        assert_eq!(encode_query_component(" "), "%20");
        assert_eq!(encode_query_component("a b"), "a%20b");
        assert_eq!(encode_query_component("a&b=c"), "a%26b%3Dc");
        assert_eq!(encode_query_component("a/b"), "a%2Fb");
    }

    #[test]
    fn encodes_utf8_multibyte() {
        // «é» = 0xC3 0xA9
        assert_eq!(encode_query_component("café"), "caf%C3%A9");
    }

    #[test]
    fn clamps_limit() {
        assert_eq!(clamp_limit(0), 1);
        assert_eq!(clamp_limit(1), 1);
        assert_eq!(clamp_limit(10), 10);
        assert_eq!(clamp_limit(25), 25);
        assert_eq!(clamp_limit(26), 25);
        assert_eq!(clamp_limit(255), 25);
    }
}
