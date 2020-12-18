eval $(minikube docker-env -p agnoes)
docker build -t game-server-app -f server/Dockerfile .
