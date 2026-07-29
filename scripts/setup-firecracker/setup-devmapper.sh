#!/usr/bin/env bash
# Configure a loop-backed devmapper thin-pool for firecracker-containerd.
# For production, use a dedicated block device instead (faster and more reliable).
#
# See: https://github.com/firecracker-microvm/firecracker-containerd/blob/main/docs/devmapper.md
set -euo pipefail

POOL_NAME="fc-dev-thinpool"
DATA_FILE="/var/lib/firecracker-containerd/snapshotter/devmapper/data"
META_FILE="/var/lib/firecracker-containerd/snapshotter/devmapper/metadata"
DATA_SIZE="100G"
META_SIZE="2G"

log() { echo "[devmapper] $*"; }

require_root() {
    if [[ "$EUID" -ne 0 ]]; then
        echo "error: run as root"
        exit 1
    fi
}

require_root

mkdir -p "$(dirname "$DATA_FILE")"

if dmsetup status "$POOL_NAME" &>/dev/null; then
    log "Thin-pool ${POOL_NAME} already exists, skipping creation"
    exit 0
fi

log "Creating ${DATA_SIZE} data file at ${DATA_FILE}..."
truncate -s "$DATA_SIZE" "$DATA_FILE"

log "Creating ${META_SIZE} metadata file at ${META_FILE}..."
truncate -s "$META_SIZE" "$META_FILE"

DATA_DEV=$(losetup --find --show "$DATA_FILE")
META_DEV=$(losetup --find --show "$META_FILE")
log "Loop devices: data=${DATA_DEV} meta=${META_DEV}"

DATA_SIZE_SECTORS=$(blockdev --getsz "$DATA_DEV")
META_SIZE_SECTORS=$(blockdev --getsz "$META_DEV")

dmsetup create "$POOL_NAME" \
    --table "0 ${DATA_SIZE_SECTORS} thin-pool ${META_DEV} ${DATA_DEV} 128 32768 1 skip_block_zeroing"

log "Thin-pool ${POOL_NAME} created successfully"
log "Add the following to your containerd config (already done if you ran install.sh):"
echo ""
echo "  [plugins]"
echo "    [plugins.\"io.containerd.snapshotter.v1.devmapper\"]"
echo "      pool_name = \"${POOL_NAME}\""
echo "      root_path = \"/var/lib/firecracker-containerd/snapshotter/devmapper\""
echo "      base_image_size = \"10GB\""
