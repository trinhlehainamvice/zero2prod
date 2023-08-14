#!/usr/bin/env bash
set -v
set -io pipefail

RUNNING_MAILCRAB_CONTAINER=$(docker ps --filter name=mailcrab --format="{{.ID}}")
if [[ -n "$RUNNING_MAILCRAB_CONTAINER" ]]; then
  echo >&2 "Mailcrab is already running. Kill it with"
  echo >&2 "  docker kill ${RUNNING_MAILCRAB_CONTAINER}"
  exit 1
fi

# Launch mailcrab using Docker
docker run --rm\
  -p 1080:1080 \
  -p 1025:1025 \
  -d \
  --name "mailcrab" \
  marlonb/mailcrab:latest 
  
>&2 echo "Mailcrab is running on port 6379"