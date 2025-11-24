# Messaging App

## What does the app do

1. Alice send "hi" to Bob

- `POST` request to `/chats/{chatId}/messages`
- Publish payload (sender ID, chat ID, content) to a messages topic in RedPanda
- Do not save in the database

2. Background process watch the messages topic

- Save to messages table
- Send to bob's websocket if he is connected

3. Message reception

- If Bob is online
  - push the message to Bob Websocket
- If Bob is offline
  - simply stored in the database
  - upon Bob connection his client will establish a connetion via `/ws/connect/{userId}` and `GET` `/chats/{chatID}/message` to fetch the conversation.

## Testing the endpoints

```bash
./init-db.sh
cargo run
hurl --test tests/test_endpoint.hu
```
