VERSION="latest"

cd ..

podman build --platform linux/arm64 ./ -t docker.io/sheosi/hallway:"$VERSION"-arm64
podman push docker.io/sheosi/hallway:"$VERSION"-arm64

podman build --platform linux/amd64 ./ -t docker.io/sheosi/hallway:"$VERSION"-amd64
podman push docker.io/sheosi/hallway:"$VERSION"-amd64


podman manifest create \
  "docker.io/sheosi/hallway:$VERSION" \
  "docker.io/sheosi/hallway:$VERSION-arm64" \
  "docker.io/sheosi/hallway:$VERSION-amd64"

podman login docker.io
podman manifest push "docker.io/sheosi/hallway:$VERSION" "docker.io/sheosi/hallway"

podman manifest rm "docker.io/sheosi/hallway:$VERSION"
