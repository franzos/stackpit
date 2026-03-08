use axum::response::IntoResponse;

use crate::queries;

use super::json_or_500;
use crate::extractors::ApiReadPool;

/// GET /api/0/projects/
pub async fn list(ApiReadPool(pool): ApiReadPool) -> impl IntoResponse {
    json_or_500(queries::projects::list_projects(&pool, None, None, None).await)
}
