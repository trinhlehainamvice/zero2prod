#!/usr/bin/env bash
set -v
set -io pipefail

RUNNING_REDIS_CONTAINER=$(docker ps --filter name=redis --format="{{.ID}}")
if [[ -n "$RUNNING_REDIS_CONTAINER" ]]; then
  echo >&2 "Redis is already running. Kill it with"
  echo >&2 "  docker kill ${RUNNING_REDIS_CONTAINER}"
  exit 1
fi

# Launch redis using Docker
docker run --rm\
  -p 6379:6379 \
  -d \
  --name "redis_$(date '+%s')" \
  redis:6
  
>&2 echo "Redis is running on port 6379"