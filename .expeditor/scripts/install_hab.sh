#!/bin/bash

set -euo pipefail

# install hab from source channel
source_channel=${1}

hab origin key download $HAB_ORIGIN
hab origin key download --auth $SCOTTHAIN_HAB_AUTH_TOKEN --secret $HAB_ORIGIN

echo "--- Installing hab binary from $source_channel"
sudo hab pkg install --channel $source_channel core/hab

echo "--- Also installing busybox and stuff"
sudo hab pkg install core/busybox-static
export BUSYBOX=$(hab pkg path core/busybox-static)/bin/busybox
