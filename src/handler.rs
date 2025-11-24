use std::sync::Arc;

use anyhow::Context;
use axum::{
    Router,
    extract::{Json, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_anyhow::ApiResult;
use serde::Serialize;
use tower_http::trace::TraceLayer;

use crate::schema::{Chat, CreatMessage, CreateChat, CreateUser, Message, User};
use crate::{
    db::{Db, ScyllaDb},
    producer::{MessageProducer, Producer},
};

pub async fn create_router() -> anyhow::Result<Router> {
    let db = ScyllaDb::new().await?;
    let db = Arc::new(db);
    let producer = MessageProducer::new("localhost:19092", "chat-messages")
        .context("Failed to create message producer")?;
    let producer = Arc::new(producer);
    let app_state = AppState { db, producer };
    let app = Router::new()
        .route("/users", post(create_user))
        .route("/chats", post(create_chat))
        .route("/users/{user_id}", get(get_user))
        .route("/chats/{chat_id}", get(get_chat))
        .route("/chats/{chat_id}/messages", post(post_message))
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);
    Ok(app)
}
/// sender_id send a message to the chatId chat room from slug
async fn post_message(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(create_message): Json<CreatMessage>,
) -> ApiResult<StatusCode> {
    // FIXME : Make sure the id of message is converted to a TimeuiD
    state.producer.send_message(create_message).await?;
    Ok(StatusCode::OK)
}
async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUser>,
) -> ApiResult<JsonWithStatus<User>> {
    let user: User = state.db.create_user(&payload.username).await?;
    Ok(JsonWithStatus {
        status: StatusCode::CREATED,
        data: user,
    })
}

async fn create_chat(
    State(state): State<AppState>,
    Json(payload): Json<CreateChat>,
) -> ApiResult<JsonWithStatus<Chat>> {
    let chat: Chat = state
        .db
        .create_chat(&payload.name, &payload.members)
        .await?;
    Ok(JsonWithStatus {
        status: StatusCode::CREATED,
        data: chat,
    })
}

