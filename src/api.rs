use std::sync::Arc;

use axum::{Router, routing::get};

use crate::vessel::AppState;

pub mod dashboards;

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
}
