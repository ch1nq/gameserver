build app:
    docker build . -f apps/{{app}}/Dockerfile -t {{app}}:latest

# Regenerate the committed gRPC-Web spectator client bundle from the protos.
# Reproducible via pinned npm deps (npm ci) + a pinned protoc; CI runs this and
# fails if apps/website/static/spectator.js drifts from the protos.
gen-spectator-client:
    #!/usr/bin/env bash
    set -euo pipefail
    cd apps/website/spectator-client
    npm ci
    rm -rf gen && mkdir gen
    protoc -I ../../../protos \
        --plugin=protoc-gen-js=node_modules/.bin/protoc-gen-js \
        --plugin=protoc-gen-grpc-web=node_modules/.bin/protoc-gen-grpc-web \
        --js_out=import_style=commonjs,binary:gen \
        --grpc-web_out=import_style=commonjs,mode=grpcwebtext:gen \
        spectator.proto achtung_spectator.proto
    npx esbuild entry.js --bundle --format=iife --outfile=../static/spectator.js
