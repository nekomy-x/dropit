//! Документация Developer API v1 (ADR-010): один источник правды для
//! панели, `/v1/developer/docs` и фильтрации OpenAPI по скоупам ключа.
//!
//! Секции — `docs/api/sections/*.md` с YAML front matter, скомпилированные
//! `scripts/build-api-docs.mjs` в [`developer_docs_content`]. Полотно
//! OpenAPI — `docs/api/openapi-developer.yaml`. Оба файла попадают в
//! бинарник на этапе сборки, поэтому задеплоенная документация — это
//! документация, с которой собран бинарник.

use std::sync::OnceLock;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use dropit_common::{ApiError, Role, Scope, SCOPE_CATALOGUE, SCOPE_PRESETS};
use serde_json::{json, Value};

use crate::developer_docs_content::{DocSection, SECTIONS};
use crate::rbac::{auth_error_response, AuthContext};
use crate::routes::ApiEnv;

/// Полотно контракта Developer API (встраивается в бинарник).
pub const DEVELOPER_OPENAPI_YAML: &str = include_str!("../../../docs/api/openapi-developer.yaml");

/// Версия публичного API (совпадает с `info.version` контракта).
pub const DEVELOPER_API_VERSION: &str = "1.0.0";

/// Все секции документации (порядок задан front matter `order`).
pub fn sections() -> &'static [DocSection] {
    SECTIONS
}

/// Доступна ли секция при данном наборе скоупов.
pub fn section_allowed(section: &DocSection, granted: &[Scope]) -> bool {
    section.requires.iter().all(|required| {
        Scope::parse(required)
            .map(|scope| granted.contains(&scope))
            .unwrap_or(false)
    })
}

/// Сериализация каталога скоупов для панели и доков.
pub fn scope_catalogue_json() -> Value {
    Value::Array(
        SCOPE_CATALOGUE
            .iter()
            .map(|info| {
                json!({
                    "id": info.scope.as_str(),
                    "title": info.title,
                    "description": info.description,
                    "group": info.group,
                    "risk": info.risk,
                    "docs_section": info.docs_section,
                })
            })
            .collect(),
    )
}

/// Сериализация пресетов.
pub fn scope_presets_json() -> Value {
    Value::Array(
        SCOPE_PRESETS
            .iter()
            .map(|preset| {
                json!({
                    "id": preset.id,
                    "title": preset.title,
                    "description": preset.description,
                    "scopes": preset.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// Манифест документации. `filter = None` — всё (панель), `Some` — только
/// секции, разрешённые скоупами ключа (Developer API).
pub fn manifest(filter: Option<&[Scope]>) -> Value {
    let sections: Vec<Value> = sections()
        .iter()
        .filter(|section| match filter {
            Some(scopes) => section_allowed(section, scopes),
            None => true,
        })
        .map(|section| {
            json!({
                "id": section.id,
                "title": section.title,
                "summary": section.summary,
                "requires": section.requires,
                "order": section.order,
            })
        })
        .collect();
    json!({
        "api_version": DEVELOPER_API_VERSION,
        "base_url": "/v1/developer",
        "openapi_url": "/v1/developer/openapi.json",
        "sections": sections,
        "scopes": scope_catalogue_json(),
        "presets": scope_presets_json(),
    })
}

/// Полное тело секции (markdown без front matter).
pub fn section_payload(id: &str) -> Option<Value> {
    sections()
        .iter()
        .find(|section| section.id == id)
        .map(|section| {
            json!({
                "id": section.id,
                "title": section.title,
                "summary": section.summary,
                "requires": section.requires,
                "markdown": strip_front_matter(section.markdown),
            })
        })
}

/// Отрезает YAML front matter (`---` … `---`), оставляя чистый markdown.
pub fn strip_front_matter(raw: &str) -> String {
    let trimmed = raw.trim_start_matches('\u{feff}');
    if let Some(rest) = trimmed.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            return rest[end + 5..].trim_start().to_string();
        }
    }
    trimmed.trim_start().to_string()
}

/// OpenAPI как JSON: полный (`None`) или отфильтрованный по скоупам
/// (`Some`) — ключ с `docs:read` видит только те операции, которые ему
/// разрешены, чужие маршруты в ответ не попадают.
pub fn openapi_json(filter: Option<&[Scope]>) -> Value {
    static SPEC: OnceLock<Value> = OnceLock::new();
    let full = SPEC.get_or_init(|| {
        serde_yaml::from_str::<Value>(DEVELOPER_OPENAPI_YAML).unwrap_or_else(|err| {
            tracing::error!(error = %err, "developer openapi yaml is not valid");
            json!({
                "openapi": "3.1.0",
                "info": { "title": "DROP IT Developer API", "version": DEVELOPER_API_VERSION },
                "paths": {},
            })
        })
    });
    match filter {
        None => full.clone(),
        Some(scopes) => filter_spec(full, scopes),
    }
}

fn filter_spec(full: &Value, scopes: &[Scope]) -> Value {
    let mut filtered = full.clone();
    let allowed: Vec<&str> = scopes.iter().map(|scope| scope.as_str()).collect();
    if let Some(paths) = filtered.get_mut("paths").and_then(Value::as_object_mut) {
        paths.retain(|_path, item| {
            let Some(ops) = item.as_object_mut() else {
                return false;
            };
            ops.retain(|key, op| {
                let is_method = matches!(
                    key.as_str(),
                    "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
                );
                if !is_method {
                    return true; // path-level parameters/summary
                }
                // Операции вида `/me` требуют только валидный ключ:
                // контракт помечает их `x-no-scope: true` явно, чтобы
                // случайно не появился маршрут без объявленного доступа.
                if op
                    .get("x-no-scope")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    return true;
                }
                op.get("x-required-scope")
                    .and_then(Value::as_str)
                    .map(|required| allowed.contains(&required))
                    .unwrap_or(false)
            });
            ops.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
                )
            })
        });
    }
    filtered["x-granted-scopes"] = json!(allowed);
    filtered
}

