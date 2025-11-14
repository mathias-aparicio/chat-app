#!/usr/bin/env bash
set -x
set -eo pipefail

echo "Starting ScyllaDB..."
docker compose up -d scylladb

echo "Waiting for ScyllaDB to start..."
while ! docker exec scylladb cqlsh -e "DESCRIBE KEYSPACES;" >/dev/null 2>&1; do
  sleep 5
done
echo "ScyllaDB is up and running."

echo "Running database migrations with cquill..."
docker compose run --rm cquill-migrator

echo "Migration complete."
