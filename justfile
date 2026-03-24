set dotenv-load := true

# start a redis and api service
service:
  docker compose up -d --build

# remove redis service and any running containers
down:
  docker compose down

# Set environment variables and run the server
server-local:
  cargo run --bin server

# Start the generator service
generate-local:
  cargo run --bin generator

# deploy the API using helm charts
helm:
  helm --install snuff ./snuff -f snuff/values-env.yaml

#upgrade a helm chart
upgrade-helm:
  helm upgrade --install snuff ./snuff -f snuff/values-env.yaml

#stop the helm services
down-helm:
   helm upgrade snuff ./snuff --set replicaCount=0