async fn get_user(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> ApiResult<JsonWithStatus<User>> {
    let user = state.db.get_user(&user_id).await?;
    Ok(JsonWithStatus {
        status: StatusCode::OK,
        data: user,
    })
}
async fn get_chat(
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> ApiResult<JsonWithStatus<Chat>> {
    let chat = state.db.get_chat(&chat_id).await?;
    Ok(JsonWithStatus {
        status: StatusCode::OK,
        data: chat,
    })
}
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
#[derive(Clone)]
struct AppState {
    // Polymorphism  allow testing with the mockall library
    db: Arc<dyn Db>,
    producer: Arc<dyn Producer>,
}
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;
    use async_trait::async_trait;
    use axum::{
        body::{Body, to_bytes},
        http::{self, Request, StatusCode},
    };
    use chrono::Utc;
    use mockall::mock;
    use serde_json::json;
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;
    use crate::producer::MockProducer;
    // TODO : Create help functions to remove code duplication
    mock! {
        pub Db {}
        #[async_trait]
        impl Db for Db {
            async fn create_user(&self, username: &str) -> Result<User>;
            async fn create_chat(&self, name: &str, members: &[Uuid]) -> Result<Chat>;
            async fn get_user(&self, user_id: &str) -> Result<User>;
            async fn get_chat(&self, chat_id: &str) -> Result<Chat>;
        }
    }

    #[tokio::test]
    async fn test_create_user() {
        let mut mock_db = MockDb::new();
        let user_id = Uuid::new_v4();
        let now = Utc::now();
        let expected_user = User {
            user_id,
            username: "test_user".to_string(),
            created_at: now,
            updated_at: now,
        };
        let expected_user_clone = expected_user.clone();
        mock_db
            .expect_create_user()
            .withf(|username| username == "test_user")
            .times(1)
            .returning(move |_| Ok(expected_user_clone.clone()));

        let app_state = AppState {
            db: Arc::new(mock_db),
            producer: Arc::new(MockProducer::new()),
        };
        let app = Router::new()
            .route("/users", post(create_user))
            .with_state(app_state);

        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::POST)
                    .uri("/users")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "username": "test_user" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response.into_body();
        let bytes = to_bytes(body, usize::MAX).await.unwrap();
        let user: User = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(user, expected_user);
    }

    #[tokio::test]
    async fn test_create_chat() {
        let mut mock_db = MockDb::new();
        let chat_id = Uuid::new_v4();
        let now = Utc::now();
        let members = vec![Uuid::new_v4(), Uuid::new_v4()];
        let expected_chat = Chat {
            chat_id,
            name: "test_chat".to_string(),
            members: members.clone(),
            created_at: now,
        };
        let expected_chat_clone = expected_chat.clone();
        mock_db
            .expect_create_chat()
            .withf(move |name, m| name == "test_chat" && m == members)
            .times(1)
            .returning(move |_, _| Ok(expected_chat_clone.clone()));

        let app_state = AppState {
            db: Arc::new(mock_db),
            producer: Arc::new(MockProducer::new()),
        };
        let app = Router::new()
            .route("/chats", post(create_chat))
            .with_state(app_state);

        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::POST)
                    .uri("/chats")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": "test_chat", "members": expected_chat.members })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response.into_body();
        let bytes = to_bytes(body, usize::MAX).await.unwrap();
        let chat: Chat = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(chat, expected_chat);
    }
    #[tokio::test]
    async fn test_get_user() {
        // We don't create a real connection to the database we create a mock instance with
        // expected result
        let mut mock_db = MockDb::new();
        let user_id = Uuid::new_v4();
        let now = Utc::now();

        let expected_user = User {
            user_id,
            username: "test_user".to_string(),
            created_at: now,
            updated_at: now,
        };
        let clone_expected_user = expected_user.clone();
        mock_db
            // Expect the function get_ser to be call on the db trait
            .expect_get_user()
            // The get_user route will be called with the user_id specificlly from the created mock
            // use we won't insert it we will make an expected return in the next methodr
            .withf(move |id| id == user_id.to_string())
            // This function will be called exactly one time
            .times(1)
            // We return the exepected user as if we had inserted it in the database
            .returning(move |_| Ok(clone_expected_user.clone()));

        let app_state = AppState {
            db: Arc::new(mock_db),
            producer: Arc::new(MockProducer::new()),
        };
        let app = Router::new()
            .route("/users/{user_id}", get(get_user))
            .with_state(app_state);
        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::GET)
                    .uri(format!("/users/{}", user_id))
                    .header(http::header::CONTENT_TYPE, "application/json")
                    // We don't need any body (lol)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body();
        let bytes = to_bytes(body, usize::MAX).await.unwrap();
        let user: User = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(user, expected_user);
    }
    #[tokio::test]
    async fn test_get_chat() {
        let mut mock_db = MockDb::new();
        let chat_id = Uuid::new_v4();
        let now = Utc::now();
        let members = vec![Uuid::new_v4(), Uuid::new_v4()];
        let expected_chat = Chat {
            chat_id,
            name: "test_chat".to_string(),
            members: members.clone(),
            created_at: now,
        };
        let expected_chat_clone = expected_chat.clone();
        mock_db
            .expect_get_chat()
            .withf(move |id| id == chat_id.to_string())
            .times(1)
            .returning(move |_| Ok(expected_chat_clone.clone()));
        let app_state = AppState {
            db: Arc::new(mock_db),
            producer: Arc::new(MockProducer::new()),
        };
        let app = Router::new()
            .route("/chats/{chat_id}", get(get_chat))
            .with_state(app_state);
        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::GET)
                    .uri(format!("/chats/{}", chat_id))
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body();
        let bytes = to_bytes(body, usize::MAX).await.unwrap();
        let chat: Chat = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(chat, expected_chat);
    }

    #[tokio::test]
    async fn test_post_message() {
        let mut mock_producer = MockProducer::new();
        let chat_id = Uuid::new_v4();
        let sender_id = Uuid::new_v4();
        let message_content = "Hello, world!".to_string();
        let create_message = CreatMessage {
            sender_id,
            content: message_content.clone(),
        };
        let create_message_clone = create_message.clone();

        mock_producer
            .expect_send_message()
            .withf(move |msg| {
                msg.sender_id == create_message_clone.sender_id
                    && msg.content == create_message_clone.content
            })
            .times(1)
            .returning(|_| Ok(()));

        let mock_db = MockDb::new(); // We don't need db for this test but AppState needs it
        let app_state = AppState {
            db: Arc::new(mock_db),
            producer: Arc::new(mock_producer),
        };

        let app = Router::new()
            .route("/chats/{chat_id}/messages", post(post_message))
            .with_state(app_state);

        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::POST)
                    .uri(format!("/chats/{}/messages", chat_id))
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&create_message).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
