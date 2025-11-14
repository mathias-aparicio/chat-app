use crate::db::create_table;
use axum::Router;
use axum::extract::{Json, State};
use axum::routing::{get, post};
use axum_anyhow::{ApiError, ApiResult, ResultExt};
use chrono::{DateTime, Utc};
use scylla::client::session::Session;
use scylla::{DeserializeRow, SerializeRow};
use std::sync::Arc;
use uuid::Uuid;
#[derive(Clone)]
struct AppState {
    session: Arc<Session>,
}

#[derive(serde::Deserialize)]
struct CreateUser {
    username: String,
}

#[derive(serde::Deserialize, serde::Serialize, SerializeRow, Clone, DeserializeRow, Debug)]
struct User {
    user_id: Uuid,
    username: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[axum::debug_handler]
async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUser>,
) -> ApiResult<Json<User>> {
    let now = Utc::now();
    let user_id = Uuid::new_v4();
    state
        .session
        .query_unpaged(
            "INSERT INTO ks.users (user_id, username, created_at, updated_at) VALUES (?, ?, ?, ?)",
            (user_id, payload.username, now, now),
        )
        .await
        .context_internal("Failed to insert user", "")?;

    let user: User = state
        .session
        .query_unpaged("SELECT * FROM ks.users WHERE user_id = ?", (user_id,))
        .await
        .context_internal("Failed to fetch the user insert", "")?
        .into_rows_result()
        .context_internal("Failed to convert result to row", "")?
        .first_row()
        .context_internal("Failed to get first row", "")?;

    Ok(Json(user))
}

async fn get_members() {
    todo!()
}

pub async fn create_router() -> anyhow::Result<Router> {
    let session = create_table().await?;
    // TODO : Add Mutex ?
    let session = Arc::new(session);
    let app_state = AppState { session };
    let app = Router::new()
        .route("/users", post(create_user))
        // .route("/chats", post(create_user))
        .route("/chats/{chatId}/members", get(get_members))
        .route("/chats/{chatId}/messages", post(get_members))
        .route("/chats/{chatId}/messages", get(get_members))
        .route("/ws/connect/{userId}", get(get_members))
        .with_state(app_state);
    Ok(app)
}
