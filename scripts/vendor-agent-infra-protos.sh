#!/usr/bin/env bash
# Vendor proto files from firecracker-containerd into libs/agent-infra/proto/.
# Mirrors what install.sh does for the FC_CTD_REF variable so the protos stay
# in sync with the firecracker-containerd version running on the host.
#
# Usage:
#   ./scripts/vendor-agent-infra-protos.sh            # uses main
#   FC_CTD_REF=v0.7.0 ./scripts/vendor-agent-infra-protos.sh

set -euo pipefail

REPO="firecracker-microvm/firecracker-containerd"
REF="${FC_CTD_REF:-main}"
DEST="$(cd "$(dirname "$0")/.." && pwd)/libs/agent-infra/proto"

# Upstream paths → local filenames
declare -A SOURCES=(
    ["proto/types.proto"]="types.proto"
    ["proto/firecracker.proto"]="firecracker.proto"
    ["proto/service/fccontrol/fccontrol.proto"]="fccontrol.proto"
)

BASE_URL="https://raw.githubusercontent.com/${REPO}/${REF}"

echo "Vendoring agent-infra protos from ${REPO}@${REF}"

for upstream_path in "${!SOURCES[@]}"; do
    dest_file="${DEST}/${SOURCES[$upstream_path]}"
    url="${BASE_URL}/${upstream_path}"
    echo "  ${upstream_path} -> proto/${SOURCES[$upstream_path]}"
    curl -fsSL "$url" -o "$dest_file"
done

echo "Done. Files written to libs/agent-infra/proto/"
