//! DROP IT workers: фоновые задачи (outbox relay, выплаты, сверка, retention,
//! автозакрытие сессий, push-диспетчер).
//!
//! Запуск: `dropit-workers --once <job>` — один прогон (CI/крон),
//! `dropit-workers run` — супервизор с расписанием.

fn main() {
    eprintln!("dropit-workers: skeleton (W3-C)");
}
