#!/bin/sh
set -e

# Create certs directory if it doesn't exist
mkdir -p /certs

# Write the public key from environment variable to file
# This allows us to use Fly secrets instead of the [[files]] section
if [ -n "$REGISTRY_PUBLIC_KEY" ]; then
    echo "Writing public key from REGISTRY_PUBLIC_KEY to /certs/public.crt"
    printf '%s\n' "$REGISTRY_PUBLIC_KEY" > /certs/public.crt
    chmod 644 /certs/public.crt
    echo "Public key written successfully"
else
    echo "WARNING: REGISTRY_PUBLIC_KEY environment variable not set"
fi

export REGISTRY_AUTH_TOKEN_ROOTCERTBUNDLE="/certs/public.crt"

# Start the registry with the serve command
exec registry "$@"
