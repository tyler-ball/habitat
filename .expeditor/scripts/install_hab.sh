#!/bin/bash

set -euo pipefail

# install hab from source channel
source_channel=${1}

hab origin key download $HAB_ORIGIN
hab origin key download --auth $SCOTTHAIN_HAB_AUTH_TOKEN --secret $HAB_ORIGIN

echo "--- Installing updated hab binary from $source_channel"
sudo hab pkg install --channel $source_channel scotthain/hab
