# ===============================================
# Stage 1: Create user "mathias"
# ===============================================
POST http://localhost:3000/users
{
    "username": "mathias"
}

# We expect a 201 Created status
HTTP 201

# Capture the user_id from the response body into a variable named "mathias_id"
[Captures]
mathias_id: jsonpath "$.user_id"

# Assert the structure and content of the response body
[Asserts]
jsonpath "$.username" == "mathias"
jsonpath "$.user_id" isString
jsonpath "$.created_at" isString
jsonpath "$.updated_at" isString


# ===============================================
# Stage 2: Create user "quentin"
# ===============================================
POST http://localhost:3000/users
{
    "username": "quentin"
}

HTTP 201
[Captures]
quentin_id: jsonpath "$.user_id"

[Asserts]
jsonpath "$.username" == "quentin"
jsonpath "$.user_id" isString
jsonpath "$.created_at" isString
jsonpath "$.updated_at" isString


# ===============================================
# Stage 3: Create a chat with both users
# ===============================================
POST http://localhost:3000/chats
# We use the captured variables in the request body with {{variable_name}}
{
    "name": "cool chat",
    "members": ["{{mathias_id}}", "{{quentin_id}}"]
}

HTTP 201
[Captures]
chat_id: jsonpath "$.chat_id"

[Asserts]
jsonpath "$.name" == "cool chat"
# Assert that the members in the response match the ones we sent
jsonpath "$.members[0]" == "{{mathias_id}}"
jsonpath "$.members[1]" == "{{quentin_id}}"
# Assert that these keys exist in the response
jsonpath "$.chat_id" exists
jsonpath "$.created_at" exists

# ===============================================
# Stage 4: Post a message to the chat
# ===============================================
POST http://localhost:3000/chats/{{chat_id}}/messages
{
    "sender_id": "{{mathias_id}}",
    "content": "Hello, world!"
}

HTTP 200
