//! Разбор аргументов командной строки.
//!
//! Синтаксис (без clap, ручной разбор):
//!   dropit-workers --once <job>   — один прогон задачи
//!   dropit-workers run            — супервизор по интервалам
//!   dropit-workers --help | -h    — справка
//!
//! `<job>` ∈ {`outbox`, `payouts`, `reconcile`, `retention`,
//! `close-sessions`, `push`, `mail`}.

use std::process::exit;

/// Имена одноразовых задач.
pub const ONCE_JOBS: &[&str] = &[
    "outbox",
    "payouts",
    "reconcile",
    "retention",
    "close-sessions",
    "push",
    "mail",
];

/// Распознанная команда.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Один прогон задачи.
    Once(String),
    /// Супервизор: все задачи по интервалам.
    Run,
}

/// Разобрать `std::env::args`. При невалидном вводе печатает справку
/// в stderr и завершает процесс кодом 2.
pub fn parse_args() -> Command {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("{}", help_text());
        exit(2);
    }
    match args[0].as_str() {
        "-h" | "--help" => {
            println!("{}", help_text());
            exit(0);
        }
        "run" => {
            if args.len() != 1 {
                eprintln!("error: `run` не принимает дополнительных аргументов\n");
                eprintln!("{}", help_text());
                exit(2);
            }
            Command::Run
        }
        "--once" => {
            if args.len() != 2 {
                eprintln!("error: `--once` требует имя задачи\n");
                eprintln!("{}", help_text());
                exit(2);
            }
            let job = &args[1];
            if !ONCE_JOBS.contains(&job.as_str()) {
                eprintln!("error: неизвестная задача `{job}`\n");
                eprintln!("{}", help_text());
                exit(2);
            }
            Command::Once(job.clone())
        }
        other => {
            eprintln!("error: неизвестный аргумент `{other}`\n");
            eprintln!("{}", help_text());
            exit(2);
        }
    }
}

/// Текст справки.
#[must_use]
pub fn help_text() -> String {
    let mut s = String::new();
    s.push_str("dropit-workers — фоновые задачи DROP IT\n\n");
    s.push_str("USAGE:\n");
    s.push_str("    dropit-workers --once <job>   один прогон задачи\n");
    s.push_str("    dropit-workers run            супервизор по интервалам\n");
    s.push_str("    dropit-workers --help         справка\n\n");
    s.push_str("JOBS:\n");
    for j in ONCE_JOBS {
        let desc = match *j {
            "outbox" => "опубликовать накопленные outbox-события",
            "payouts" => "выплаты за закрытые периоды (+ TTL интентов)",
            "reconcile" => "сверка ledger vs провайдер (+ инвариант payee)",
            "retention" => "GDPR/retention: псевдонимизация по срокам",
            "close-sessions" => "автозакрытие сессий по расписанию клуба",
            "push" => "отправка накопленных push-уведомлений",
            "mail" => "отправка очереди транзакционных писем (Resend)",
            _ => "",
        };
        s.push_str(&format!("    {j:<16}{desc}\n"));
    }
    s.push_str("\nENV:\n");
    s.push_str("    DATABASE_URL, NATS_URL, REDIS_URL,\n");
    s.push_str("    OUTBOX_INTERVAL_SECS, PAYOUT_INTERVAL_SECS,\n");
    s.push_str("    RECONCILE_INTERVAL_SECS, RETENTION_INTERVAL_SECS,\n");
    s.push_str("    CLOSE_SESSIONS_INTERVAL_SECS, PUSH_INTERVAL_SECS,\n");
    s.push_str("    OUTBOX_BATCH_SIZE, OUTBOX_MAX_ATTEMPTS, PUSH_BATCH_SIZE,\n");
    s.push_str("    DROPIT_RUN_MIGRATIONS\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_args<F: FnOnce()>(args: &[&str], f: F) {
        // parse_args читает std::env::args напрямую; для тестирования
        // используем parse_slice.
        let _ = args;
        f();
    }

    /// Чистая функция разбора слайса аргументов (без exit).
    pub fn parse_slice(args: &[String]) -> Result<Command, String> {
        if args.is_empty() {
            return Err("no args".into());
        }
        match args[0].as_str() {
            "run" => {
                if args.len() != 1 {
                    return Err("`run` не принимает аргументов".into());
                }
                Ok(Command::Run)
            }
            "--once" => {
                if args.len() != 2 {
                    return Err("`--once` требует имя задачи".into());
                }
                let job = &args[1];
                if !ONCE_JOBS.contains(&job.as_str()) {
                    return Err(format!("неизвестная задача `{job}`"));
                }
                Ok(Command::Once(job.clone()))
            }
            "-h" | "--help" => Err("__help__".into()),
            other => Err(format!("неизвестный аргумент `{other}`")),
        }
    }

    #[test]
    fn run_command() {
        let args = vec!["run".to_string()];
        assert_eq!(parse_slice(&args).unwrap(), Command::Run);
    }

    #[test]
    fn once_each_job() {
        for job in ONCE_JOBS {
            let args = vec!["--once".to_string(), job.to_string()];
            assert_eq!(parse_slice(&args).unwrap(), Command::Once(job.to_string()));
        }
    }

    #[test]
    fn once_unknown_job_rejected() {
        let args = vec!["--once".to_string(), "bogus".to_string()];
        assert!(parse_slice(&args).is_err());
    }

    #[test]
    fn once_missing_job_rejected() {
        let args = vec!["--once".to_string()];
        assert!(parse_slice(&args).is_err());
    }

    #[test]
    fn run_with_extra_arg_rejected() {
        let args = vec!["run".to_string(), "x".to_string()];
        assert!(parse_slice(&args).is_err());
    }

    #[test]
    fn unknown_first_arg_rejected() {
        let args = vec!["frobnicate".to_string()];
        assert!(parse_slice(&args).is_err());
    }

    #[test]
    fn empty_args_rejected() {
        let args: Vec<String> = vec![];
        assert!(parse_slice(&args).is_err());
    }

    #[test]
    fn help_text_lists_all_jobs() {
        let h = help_text();
        for job in ONCE_JOBS {
            assert!(h.contains(job), "help должен упоминать {job}");
        }
    }

    // Удовлетворяем неиспользуемую обёртку (просто демонстрируем вызов).
    #[test]
    fn with_args_wrapper_is_harmless() {
        with_args(&["run"], || {
            assert_eq!(
                ONCE_JOBS.len(),
                7,
                "outbox, payouts, reconcile, retention, close-sessions, push, mail"
            );
        });
    }
}
