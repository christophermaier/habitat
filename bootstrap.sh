#!/bin/bash

# Given a bootstrap tarball, extract and install everything needed to
# run a given Habitat builder service on this machine.

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
tar=$(find_if_exists tar)

# Key Variables
########################################################################

# The path to a downloaded-and-verified bootstrap tarball
archive=${1}

# The name of the builder service (just one right now) that should be
# installed on this machine
# TODO: Take multiple args for the name of the service(s) you want to run on this machine
# TODO: if service starts with "core/", chop that off
service=${2}

# We're always going to need all the packages for running the
# supervisor; that's a given. We'll also need to install the package
# for the builder service that we want running on this server.
packages_to_install=(hab-launcher
                     hab
                     hab-sup
                     hab-butterfly

                     ${service})

# Unpack the tarball
########################################################################

tmpdir=/tmp/hab_bootstrap_$(date +%s)
mkdir -p ${tmpdir}

${tar} --extract \
       --verbose \
       --file=${archive} \
       --directory=${tmpdir}

# This is the hab binary from the bootstrap bundle. We'll use this to
# install everything.
hab_bootstrap_bin=${tmpdir}/bin/hab

# Install the bits
########################################################################

# Install the key(s) first. These need to be in place before
# installing any packages; otherwise, hab will try to contact a depot
# to retrieve them to verify the packages.
log "Installing public origin keys"
mkdir -p /hab/cache/keys
cp ${tmpdir}/keys/* /hab/cache/keys

for pkg in ${packages_to_install[@]}
do
    # Using a fake depot URL keeps us honest; this will fail loudly if
    # we need to go off the box to get *anything*
    HAB_DEPOT_URL=http://not-a-real-depot.habitat.sh \
                 ${hab_bootstrap_bin} pkg install ${tmpdir}/artifacts/core-${pkg}-*.hart
done

# Load the service
########################################################################

# Now that everything's installed, we need to load the service we want
# to actually run. To use the package we just installed and not try to
# hit the public depot, we must provide the fully-qualified package
# identifier.
fully_qualified_identifier=$(cat $(hab pkg path core/${service})/IDENT)
${hab_bootstrap_bin} svc load ${fully_qualified_identifier}

# TODO: might need to run hab svc load --force with the correct "real"
# configuration parameters. Maybe.

# Now we ensure that the hab binary being used on the system is the
# one that we just installed.
${hab_bootstrap_bin} pkg binlink core/hab hab

# Clean up after ourselves
rm -Rf ${tmpdir}
