#!/bin/bash

set -xeuo pipefail

# This is where we ultimately put all the things
S3_ROOT_URL="https://s3-us-west-2.amazonaws.com/habitat-builder-bootstrap"

curl ${S3_ROOT_URL}/LATEST > /tmp/LATEST
latest_package=$(head /tmp/LATEST -n1)
echo "Downloading ${latest_package} from S3"

# TODO: save this to /tmp
curl --remote-name ${S3_ROOT_URL}/${latest_package}

# Verify
CHECKSUM=$(head /tmp/LATEST -n2 | tail -n1)
shasum --check <<< "${CHECKSUM}  ${latest_package}"

echo "Verified!"
