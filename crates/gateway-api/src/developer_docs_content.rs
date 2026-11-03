//! GENERATED FILE — do not edit. Run `node scripts/build-api-docs.mjs`.
//!
//! Секции документации Developer API v1 (ADR-010). Источник правды —
//! `docs/api/sections/*.md`: front matter становится полями структуры,
//! тело встраивается в бинарник через `include_str!`. Ручные правки этого
//! файла затираются при следующей сборке, а `--check` в гейте падает.

/// Одна секция документации.
pub struct DocSection {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    /// Скоупы, без которых секция не видна ключу.
    pub requires: &'static [&'static str],
    /// Порядок в меню (меньше — выше).
    pub order: i32,
    /// Markdown вместе с front matter; хендлер отрезает front matter.
    pub markdown: &'static str,
}

/// Все секции в порядке меню (`order`).
pub const SECTIONS: &[DocSection] = &[
    DocSection {
        id: "overview",
        title: "Overview",
        summary: "What the Developer API v1 can automate, who it is for, and the conventions every endpoint shares.",
        requires: &[],
        order: 10,
        markdown: include_str!("../../../docs/api/sections/overview.md"),
    },
    DocSection {
        id: "quickstart",
        title: "Quickstart",
        summary: "Create a key in the panel, make your first calls with curl, TypeScript or Python, and place a request without duplicates.",
        requires: &[],
        order: 20,
        markdown: include_str!("../../../docs/api/sections/quickstart.md"),
    },
    DocSection {
        id: "authentication",
        title: "Authentication",
        summary: "Key format, both supported headers, the 401 codes, rotation, revocation, and why a key never belongs in a browser.",
        requires: &[],
        order: 30,
        markdown: include_str!("../../../docs/api/sections/authentication.md"),
    },
    DocSection {
        id: "scopes",
        title: "Scopes",
        summary: "The nine permissions a key can carry, the four presets, and how to keep a key at the minimum it needs.",
        requires: &[],
        order: 40,
        markdown: include_str!("../../../docs/api/sections/scopes.md"),
    },
    DocSection {
        id: "errors",
        title: "Errors",
        summary: "The single error envelope, every code the API can return, and how to retry safely.",
        requires: &[],
        order: 50,
        markdown: include_str!("../../../docs/api/sections/errors.md"),
    },
    DocSection {
        id: "rate-limits",
        title: "Rate limits",
        summary: "One rolling 60-second window per key, the headers that report it, and a backoff that behaves.",
        requires: &[],
        order: 60,
        markdown: include_str!("../../../docs/api/sections/rate-limits.md"),
    },
    DocSection {
        id: "pagination",
        title: "Pagination",
        summary: "Opaque cursors, stable descending order, 1–100 items per page, and a loop that never duplicates or skips.",
        requires: &[],
        order: 70,
        markdown: include_str!("../../../docs/api/sections/pagination.md"),
    },
    DocSection {
        id: "versioning",
        title: "Versioning",
        summary: "What /v1 promises, which changes are additive, and the six-month deprecation window.",
        requires: &[],
        order: 80,
        markdown: include_str!("../../../docs/api/sections/versioning.md"),
    },
    DocSection {
        id: "profile",
        title: "Profile",
        summary: "GET /v1/developer/profile — the club fields you can rely on to map a DROP IT club to a venue record.",
        requires: &["profile:read"],
        order: 90,
        markdown: include_str!("../../../docs/api/sections/profile.md"),
    },
    DocSection {
        id: "sessions",
        title: "Sessions",
        summary: "Create, inspect and close a night, issue the guest QR, and stay on the right side of the session state machine.",
        requires: &["sessions:read"],
        order: 100,
        markdown: include_str!("../../../docs/api/sections/sessions.md"),
    },
    DocSection {
        id: "requests",
        title: "Requests",
        summary: "The live queue, placing requests idempotently, moderating them, and the privacy line the API never crosses.",
        requires: &["requests:read"],
        order: 110,
        markdown: include_str!("../../../docs/api/sections/requests.md"),
    },
    DocSection {
        id: "catalog",
        title: "Catalog",
        summary: "Search the track catalog, understand the hit shape and the canonical key, and cache results without going stale.",
        requires: &["catalog:read"],
        order: 120,
        markdown: include_str!("../../../docs/api/sections/catalog.md"),
    },
    DocSection {
        id: "stats",
        title: "Stats",
        summary: "One aggregated overview per period — sessions, requests, captured revenue in minor units, top tracks.",
        requires: &["stats:read"],
        order: 130,
        markdown: include_str!("../../../docs/api/sections/stats.md"),
    },
    DocSection {
        id: "security",
        title: "Security",
        summary: "How keys are stored and verified, what each control buys you, and what to do when one leaks.",
        requires: &[],
        order: 140,
        markdown: include_str!("../../../docs/api/sections/security.md"),
    },
    DocSection {
        id: "recipes",
        title: "Recipes",
        summary: "Three automations you can ship — queue triage with a nightly close, POS-placed requests, and a nightly revenue reconciliation.",
        requires: &[],
        order: 150,
        markdown: include_str!("../../../docs/api/sections/recipes.md"),
    },
    DocSection {
        id: "changelog",
        title: "Changelog",
        summary: "Releases of the public Developer API, starting with 1.0.0 on 2026-10-28.",
        requires: &[],
        order: 160,
        markdown: include_str!("../../../docs/api/sections/changelog.md"),
    },
];
