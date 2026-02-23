use std::sync::Arc;

use axum::{Json, extract::Path, extract::State, response::IntoResponse};
use reqwest::{StatusCode, header};
use serde_json::json;

use crate::{dashboard::Dashboard, vessel::AppState};

pub async fn list(State(state): State<Arc<AppState>>) -> impl axum::response::IntoResponse {
    let dashboards: Vec<Dashboard> = state.dashboard_store.list_dashboards();

    axum::Json(dashboards).into_response()
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Dashboard>, (StatusCode, Json<serde_json::Value>)> {
    match state.dashboard_store.get_dashboard(&id) {
        Some(dashboard) => Ok(Json(dashboard)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Dashboard not found" })),
        )),
    }
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(dashboard): Json<Dashboard>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .dashboard_store
        .save_dashboard(&dashboard)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/dashboards/{}", dashboard.id))],
        Json(dashboard),
    ))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(dashboard): Json<Dashboard>,
) -> Result<Json<Dashboard>, (StatusCode, Json<serde_json::Value>)> {
    if state.dashboard_store.get_dashboard(&id).is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Dashboard not found" })),
        ));
    }

    state
        .dashboard_store
        .save_dashboard(&dashboard)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to save dashboard" })),
            )
        })?;

    Ok(Json(dashboard))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    state.dashboard_store.delete_dashboard(&id).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to delete dashboard" })),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}
