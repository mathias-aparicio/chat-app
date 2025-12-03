use crate::AppState;
use crate::NODE_ID;
use crate::schema::{Chat, CreatMessage, CreateChat, CreateUser, LoginPayload, PandaMessage, User};
use crate::websocket::handle_socket;
use anyhow::Context;
use anyhow::anyhow;
use axum::extract::WebSocketUpgrade;
use axum::{
    Router,
    extract::{Form, Json, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::CookieJar;
use serde::Serialize;
use std::str::FromStr;
use tower_http::{services::ServeDir, trace::TraceLayer};
use uuid::Uuid;

pub async fn create_router(state: AppState) -> anyhow::Result<Router> {
    let app = Router::new()
        // UI routes
        .route("/", get(render_index))
        .route("/dashboard", get(dashboard))
        .route("/ui/chats/{chat_id}", get(render_chat))
        // API routes
        .route("/logout", get(logout))
        .route("/login", post(login))
        .route("/users", post(create_user))
        .route("/chats", post(create_chat))
        .route("/users/{user_id}", get(get_user))
        .route("/chats/{chat_id}", get(get_chat))
        .route("/chats/{chat_id}/messages", post(post_message))
        .route("/chats/{chat_id}/messages", get(get_messages))
        .route("/ws/connect/{user_id}", get(get_websocket))
        // Serving static file
        .nest_service("/static", ServeDir::new("static"))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    Ok(app)
}
// ----- **** ----
// Begin of handler definition
// ----- **** ----

async fn get_websocket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let user_id = Uuid::from_str(&user_id)?;
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, user_id)))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(payload): Form<LoginPayload>,
) -> ApiResult<impl IntoResponse> {
    let user = state.db.get_user_by_username(&payload.username).await?;

    let mut cookie = axum_extra::extract::cookie::Cookie::new("user_id", user.user_id.to_string());
    cookie.set_path("/");

    let jar = jar.add(cookie);
    Ok((jar, Redirect::to("/")))
}
async fn logout(State(_state): State<AppState>, jar: CookieJar) -> ApiResult<impl IntoResponse> {
    let mut cookie = axum_extra::extract::cookie::Cookie::new("user_id", "");
    cookie.set_path("/");

    cookie.make_removal();
    let jar = jar.add(cookie);
    Ok((jar, Redirect::to("/")))
}
async fn get_messages(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> ApiResult<JsonWithStatus<Vec<PandaMessage>>> {
    let chat_id = Uuid::from_str(&chat_id)?;
    let messages = state.db.get_messages(chat_id).await?;
    Ok(JsonWithStatus {
        data: messages,
        status: StatusCode::OK,
    })
}
async fn render_index(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<impl IntoResponse> {
    if jar.get("user_id").is_some() {
        return Ok(Redirect::to("/dashboard").into_response());
    }
    let context = tera::Context::new();
    let rendered = state
        .tera
        .render("login.html", &context)
        .map_err(|e| anyhow!("Template rendering failed: {}", e))?;

    Ok(Html(rendered).into_response())
}

async fn dashboard(State(state): State<AppState>, jar: CookieJar) -> ApiResult<Html<String>> {
    let user_id_str = jar
        .get("user_id")
        .ok_or(anyhow!("No user_id cookie found"))?
        .value();
    let current_user_id = Uuid::from_str(user_id_str)?;

    let chats = state.db.get_chats_for_user(current_user_id).await?;

    let all_users = state.db.get_all_users().await?;
    let current_user = all_users
        .iter()
        .find(|u| u.user_id == current_user_id)
        .ok_or(anyhow!("Current user not found in database"))?;

    let available_users: Vec<&User> = all_users
        .iter()
        .filter(|u| u.user_id != current_user_id)
        .collect();

    let mut context = tera::Context::new();
    context.insert("chats", &chats);
    context.insert("available_users", &available_users);
    context.insert("current_user", &current_user);

    let rendered = state
        .tera
        .render("dashboard.html", &context)
        .map_err(|e| anyhow!("Template rendering failed: {}", e))?;

    Ok(Html(rendered))
}

async fn create_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    // axum_extra form support multiple values
    axum_extra::extract::Form(payload): axum_extra::extract::Form<CreateChat>,
) -> ApiResult<Response> {
    let user_id = jar
        .get("user_id")
        .ok_or(anyhow!("No user_id cookie found"))?
        .value();
    let user_id = Uuid::from_str(user_id)?;

    let mut members = payload.members.clone();

    if !members.contains(&user_id) {
        members.push(user_id);
    }

    let chat: Chat = state.db.create_chat(&payload.name, &members).await?;

    if headers.contains_key("hx-request") {
        let mut context = tera::Context::new();
        context.insert("chat", &chat);
        let rendered = state
            .tera
            .render("partials/chat_item.html", &context)
            .map_err(|e| anyhow!("Template rendering failed: {}", e))?;
        return Ok(Html(rendered).into_response());
    }

    Ok(JsonWithStatus {
        status: StatusCode::CREATED,
        data: chat,
    }
    .into_response())
}

async fn post_message(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    headers: HeaderMap,
    jar: CookieJar,
    Form(create_message): Form<CreatMessage>,
) -> ApiResult<impl IntoResponse> {
    let sender_id = jar
        .get("user_id")
        .ok_or(anyhow!("Failed to get user_id from cookie"))?
        .value_trimmed();

    let content = create_message.content;
    let sender_id = Uuid::parse_str(sender_id)?;
    let chat_id = Uuid::parse_str(&chat_id)?;
    let message_id = Uuid::now_v1(&NODE_ID);
    state
        .producer
        .send_message(PandaMessage {
            sender_id,
            content,
            chat_id,
            message_id,
        })
        .await?;

    if headers.contains_key("hx-request") {
        return Ok(StatusCode::OK.into_response());
    }
    Ok(StatusCode::OK.into_response())
}
async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Form(payload): Form<CreateUser>,
) -> ApiResult<Response> {
    match state.db.create_user(&payload.username).await {
        Ok(user) => {
            if headers.contains_key("hx-request") {
                let jar = jar.add(axum_extra::extract::cookie::Cookie::new(
                    "user_id",
                    user.user_id.to_string(),
                ));
                // HTMX redirect via header or just redirect
                return Ok((jar, Redirect::to("/dashboard")).into_response());
            }
            Ok(JsonWithStatus {
                status: StatusCode::CREATED,
                data: user,
            }
            .into_response())
        }
        Err(e) => {
            if headers.contains_key("hx-request") {
                // Return error message div
                return Ok(Html(format!(
                    "<div class='error'>Error creating user: {}</div>",
                    e
                ))
                .into_response());
            }
            Err(e.into())
        }
    }
}
async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> ApiResult<JsonWithStatus<User>> {
    let user_id = Uuid::from_str(&user_id).context("Failed to parse user_id from str to UUID")?;
    let user = state.db.get_user(user_id).await?;
    Ok(JsonWithStatus {
        status: StatusCode::OK,
        data: user,
    })
}
async fn get_chat(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> ApiResult<JsonWithStatus<Chat>> {
    let chat_id = Uuid::from_str(&chat_id).context("Failed to parse chat_id from str to UUID")?;
    let chat = state.db.get_chat(chat_id).await?;
    Ok(JsonWithStatus {
        status: StatusCode::OK,
        data: chat,
    })
}

async fn render_chat(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    jar: CookieJar,
) -> ApiResult<Html<String>> {
    let user_id_str = jar
        .get("user_id")
        .ok_or(anyhow!("No user_id cookie found"))?
        .value();
    let current_user_id = Uuid::from_str(user_id_str)?;

    let chat_id_uuid =
        Uuid::from_str(&chat_id).context("Failed to parse chat_id from str to UUID")?;
    let chat = state.db.get_chat(chat_id_uuid).await?;
    let messages = state.db.get_messages(chat_id_uuid).await?;

    let mut context = tera::Context::new();
    context.insert("chat", &chat);
    context.insert("messages", &messages);
    context.insert("user_id", &current_user_id);

    let rendered = state
        .tera
        .render("chat.html", &context)
        .map_err(|e| anyhow!("Template rendering failed: {}", e))?;

    Ok(Html(rendered))
}

// ----- **** ----
// // End of handler definition
// ----- **** ----

pub struct JsonWithStatus<T> {
    pub status: StatusCode,
    pub data: T,
}

impl<T> IntoResponse for JsonWithStatus<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        (self.status, Json(self.data)).into_response()
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("Application error: {:#}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
pub struct AppError(anyhow::Error);

pub type ApiResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::producer::MockProducer;
    use crate::{AppState, ConnectionMap};
    use anyhow::Result;
    use async_trait::async_trait;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{self, Request, StatusCode},
        response::Response,
        routing::{get, post},
    };
    use chrono::Utc;
    use mockall::mock;
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tera::Tera;
    use tokio::sync::RwLock;
    use tower::ServiceExt;
    use uuid::Uuid;

    // --- MOCK DEFINITIONS ---
    mock! {
        pub Db {}
        #[async_trait]
        impl Db for Db {
            async fn create_user(&self, username: &str) -> Result<User>;
            async fn create_chat(&self, name: &str, members: &[Uuid]) -> Result<Chat>;
            async fn get_user(&self, user_id: Uuid) -> Result<User>;
            async fn get_chat(&self, chat_id: Uuid) -> Result<Chat>;
            async fn insert_message(&self, message: PandaMessage) -> Result<crate::schema::PandaMessage>;
            async fn get_messages(&self, chat_id: Uuid) -> Result<Vec<PandaMessage>>;
            async fn get_chats_for_user(&self, user_id: Uuid) -> Result<Vec<Chat>>;
            async fn get_user_by_username(&self, username: &str) -> Result<User>;
            async fn get_all_users(&self) -> Result<Vec<User>>;
            async fn get_members_of_chat(&self, chat_id: Uuid) -> Result<Vec<Uuid>>;
        }
    }

    // --- TESTS ---

    #[tokio::test]
    async fn test_create_user() {
        let mut mock_db = MockDb::new();
        let user_id = new_uuid();
        let now = Utc::now();
        let expected_user = User {
            user_id,
            username: "test_user".to_string(),
            created_at: now,
            updated_at: now,
        };

        // Setup Expectations
        let user_clone = expected_user.clone();
        mock_db
            .expect_create_user()
            .withf(|username| username == "test_user")
            .times(1)
            .returning(move |_| Ok(user_clone.clone()));

        // Setup App
        let state = create_test_state(mock_db, MockProducer::new());
        let app = Router::new()
            .route("/users", post(create_user))
            .with_state(state);

        // Execute
        // CHANGE: Use build_form_request instead of build_json_request because the handler
        // uses Form extractor.
        let req = build_form_request("/users", "username=test_user".to_string(), None);
        let response = app.oneshot(req).await.unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::CREATED);
        let user: User = deserialize_body(response).await;
        assert_eq!(user, expected_user);
    }

    #[tokio::test]
    async fn test_create_chat() {
        let mut mock_db = MockDb::new();
        let chat_id = new_uuid();
        let now = Utc::now();

        let member1 = new_uuid();
        let member2 = new_uuid();
        let current_user_id = new_uuid(); // Use a fixed ID for the cookie

        // The handler adds the current user (from cookie) to the members list.
        // So we expect the DB to receive [member1, member2, current_user_id]
        let mut expected_members = vec![member1, member2];
        expected_members.push(current_user_id);

        let expected_chat = Chat {
            chat_id,
            name: "test_chat".to_string(),
            members: expected_members.clone(),
            created_at: now,
        };

        // Setup Expectations
        let chat_clone = expected_chat.clone();
        let members_check = expected_members.clone();

        mock_db
            .expect_create_chat()
            .withf(move |name, m| name == "test_chat" && m == members_check)
            .times(1)
            .returning(move |_, _| Ok(chat_clone.clone()));

        // Setup App
        let state = create_test_state(mock_db, MockProducer::new());
        let app = Router::new()
            .route("/chats", post(create_chat))
            .with_state(state);

        // Execute
        let cookie = format!("user_id={}", current_user_id);
        let body_str = format!("name=test_chat&members={}&members={}", member1, member2);
        let req = build_form_request("/chats", body_str, Some(&cookie));

        let response = app.oneshot(req).await.unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::CREATED);
        let chat: Chat = deserialize_body(response).await;
        assert_eq!(chat, expected_chat);
    }

    #[tokio::test]
    async fn test_get_user() {
        let mut mock_db = MockDb::new();
        let user_id = new_uuid();
        let now = Utc::now();
        let expected_user = User {
            user_id,
            username: "test_user".to_string(),
            created_at: now,
            updated_at: now,
        };

        // Setup Expectations
        let user_clone = expected_user.clone();
        mock_db
            .expect_get_user()
            .withf(move |id| *id == user_id)
            .times(1)
            .returning(move |_| Ok(user_clone.clone()));

        // Setup App
        let state = create_test_state(mock_db, MockProducer::new());
        let app = Router::new()
            .route("/users/{user_id}", get(get_user))
            .with_state(state);

        // Execute
        let req = build_get_request(&format!("/users/{}", user_id));
        let response = app.oneshot(req).await.unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::OK);
        let user: User = deserialize_body(response).await;
        assert_eq!(user, expected_user);
    }

    #[tokio::test]
    async fn test_get_chat() {
        let mut mock_db = MockDb::new();
        let chat_id = new_uuid();
        let now = Utc::now();
        let members = vec![new_uuid(), new_uuid()];
        let expected_chat = Chat {
            chat_id,
            name: "test_chat".to_string(),
            members: members.clone(),
            created_at: now,
        };

        // Setup Expectations
        let chat_clone = expected_chat.clone();
        mock_db
            .expect_get_chat()
            .withf(move |id| *id == chat_id)
            .times(1)
            .returning(move |_| Ok(chat_clone.clone()));

        // Setup App
        let state = create_test_state(mock_db, MockProducer::new());
        let app = Router::new()
            .route("/chats/{chat_id}", get(get_chat))
            .with_state(state);

        // Execute
        let req = build_get_request(&format!("/chats/{}", chat_id));
        let response = app.oneshot(req).await.unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::OK);
        let chat: Chat = deserialize_body(response).await;
        assert_eq!(chat, expected_chat);
    }

    #[tokio::test]
    async fn test_post_message() {
        let mut mock_producer = MockProducer::new();
        let chat_id = new_uuid();
        let sender_id = new_uuid();
        let message_content = "Hello, world!".to_string();
        let create_message = CreatMessage {
            content: message_content.clone(),
        };

        // Setup Expectations
        let content_check = create_message.content.clone();
        mock_producer
            .expect_send_message()
            .withf(move |msg| msg.sender_id == sender_id && msg.content == content_check)
            .times(1)
            .returning(|_| Ok(()));

        // Setup App
        // We use an empty MockDb because this route only uses the producer
        let state = create_test_state(MockDb::new(), mock_producer);
        let app = Router::new()
            .route("/chats/{chat_id}/messages", post(post_message))
            .with_state(state);

        // Execute
        let cookie = format!("user_id={}", sender_id);
        // post_message expects a Form, not JSON, so we use the form helper
        // Note: crate::CreatMessage uses "content" field
        let body_str = format!("content={}", message_content);
        let req = build_form_request(
            &format!("/chats/{}/messages", chat_id),
            body_str,
            Some(&cookie),
        );

        let response = app.oneshot(req).await.unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::OK);
    }

    // --- HELPER FUNCTIONS ---

    fn new_uuid() -> Uuid {
        // Use random bytes to ensure uniqueness in tests
        Uuid::new_v4()
    }

    /// Creates the AppState, wrapping the mocks in Arcs
    fn create_test_state(db: MockDb, producer: MockProducer) -> AppState {
        AppState {
            db: Arc::new(db),
            producer: Arc::new(producer),
            tera: Arc::new(Tera::default()),
            connections_map: ConnectionMap::new(RwLock::new(HashMap::new())),
        }
    }

    /// Helper to build a generic JSON POST request
    #[allow(dead_code)]
    fn build_json_request(uri: &str, body: impl Serialize, cookie: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method(http::Method::POST)
            .uri(uri)
            .header(http::header::CONTENT_TYPE, "application/json");

        if let Some(c) = cookie {
            builder = builder.header(http::header::COOKIE, c);
        }

        builder
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap()
    }

    /// Helper to build a Form UrlEncoded POST request
    fn build_form_request(uri: &str, body_str: String, cookie: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method(http::Method::POST)
            .uri(uri)
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            );

        if let Some(c) = cookie {
            builder = builder.header(http::header::COOKIE, c);
        }

        builder.body(Body::from(body_str)).unwrap()
    }

    /// Helper to build a generic GET request
    fn build_get_request(uri: &str) -> Request<Body> {
        Request::builder()
            .method(http::Method::GET)
            .uri(uri)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::empty())
            .unwrap()
    }

    /// Helper to deserialize the response body
    async fn deserialize_body<T: DeserializeOwned>(response: Response) -> T {
        let body = response.into_body();
        let bytes = to_bytes(body, usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
