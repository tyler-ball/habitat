#!/bin/bash

 set -euo pipefail

 ########################################################################
# `component` should be the subdirectory name in `components` where a
# particular component code resides.
#
# e.g. `hab` for `core/hab`, `plan-build` for `core/plan-build`,
# etc.
component=${1}

# export HAB_BLDR_CHANNEL=$BUILDKITE_BUILD_ID
# export BUILD_CHANNEL=$BUILDKITE_BUILD_ID

hab pkg install core/hab

destination_channel=$BUILDKITE_BUILD_ID

hab_bin_path=$(hab pkg path core/hab)
hab_binary="$hab_bin_path/bin/hab"
hab_binary_version=$($hab_binary --version)

echo "--- Using habitat version $hab_binary_version"

# export HAB_BIN=$hab_binary

# probably grab dynamically or something
# export BUSYBOX=/hab/pkgs/core/busybox-static/1.29.2/20190115014552/bin/busybox
# export HAB_STUDIO_BACKLINE_PKG=core/hab-backline/0.80.6/20190422194228
# export HAB_BINARY_PKG=core/hab/0.80.6/20190422194221
# export HAB_PLAN_BUILD_PKG=core/hab-plan-build/0.80.6/20190422194228

 echo "--- Running a build $HAB_ORIGIN / $component / ${destination_channel:-}"
$hab_binary origin key download $HAB_ORIGIN
$hab_binary origin key download --auth $SCOTTHAIN_HAB_AUTH_TOKEN --secret $HAB_ORIGIN

 echo "--- Using $hab_binary_version"
$hab_binary pkg build "components/${component}"
# components/studio/bin/hab-studio.sh build "components/${component}"
. results/last_build.env

#  # Always upload to the destination channel.
# $hab_binary pkg upload --auth $SCOTTHAIN_HAB_AUTH_TOKEN --channel $destination_channel "results/$pkg_artifact"