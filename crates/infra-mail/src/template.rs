//! Шаблоны писем и подстановка `{{var}}`.
//!
//! Движка шаблонов нет намеренно (контракт §3): четыре письма не стоят
//! новой зависимости, а `handlebars`/`tera` тянут за собой парсер, кейсы
//! и целый класс ошибок времени выполнения. Здесь — один проход по
//! строке, подстановка по имени и один осознанный выбор: значение
//! экранируется в HTML-версии и не экранируется в текстовой.
//!
//! Отсутствующая переменная — ошибка [`MailError::Config`], а не пустая
//! строка: письмо с дырой вместо кода подтверждения хуже, чем письмо,
//! которое не отправилось. Такое письмо должно упасть в тесте, а не
//! уехать гостю.
//!
//! Ссылки приходят в `vars` от вызывающего: рендер не читает окружение,
//! поэтому `MAIL_APP_BASE_URL` — забота слоя, который собирает письмо.

use crate::error::MailError;

/// Ключ шаблона (он же `mail_message.template`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKey {
    /// Приглашение DJ в клуб.
    DjInvite,
    /// Код подтверждения e-mail гостя.
    GuestVerify,
    /// Уведомление о выплате.
    PayoutPaid,
    /// Тестовое письмо из админки.
    MailTest,
}

impl TemplateKey {
    /// Строковое имя шаблона — значение колонки `template`.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DjInvite => "dj_invite",
            Self::GuestVerify => "guest_verify",
            Self::PayoutPaid => "payout_paid",
            Self::MailTest => "mail_test",
        }
    }

    /// Разобрать имя шаблона; неизвестное — `None`.
    ///
    /// Неизвестный шаблон — это не «другое письмо», а рассинхрон кода и
    /// данных, поэтому здесь `None`, а не вариант `Other`.
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim() {
            "dj_invite" => Some(Self::DjInvite),
            "guest_verify" => Some(Self::GuestVerify),
            "payout_paid" => Some(Self::PayoutPaid),
            "mail_test" => Some(Self::MailTest),
            _ => None,
        }
    }

    /// Тексты шаблона.
    fn parts(self) -> Template {
        match self {
            Self::DjInvite => Template {
                subject: "Приглашение в {{club_name}}",
                preheader: "Клуб приглашает вас в консоль диджея DROP IT",
                body: DJ_INVITE_BODY,
                text: DJ_INVITE_TEXT,
            },
            Self::GuestVerify => Template {
                subject: "Подтвердите e-mail",
                preheader: "Код подтверждения для аккаунта DROP IT",
                body: GUEST_VERIFY_BODY,
                text: GUEST_VERIFY_TEXT,
            },
            Self::PayoutPaid => Template {
                subject: "Выплата отправлена",
                preheader: "Выплата за период отправлена на счёт",
                body: PAYOUT_PAID_BODY,
                text: PAYOUT_PAID_TEXT,
            },
            Self::MailTest => Template {
                subject: "Тестовое письмо DROP IT",
                preheader: "Проверка транспорта писем",
                body: MAIL_TEST_BODY,
                text: MAIL_TEST_TEXT,
            },
        }
    }
}

/// Готовое письмо: тема и две версии тела.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedMail {
    /// Тема письма.
    pub subject: String,
    /// HTML-версия (значения переменных экранированы).
    pub html: String,
    /// Текстовая версия (значения как есть).
    pub text: String,
}

/// Исходники одного шаблона.
struct Template {
    subject: &'static str,
    preheader: &'static str,
    body: &'static str,
    text: &'static str,
}

/// Отрендерить письмо по ключу и переменным.
///
/// Переменные берутся из JSON-объекта по имени; поддерживаются строки,
/// числа и булевы значения. `null`, массивы и объекты считаются
/// отсутствующими: подставлять `[object Object]` в письмо нельзя.
pub fn render(key: TemplateKey, vars: &serde_json::Value) -> Result<RenderedMail, MailError> {
    let parts = key.parts();
    let key_str = key.as_str();
    let subject = substitute(parts.subject, vars, key_str, false)?;
    let html_doc = layout(parts.preheader, parts.body);
    let html = substitute(&html_doc, vars, key_str, true)?;
    let text = substitute(parts.text, vars, key_str, false)?;
    Ok(RenderedMail {
        subject,
        html,
        text,
    })
}

