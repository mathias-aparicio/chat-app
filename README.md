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
# POST user
curl --location 'http://127.0.0.1:3000/users' \
--header 'Content-Type: application/json' \
--data '{
    "username": "Mathias"
}'
# POST chat

curl --location 'http://127.0.0.1:3000/chat' \
--header 'Content-Type: application/json' \
--data '{
    "name": "Young Aspiring dev",
    "users": ["69b8419c-20e8-438b-9b86-95d2263c287c", "aa7c0241-b0aa-4ffd-9762-b125f75894b7"]
}'
```
