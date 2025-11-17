CREATE KEYSPACE IF NOT EXISTS ks WITH REPLICATION = {
    'class' : 'SimpleStrategy',
    'replication_factor' : 1
  };

USE ks;

CREATE TABLE IF NOT EXISTS users (
    user_id    UUID,
    username   TEXT,
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    PRIMARY KEY (user_id)
);

CREATE TABLE IF NOT EXISTS chats (
    chat_id    UUID,
    members      list<UUID>,
    name       TEXT,
    created_at TIMESTAMP,
    PRIMARY KEY (chat_id)
);

CREATE TABLE IF NOT EXISTS messages (
    chat_id    UUID,
    message_id TIMEUUID, 
    sender_id  UUID,
    content    TEXT,
    PRIMARY KEY (chat_id, message_id)
) WITH CLUSTERING ORDER BY (message_id DESC);
