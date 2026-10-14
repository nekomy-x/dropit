//! Seed демо-данных для тестового запуска (Test Mode, мок-Stripe).
//!
//! Создаёт: клуб, DJ, админа (с известным TOTP-секретом), трек.
//! Пароли: см. константы ниже — для «потыкать», НЕ прод.
//!
//! Запуск:
//! ```sh
//! cargo run -p dropit-gateway-api --bin seed_demo
//! ```
//! (или через основной бинари с --features см. README).

use uuid::Uuid;

fn main() -> anyhow::Result<()> {
    // Параметры демо.
    const CLUB_EMAIL: &str = "demo@club.test";
    const CLUB_PASSWORD: &str = "demo-club-pass";
    const DJ_EMAIL: &str = "demo@dj.test";
    const DJ_PASSWORD: &str = "demo-dj-pass";
    const ADMIN_EMAIL: &str = "demo@admin.test";
    const ADMIN_PASSWORD: &str = "demo-admin-pass";
    /// base32 TOTP-секрет админа (Google Authenticator: добавить
    /// вручную по этому ключу).
    const ADMIN_TOTP_BASE32: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let db_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL required (see README: postgres://dropit:dropit_local@localhost:5432/dropit)");
        let pool = dropit_infra_db::connect(&db_url).await?;

        // --- Клуб -------------------------------------------------
        let club_id = Uuid::now_v7();
        let club_hash = dropit_gateway_api::hash_password(CLUB_PASSWORD)
            .map_err(anyhow::Error::msg)?;
        sqlx::query(
            "INSERT INTO club (id, name, slug, email, password_hash, status) \
             VALUES ($1, 'Demo Night Club', 'demo-club', $2, $3, 'active') \
             ON CONFLICT ON CONSTRAINT club_email_key DO UPDATE SET password_hash = EXCLUDED.password_hash",
        )
        .bind(club_id)
        .bind(CLUB_EMAIL)
        .bind(&club_hash)
        .execute(&pool)
        .await?;
        let club_id: (Uuid,) =
            sqlx::query_as("SELECT id FROM club WHERE email = $1")
                .bind(CLUB_EMAIL)
                .fetch_one(&pool)
                .await?;
        let club_id = club_id.0;

        // --- DJ ---------------------------------------------------
        let dj_hash = dropit_gateway_api::hash_password(DJ_PASSWORD)
            .map_err(anyhow::Error::msg)?;
        sqlx::query(
            "INSERT INTO dj (id, club_id, email, password_hash, display_name, status) \
             VALUES ($1, $2, $3, $4, 'DJ Demo', 'active') \
             ON CONFLICT ON CONSTRAINT dj_email_key DO UPDATE SET password_hash = EXCLUDED.password_hash",
        )
        .bind(Uuid::now_v7())
        .bind(club_id)
        .bind(DJ_EMAIL)
        .bind(&dj_hash)
        .execute(&pool)
        .await?;

        // --- Админ (TOTP обязателен с F1) -------------------------
        let admin_hash = dropit_gateway_api::hash_password(ADMIN_PASSWORD)
            .map_err(anyhow::Error::msg)?;
        let totp_enc = dropit_gateway_api::encrypt_totp_secret_for_seed(ADMIN_TOTP_BASE32)
            .map_err(anyhow::Error::msg)?;
        sqlx::query(
            "INSERT INTO admin_user (id, email, password_hash, totp_secret_enc, totp_confirmed_at) \
             VALUES ($1, $2, $3, $4, now()) \
             ON CONFLICT ON CONSTRAINT admin_user_email_key DO UPDATE SET password_hash = EXCLUDED.password_hash, totp_secret_enc = EXCLUDED.totp_secret_enc",
        )
        .bind(Uuid::now_v7())
        .bind(ADMIN_EMAIL)
        .bind(&admin_hash)
        .bind(&totp_enc)
        .execute(&pool)
        .await?;

        // --- Треки для демо-поиска ---------------------------------
        for (title, artist, key) in [
            ("Vivir Mi Vida", "Marc Anthony", "itunes:demo-vivir"),
            ("Bailando", "Enrique Iglesias", "itunes:demo-bailando"),
            ("Despacito", "Luis Fonsi", "itunes:demo-despacito"),
            ("Taki Taki", "DJ Snake", "itunes:demo-taki"),
            ("Gasolina", "Daddy Yankee", "itunes:demo-gasolina"),
        ] {
            sqlx::query(
                "INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) \
                 VALUES ($1, 'itunes', $2, $3, $4, $5) \
                 ON CONFLICT ON CONSTRAINT track_cache_source_source_id_key DO NOTHING",
            )
            .bind(Uuid::now_v7())
            .bind(key)
            .bind(key)
            .bind(title)
            .bind(artist)
            .execute(&pool)
            .await?;
        }

        println!("✅ Seed готов (Test Mode, мок-Stripe)");
        println!("   Клуб:      {CLUB_EMAIL} / {CLUB_PASSWORD}");
        println!("   DJ:        {DJ_EMAIL} / {DJ_PASSWORD}");
        println!("   Админ:     {ADMIN_EMAIL} / {ADMIN_PASSWORD}");
        println!("   TOTP-ключ админа (введите в приложение-аутентификатор):");
        println!("   {ADMIN_TOTP_BASE32}");
        println!("   Треки: 5 демо-записей в track_cache");
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}
