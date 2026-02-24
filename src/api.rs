use std::sync::Arc;

use axum::{Router, routing::get};

use crate::vessel::AppState;

pub mod dashboards;
pub mod modules;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/dashboards",
            get(dashboards::list).post(dashboards::create),
        )
        .route(
            "/dashboards/:id",
            get(dashboards::get)
                .put(dashboards::update)
                .delete(dashboards::delete),
        )
        .route("/modules", get(modules::list_modules))
        .route("/modules/version", get(modules::api_version))
}
