# start a redis service
redis:
  docker compose up -d --build

# remove redis service and any running containers
down:
  docker compose down
