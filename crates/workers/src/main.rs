//! DROP IT workers: фоновые задачи (outbox relay, выплаты, сверка, retention,
//! автозакрытие сессий, push-диспетчер, диспетчер писем).
//!
//! Запуск:
//!   `dropit-workers --once <job>` — один прогон (CI/крон),
//!   `dropit-workers run` — супервизор с расписанием по интервалам из env.
//!
//! Каждая задача печатает одну JSON-строку итога и возвращает код 0/1.
//! Деградация: без `DATABASE_URL` БД-задачи пропускаются; без `NATS_URL`
//! используется `NoopBus` (outbox-реле пропускается, push продолжает работу);
//! без push-ключей — `NoopPush`; без `RESEND_API_KEY` — `NoopMail`.

mod cli;
mod config;
mod jobs;
mod mail;
mod push;
mod runtime;

use std::sync::Arc;

use cli::Command;
use config::Config;
use jobs::JobReport;
use runtime::WorkerEnv;

#[tokio::main]
async fn main() {
    init_tracing();
    let cmd = cli::parse_args();
    let config = Config::from_env();
    let env = Arc::new(WorkerEnv::build(config).await);

    // Heartbeat для публичной `/status`: супервизор пишет каждые 15 с,
    // одноразовый прогон — один раз перед работой. Без БД пропускается.
    if let Some(repo) = env.jobs.clone() {
        let _ = repo.record_heartbeat("dropit-workers").await;
        if matches!(cmd, Command::Run) {
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                    let _ = repo.record_heartbeat("dropit-workers").await;
                }
            });
        }
    }

    let code = match cmd {
        Command::Run => jobs::run_supervisor(env).await,
        Command::Once(job) => {
            let report = dispatch_once(&env, &job).await;
            println!("{}", report.to_line());
            report.exit_code()
        }
    };
    std::process::exit(code);
}

/// Инициализация tracing-subscriber из env (`RUST_LOG`).
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

/// Диспетчер одноразового прогона по имени задачи.
async fn dispatch_once(env: &WorkerEnv, job: &str) -> JobReport {
    match job {
        "outbox" => jobs::run_outbox_env(env).await,
        "payouts" => {
            jobs::run_payouts_with_mail(env.jobs.as_ref(), env.payouts.as_ref(), env.mail.as_ref())
                .await
        }
        "reconcile" => jobs::run_reconcile_once(env.payouts.as_ref()).await,
        "retention" => jobs::run_retention_once(env.jobs.as_ref()).await,
        "close-sessions" => jobs::run_close_sessions_once(env.jobs.as_ref()).await,
        "push" => jobs::run_push_once(env).await,
        "mail" => jobs::run_mail_once(env).await,
        other => JobReport::error("unknown", format!("неизвестная задача `{other}`")),
    }
}
