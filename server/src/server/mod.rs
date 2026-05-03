pub mod errors;
pub mod routes;
pub mod session;
pub mod state;

use axum::Router;
use state::AppState;
pub fn build_router(state: AppState) -> Router {
    routes::build_routes(state)
}
