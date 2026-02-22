use std::sync::Arc;

use axum::{extract::State, response::IntoResponse};

use crate::{dashboard::Dashboard, vessel::AppState};

pub async fn list(State(state): State<Arc<AppState>>) -> impl axum::response::IntoResponse {
    let dashboards: Vec<Dashboard> = state.dashboard_store.list_dashboards();

    axum::Json(dashboards).into_response()
}
