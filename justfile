set dotenv-load := true

# start a redis service
redis:
  docker compose up -d --build

# remove redis service and any running containers
down:
  docker compose down

# Set environment variables and run the server
server:
  cargo run --bin server

# Start the generator service
generate:
  cargo run --bin generator