// ============================================================
// Панель клуба: документация без фильтра (по JWT клуба)
// ============================================================

pub fn panel_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/club/developers/docs", get(panel_manifest))
        .route("/v1/club/developers/docs/:section", get(panel_section))
        .route("/v1/club/developers/openapi.json", get(panel_openapi))
}

fn club_only(ctx: &AuthContext) -> Option<Response> {
    if ctx.role() == Role::Club {
        None
    } else {
        Some(auth_error_response(ApiError::Forbidden))
    }
}

async fn panel_manifest(ctx: AuthContext, State(_state): State<ApiEnv>) -> Response {
    if let Some(denied) = club_only(&ctx) {
        return denied;
    }
    (StatusCode::OK, Json(manifest(None))).into_response()
}

async fn panel_section(
    ctx: AuthContext,
    State(_state): State<ApiEnv>,
    Path(id): Path<String>,
) -> Response {
    if let Some(denied) = club_only(&ctx) {
        return denied;
    }
    match section_payload(&id) {
        Some(payload) => (StatusCode::OK, Json(payload)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(dropit_common::ErrorEnvelope {
                code: "not_found".into(),
                message: "Documentation section not found.".into(),
                details: None,
            }),
        )
            .into_response(),
    }
}

async fn panel_openapi(ctx: AuthContext, State(_state): State<ApiEnv>) -> Response {
    if let Some(denied) = club_only(&ctx) {
        return denied;
    }
    (StatusCode::OK, Json(openapi_json(None))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_is_stripped() {
        let raw = "---\nid: x\ntitle: X\n---\n\n# Hello\n\nBody.\n";
        assert_eq!(strip_front_matter(raw), "# Hello\n\nBody.\n");
        assert_eq!(strip_front_matter("# No front matter"), "# No front matter");
    }

    #[test]
    fn scopes_filter_spec_operations() {
        let doc = json!({
            "openapi": "3.1.0",
            "paths": {
                "/v1/developer/me": { "get": { "x-no-scope": true } },
                "/v1/developer/profile": { "get": { "x-required-scope": "profile:read" } },
                "/v1/developer/docs": { "get": { "x-required-scope": "docs:read" } },
                "/v1/developer/stats/overview": { "get": { "x-required-scope": "stats:read" } }
            }
        });
        let filtered = filter_spec(&doc, &[Scope::DocsRead]);
        let paths = filtered["paths"].as_object().unwrap();
        assert!(paths.contains_key("/v1/developer/me"));
        assert!(paths.contains_key("/v1/developer/docs"));
        assert!(!paths.contains_key("/v1/developer/profile"));
        assert!(!paths.contains_key("/v1/developer/stats/overview"));
        assert_eq!(filtered["x-granted-scopes"], json!(["docs:read"]));
    }
}
