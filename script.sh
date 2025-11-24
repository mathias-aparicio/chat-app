#!/usr/bin/env bash
set -euxo pipefail

docker compose up -d

echo "Waiting for ScyllaDB to start..."
while ! docker exec scylladb cqlsh -e "DESCRIBE KEYSPACES;" >/dev/null 2>&1; do
  sleep 2
done
echo "ScyllaDB is up and running."

echo "Running database migrations with cquill..."
docker compose run --rm cquill-migrator
echo "Migration complete."

echo "Creating chat-messages Topic..."
docker exec redpanda-0 rpk topic create chat-messages -p 1 -r 1
echo "Setup complet"
