#!/bin/bash

# Downloads and verifies the latest builder bootstrap tarball,
# returning the path to the tarball on standard output.

set -euo pipefail

self=${0}
log() {
  >&2 echo "${self}: $1"
}

find_if_exists() {
    command -v ${1} || { log "Required utility '${1}' cannot be found!  Aborting."; exit 1; }
}

# These are the key utilities this script uses. If any are not present
# on the machine, the script will exit.
awk=$(find_if_exists awk)
curl=$(find_if_exists curl)
shasum=$(find_if_exists shasum)
readlink=$(find_if_exists readlink)

# This is where we ultimately put all the things. All contents of the
# bucket will be publicly readable, so we can just use curl to grab them.
s3_root_url="https://s3-us-west-2.amazonaws.com/habitat-builder-bootstrap"

# Pull down the most recent tarball manifest file from S3. The name of
# the corresponding tarball is the first line of the file.
manifest_url=${s3_root_url}/LATEST
log "Downloading latest builder tarball manifest from ${manifest_url}"
${curl} --remote-name ${manifest_url} >&2
latest_package=$(${awk} 'NR==1' LATEST)

# Now that we know the latest tarball, let's download it, too.
latest_package_url=${s3_root_url}/${latest_package}
log "Downloading ${latest_package} from ${latest_package_url}"
${curl} --remote-name ${s3_root_url}/${latest_package} >&2

# Verify the tarball; the SHA256 checksum is the 2nd line of the
# manifest file.
checksum=$(${awk} 'NR==2' LATEST)
log "Verifying ${latest_package} with checksum ${checksum}"
${shasum} --algorithm 256 --check <<< "${checksum}  ${latest_package}" >&2

# Return the path to the downloaded and verified package for pipelines
echo $(${readlink} -f ${latest_package})
