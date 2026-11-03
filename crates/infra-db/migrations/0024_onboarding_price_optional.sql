-- DROP IT — миграция 0024: цена в заявке клуба становится необязательной.
--
-- Зачем: компактная форма `/register` больше не спрашивает «Цена за гостя, €».
-- Тариф клуб задаёт в панели (`/pricing`) после активации аккаунта, поэтому
-- заявка может не называть цену вовсе. Раньше `price_minor` был NOT NULL с
-- CHECK 100..100000, и заявке без цены пришлось бы выдумывать число — эта
-- выдуманная цифра попала бы в карточку модерации как «выбор клуба».
--
-- Визард `/connect` цену по-прежнему присылает, поэтому CHECK остаётся: те же
-- границы €1..€1000, но теперь он разрешает NULL.
--
-- DDL идемпотентен: стенд применяет миграции как `psql -f`, воркер — через
-- sqlx Migrator; повторный прогон не должен падать на уже изменённых объектах.

ALTER TABLE club_application
    ALTER COLUMN price_minor DROP NOT NULL;

ALTER TABLE club_application
    DROP CONSTRAINT IF EXISTS club_application_price_range;

ALTER TABLE club_application
    ADD CONSTRAINT club_application_price_range
        CHECK (price_minor IS NULL OR price_minor BETWEEN 100 AND 100000);
