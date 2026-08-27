build app:
    docker build . -f apps/{{app}}/Dockerfile -t {{app}}:latest

# Build the game host and load it into the microsandbox image cache.
#
# microsandbox keeps its own image cache and cannot see the Docker daemon's
# images, so a locally-built tag has to be handed over explicitly. `msb load`
# reads a Docker archive, which is why the image is built through Docker first.
#
# The cache lives in the `microsandbox_data` volume, so this needs the website
# container running (it installs `msb` there on first boot). `GAME_HOST_IMAGE` in
# docker-compose.yml must match the tag.
load-game-host tag="achtung-game-host:local":
    docker build . -f apps/achtung-host/Dockerfile -t {{tag}}
    docker save {{tag}} | docker compose exec -T website /root/.microsandbox/bin/msb load

# Same, for a website process running directly on the host rather than in compose
# (a different cache: ~/.microsandbox instead of the volume).
load-game-host-native tag="achtung-game-host:local":
    docker build . -f apps/achtung-host/Dockerfile -t {{tag}}
    docker save {{tag}} | ~/.microsandbox/bin/msb load

# Images the website container can boot sandboxes from.
msb-images:
    docker compose exec -T website /root/.microsandbox/bin/msb images

# Sandboxes this project owns, including any left by a crashed run.
msb-sandboxes:
    docker compose exec -T website /root/.microsandbox/bin/msb ps --label achtung.managed=1

# Force-remove every sandbox this project owns.
#
# The provider sweeps stale sandboxes itself before each match; this is for
# clearing them out by hand while debugging.
msb-clean:
    docker compose exec -T website /root/.microsandbox/bin/msb rm --force --label achtung.managed=1
