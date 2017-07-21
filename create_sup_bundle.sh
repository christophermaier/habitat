#!/bin/bash

# Create a tarball of all the Habitat artifacts needed to run the
# Habitat Supervisor on a system. This includes *all*
# dependencies. The goal is to have everything needed to run the
# supervisor *without* needing to talk to a running Depot.
#
# Because you have to bootstrap yourself from *somewhere* :)
#
# You must run this as root, because `hab` is going to be installing
# packages.
#
# This generates a tar file (not tar.gz!) that has the following
# internal structure:
#
# |-- ARCHIVE_ROOT
# |   |-- artifacts
# |   |   `-- all the hart files
# |   |-- bin
# |   |   `-- hab
# |   `-- keys
# |       `-- all the origin keys

set -euo pipefail

# TODO: Alternatively, just dispense with versions altogether and just
# get the latest stable!

HAB_VERSION=${1}
# TODO: Validate version?
echo "Version $HAB_VERSION"

# hab-launcher is versioned differently than the other packages
# (monotonically increasing integer only). It also isn't going to
# change much at all. This is the current version.
#
# TODO: Accept this as an optional argument?
LAUNCHER_VERSION=4435

# This is where we ultimately put all the things
S3_BUCKET="habitat-builder-bootstrap"

THIS_BOOTSTRAP_BUNDLE=hab_builder_bootstrap_$(date +%Y%m%d%H%M%S)

########################################################################
# Download all files locally

# Because Habitat may have already run on this system, we'll want to
# make sure we start in a pristine environment. That way, we can just
# blindly copy everything in ${SANDBOX_DIR}/hab/cache/artifacts, confident
# that those artifacts are everything we need, and no more.
SANDBOX_DIR=${THIS_BOOTSTRAP_BUNDLE}
mkdir ${SANDBOX_DIR}
echo "Using ${SANDBOX_DIR} as the Habitat root directory"

SUP_PACKAGES=(core/hab-launcher/${LAUNCHER_VERSION}
              core/hab/${HAB_VERSION}
              core/hab-sup/${HAB_VERSION}
              core/hab-butterfly/${HAB_VERSION})
for package in ${SUP_PACKAGES[@]}
do
    FS_ROOT=${SANDBOX_DIR} hab pkg install --channel=stable ${package}
done

# TODO: These packages (in prod, anyway) seem to be rather old... we
# may need to tweak them
BUILDER_PACKAGES=(core/hab-builder-api
                  core/hab-builder-admin
                  core/hab-builder-jobsrv
                  core/hab-builder-router
                  core/hab-builder-sessionsrv
                  core/hab-builder-vault
                  core/hab-builder-worker)
for package in ${BUILDER_PACKAGES[@]}
do
    FS_ROOT=${SANDBOX_DIR} hab pkg install --channel=stable ${package}
done

########################################################################
# Package everything up

ARTIFACT_DIR=${SANDBOX_DIR}/hab/cache/artifacts
echo "Creating TAR for all artifacts"

# TODO: pipe this through sort just to be damn sure that there's only one.

sup_artifact=$(echo ${ARTIFACT_DIR}/core-hab-sup-*)
archive_name=${THIS_BOOTSTRAP_BUNDLE}.tar
echo "Generating archive: ${archive_name}"

tar --create \
    --verbose \
    --file=${archive_name} \
    --directory=${SANDBOX_DIR}/hab/cache \
    artifacts

# We'll need a hab binary to bootstrap ourselves; let's take the one
# we just downloaded, shall we?
hab_pkg_dir=$(echo ${SANDBOX_DIR}/hab/pkgs/core/hab/${HAB_VERSION}/*)
tar --append \
    --verbose \
    --file=${archive_name} \
    --directory=${hab_pkg_dir} \
    bin

# We're also going to need the public origin key!
tar --append \
    --verbose \
    --file=${archive_name} \
    --directory=${SANDBOX_DIR}/hab/cache \
    keys

########################################################################
# Upload to S3
# TODO: This could be a separate function / script

SHA256=$(shasum --algorithm 256 ${archive_name} | awk '{print $1}')

aws s3 cp --acl=public-read ${archive_name} s3://${S3_BUCKET}

MANIFEST_FILE=${THIS_BOOTSTRAP_BUNDLE}_manifest.txt
echo ${archive_name} > ${MANIFEST_FILE}
echo ${SHA256} >> ${MANIFEST_FILE}
echo >> ${MANIFEST_FILE}
tar --list --file ${archive_name} | sort >> ${MANIFEST_FILE}

aws s3 cp --acl=public-read ${MANIFEST_FILE} s3://${S3_BUCKET}
aws s3 cp --acl=public-read s3://${S3_BUCKET}/${MANIFEST_FILE} s3://${S3_BUCKET}/LATEST

########################################################################
# Cleanup

# TODO: Actually clean up
