#!/bin/bash

# Given a bootstrap tarball, extract and install everything needed to
# run a given Habitat builder service on this machine.

set -euo pipefail

archive=${1}
service=${2}

# TODO: Take multiple args for the name of the service(s) you want to run on this machine
# TODO: if service starts with "core/", chop that off

TMPDIR=/tmp/hab_bootstrap_$(date +%s)
mkdir -p ${TMPDIR}

tar --extract \
    --verbose \
    --file=${archive} \
    --directory=${TMPDIR}

HAB_BIN=${TMPDIR}/bin/hab

# Prep the key(s)
mkdir -p /hab/cache/keys
cp ${TMPDIR}/keys/* /hab/cache/keys

# We're always going to need all the packages for running the
# supervisor; that's a given. We'll also need to install the package
# for the builder service that we want running on this server.
PACKAGES_TO_INSTALL=(hab-launcher
                     hab
                     hab-sup
                     hab-butterfly
                     ${service})
for pkg in ${PACKAGES_TO_INSTALL[@]}
do
    # Using a fake depot URL keeps us honest; this will fail loudly if
    # we need to go off the box to get *anything*
    HAB_DEPOT_URL=http://not-a-real-depot.habitat.sh \
                 ${HAB_BIN} pkg install ${TMPDIR}/artifacts/core-${pkg}-*.hart
done

# Now that everything's installed, we need to load the service we want
# to actually run. To use the package we just installed and not try to
# hit the public depot, we must provide the fully-qualified package
# identifier.
fully_qualified_identifier=$(cat $(hab pkg path core/${service})/IDENT)
${HAB_BIN} svc load ${fully_qualified_identifier}

# TODO: might need to run hab svc load --force with the correct "real"
# configuration parameters. Maybe.

# Binlink the real hab
${HAB_BIN} pkg binlink core/hab hab

# Clean up after ourselves
rm -Rf ${TMPDIR}