/// Пройти по строке и заменить `{{var}}`.
///
/// `escape` включает HTML-экранирование подставленных значений (сам
/// шаблон не экранируется — иначе разметка письма превратилась бы в
/// текст).
fn substitute(
    template: &str,
    vars: &serde_json::Value,
    key: &str,
    escape: bool,
) -> Result<String, MailError> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find("}}").ok_or_else(|| {
            MailError::Config(format!("template {key}: unterminated {{{{ placeholder"))
        })?;
        let name = after[..end].trim();
        if name.is_empty() {
            return Err(MailError::Config(format!(
                "template {key}: empty {{{{}}}} placeholder"
            )));
        }
        let value = lookup(vars, name).ok_or_else(|| {
            MailError::Config(format!(
                "missing template var {{{{ {name} }}}} in template {key}"
            ))
        })?;
        if escape {
            out.push_str(&escape_html(&value));
        } else {
            out.push_str(&value);
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Достать скалярную переменную как строку.
fn lookup(vars: &serde_json::Value, name: &str) -> Option<String> {
    match vars.get(name)? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Экранировать текст для вставки в HTML.
///
/// Покрывает пять символов, которых достаточно для текста и значений
/// атрибутов: `& < > " '`. Кавычка-апостроф уходит в `&#39;`, чтобы
/// строка была безопасна и в одинарных кавычках атрибута.
#[must_use]
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Общий каркас письма: тёмная карточка с акцентом продукта.
///
/// Вёрстка таблицами и inline-стилями: почтовые клиенты не грузят
/// внешний CSS и не знают `<style>`. Ширина 520 px — читаемая строка на
/// десктопе и полная ширина на телефоне.
fn layout(preheader: &str, body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 1600);
    out.push_str("<!doctype html>\n<html lang=\"ru\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<meta name=\"color-scheme\" content=\"dark light\">\n");
    out.push_str("<title>");
    out.push_str(preheader);
    out.push_str("</title>\n</head>\n<body style=\"margin:0;padding:0;background:#0b0b0f;");
    out.push_str("font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;");
    out.push_str("color:#f2f2f5;-webkit-font-smoothing:antialiased;\">\n");
    out.push_str("<div style=\"display:none;max-height:0;overflow:hidden;opacity:0;\">");
    out.push_str(preheader);
    out.push_str("</div>\n");
    out.push_str("<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" ");
    out.push_str(
        "style=\"background:#0b0b0f;\">\n<tr><td align=\"center\" style=\"padding:32px 16px;\">\n",
    );
    out.push_str("<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" ");
    out.push_str("style=\"max-width:520px;background:#15151b;border:1px solid #23232b;");
    out.push_str("border-radius:16px;\">\n<tr><td style=\"padding:32px;\">\n");
    out.push_str("<div style=\"font-size:13px;font-weight:700;letter-spacing:.14em;");
    out.push_str("text-transform:uppercase;color:#00ff87;margin:0 0 20px;\">DROP IT</div>\n");
    out.push_str(body);
    out.push_str("\n</td></tr>\n</table>\n");
    out.push_str("<p style=\"margin:16px 0 0;font-size:12px;line-height:18px;color:#8a8a94;\">");
    out.push_str("DROP IT — заявки гостей и очередь диджея.<br>");
    out.push_str("Это письмо отправлено автоматически, отвечать на него не нужно.");
    out.push_str("</p>\n</td></tr>\n</table>\n</body>\n</html>\n");
    out
}

/// Текстовая версия приглашения DJ.
const DJ_INVITE_TEXT: &str = "\
Привет, {{dj_name}}!

Клуб {{club_name}} приглашает вас в DROP IT — консоль диджея, куда гости \
присылают треки.

Открыть приглашение: {{invite_url}}
Ссылка действует до {{expires_at}}.

Если вы не ждали это письмо, просто проигнорируйте его: без вашего \
подтверждения ничего не произойдёт.
";

/// HTML-версия приглашения DJ.
const DJ_INVITE_BODY: &str = "\
<h1 style=\"margin:0 0 16px;font-size:24px;line-height:32px;font-weight:700;color:#f2f2f5;\">\
Вас приглашают в DROP IT</h1>
<p style=\"margin:0 0 12px;font-size:15px;line-height:24px;color:#c9c9d1;\">\
Привет, {{dj_name}}! Клуб <strong style=\"color:#f2f2f5;\">{{club_name}}</strong> \
приглашает вас в консоль диджея: гости присылают треки, вы принимаете их в очередь.</p>
<p style=\"margin:0 0 24px;font-size:15px;line-height:24px;color:#c9c9d1;\">\
Ссылка действует до {{expires_at}}.</p>
<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" style=\"margin:0 0 24px;\">\
<tr><td style=\"background:#00ff87;border-radius:10px;\">\
<a href=\"{{invite_url}}\" style=\"display:inline-block;padding:13px 22px;font-size:15px;\
font-weight:600;color:#06110b;text-decoration:none;\">Открыть приглашение</a>\
</td></tr></table>
<p style=\"margin:0 0 8px;font-size:13px;line-height:20px;color:#8a8a94;\">\
Если кнопка не открывается, скопируйте ссылку в браузер:</p>
<p style=\"margin:0 0 20px;font-size:13px;line-height:20px;color:#c9c9d1;word-break:break-all;\">\
{{invite_url}}</p>
<p style=\"margin:0;font-size:13px;line-height:20px;color:#8a8a94;\">\
Если вы не ждали это письмо, просто проигнорируйте его.</p>";

/// Текстовая версия кода подтверждения.
const GUEST_VERIFY_TEXT: &str = "\
Код подтверждения: {{code}}

Введите его в приложении DROP IT. Код действует до {{expires_at}}.

Если вы не запрашивали код, проигнорируйте письмо: без него аккаунт \
не подтверждается.
";

/// HTML-версия кода подтверждения.
const GUEST_VERIFY_BODY: &str = "\
<h1 style=\"margin:0 0 16px;font-size:24px;line-height:32px;font-weight:700;color:#f2f2f5;\">\
Подтвердите e-mail</h1>
<p style=\"margin:0 0 20px;font-size:15px;line-height:24px;color:#c9c9d1;\">\
Введите этот код в приложении DROP IT:</p>
<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" width=\"100%\" \
style=\"margin:0 0 20px;\"><tr><td align=\"center\" style=\"background:#1d1d25;\
border:1px solid #2b2b35;border-radius:12px;padding:20px 16px;\">\
<div style=\"font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;\
font-size:32px;line-height:38px;font-weight:700;letter-spacing:.32em;color:#00ff87;\">{{code}}</div>\
</td></tr></table>
<p style=\"margin:0 0 20px;font-size:15px;line-height:24px;color:#c9c9d1;\">\
Код действует до {{expires_at}}.</p>
<p style=\"margin:0;font-size:13px;line-height:20px;color:#8a8a94;\">\
Если вы не запрашивали код, проигнорируйте письмо: без него аккаунт не подтверждается.</p>";

/// Текстовая версия уведомления о выплате.
const PAYOUT_PAID_TEXT: &str = "\
{{payee_name}}, выплата отправлена.

Сумма: {{amount}}
Период: {{period}}

Деньги идут на счёт получателя; зачисление обычно занимает 1–3 рабочих дня \
и зависит от банка. Итоговые суммы по сменам видны в панели.
";

/// HTML-версия уведомления о выплате.
const PAYOUT_PAID_BODY: &str = "\
<h1 style=\"margin:0 0 16px;font-size:24px;line-height:32px;font-weight:700;color:#f2f2f5;\">\
Выплата отправлена</h1>
<p style=\"margin:0 0 20px;font-size:15px;line-height:24px;color:#c9c9d1;\">\
{{payee_name}}, выплата за период отправлена на счёт.</p>
<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" border=\"0\" width=\"100%\" \
style=\"margin:0 0 20px;background:#1d1d25;border:1px solid #2b2b35;border-radius:12px;\">
<tr><td style=\"padding:14px 16px;font-size:13px;line-height:20px;color:#8a8a94;\">Сумма</td>\
<td align=\"right\" style=\"padding:14px 16px;font-size:15px;line-height:20px;\
font-weight:600;color:#f2f2f5;\">{{amount}}</td></tr>
<tr><td style=\"padding:0 16px 14px;font-size:13px;line-height:20px;color:#8a8a94;\">Период</td>\
<td align=\"right\" style=\"padding:0 16px 14px;font-size:15px;line-height:20px;\
color:#c9c9d1;\">{{period}}</td></tr>
</table>
<p style=\"margin:0 0 20px;font-size:15px;line-height:24px;color:#c9c9d1;\">\
Зачисление обычно занимает 1–3 рабочих дня и зависит от банка.</p>
<p style=\"margin:0;font-size:13px;line-height:20px;color:#8a8a94;\">\
Итоговые суммы по сменам всегда видны в панели — письмо лишь уведомление.</p>";

/// Текстовая версия тестового письма.
const MAIL_TEST_TEXT: &str = "\
Это тестовое письмо DROP IT.

Если вы его получили, транспорт писем настроен верно: ключ Resend принят, \
домен отправителя подтверждён.

Отправил: {{sent_by}}
";

/// HTML-версия тестового письма.
const MAIL_TEST_BODY: &str = "\
<h1 style=\"margin:0 0 16px;font-size:24px;line-height:32px;font-weight:700;color:#f2f2f5;\">\
Тестовое письмо DROP IT</h1>
<p style=\"margin:0 0 16px;font-size:15px;line-height:24px;color:#c9c9d1;\">\
Если вы это читаете, транспорт писем настроен верно: ключ Resend принят, \
домен отправителя подтверждён.</p>
<p style=\"margin:0 0 20px;font-size:15px;line-height:24px;color:#c9c9d1;\">\
Письмо отправлено вручную из админ-панели.</p>
<p style=\"margin:0;font-size:13px;line-height:20px;color:#8a8a94;\">\
Отправил: {{sent_by}}</p>";
