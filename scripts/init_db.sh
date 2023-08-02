#!/usr/bin/env bash
set -v
set -io pipefail

# Function to check if running in Windows WSL and modify command if needed
# Reference: https://stackoverflow.com/a/61036356
function wsl_execute {
  if [[ -n "$IS_WSL" || -n "$WSL_DISTRO_NAME" ]]; then
    echo "$1.exe" # Append .exe for Windows WSL
  else
    echo "$1" # Use the original command for Linux
  fi
}

psql_exe=$(wsl_execute "psql")
sqlx_exe=$(wsl_execute "sqlx")

# Check if psql is installed
if ! [ -x "$(command -v "$psql_exe")" ]; then
  echo >&2 "Error: psql is not installed."
  exit 1
fi

# Check if sqlx is installed
if ! [ -x "$(command -v "$sqlx_exe")" ]; then
  echo >&2 "Error: sqlx is not installed."
  echo >&2 "Use:"
  echo >&2 " cargo install --version=0.5.7 sqlx-cli --no-default-features --features postgres"
  echo >&2 "to install it."
  exit 1
fi

# Check if a custom user has been set, otherwise default to 'postgres'
DB_USER=${POSTGRES_USER:=postgres}
# Check if a custom password has been set, otherwise default to 'password'
DB_PASSWORD="${POSTGRES_PASSWORD:=password}"
# Check if a custom database name has been set, otherwise default to 'newsletter'
DB_NAME="${POSTGRES_DB:=newsletter}"
# Check if a custom port has been set, otherwise default to '5432'
DB_PORT="${POSTGRES_PORT:=5432}"

RUNNING_CONTAINER=$(docker ps --filter name=postgres --format="{{.ID}}")
if [[ -n "$RUNNING_CONTAINER" ]]; then
  echo >&2 "Postgres is already running. Kill it with"
  echo >&2 "  docker kill ${RUNNING_CONTAINER}"
  exit 1
fi

# Allow to skip Docker if a dockerized Postgres database is already running
if [[ -z "${SKIP_DOCKER}" ]]; then
  # Launch postgres using Docker
  docker run \
    -e POSTGRES_USER=${DB_USER} \
    -e POSTGRES_PASSWORD=${DB_PASSWORD} \
    -e POSTGRES_DB=${DB_NAME} \
    --name postgres \
    -p "${DB_PORT}":5432 \
    -d postgres \
    postgres -N 1000
# ^ Increased maximum number of connections for testing purposes
fi

# Keep pinging Postgres until it's ready to accept commands
export PGPASSWORD="${DB_PASSWORD}"
until $psql_exe -h "localhost" -U "${DB_USER}" -p "${DB_PORT}" -d "postgres" -c '\q'; do
  echo >&2 "Postgres is still unavailable - sleeping"
  sleep 1
done
echo >&2 "Postgres is up and running on port ${DB_PORT}!"

# export variables only exist in bash execution
export DATABASE_URL=postgres://${DB_USER}:${DB_PASSWORD}@localhost:${DB_PORT}/${DB_NAME}
echo >&2 "Database url: $DATABASE_URL"
$sqlx_exe database create --database-url $DATABASE_URL

# Need to create migrations table before running migrations
# sqlx migrate add <table_name>
$sqlx_exe migrate run --database-url $DATABASE_URL
echo >&2 "Postgres has been migrated, ready to go!"