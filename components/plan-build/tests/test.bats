#!/usr/bin/env bats

# TODO (CM): set up some habitat packages for bats-assert, bats-file. For
# now, I'll just use Homebrew:
#
# brew tap kaos/shell
# brew install bats-assert
# brew install bats-file
#
# Might just add these into the core/bats package?

load '/usr/local/lib/bats-support/load.bash'
load '/usr/local/lib/bats-assert/load.bash'
load '/usr/local/lib/bats-file/load.bash'

load ${BATS_TEST_DIRNAME}/../bin/lib.sh

# setup() {
#   TEST_TEMP_DIR="$(temp_make)"
# }

# teardown() {
#   temp_del "$TEST_TEMP_DIR"
# }

@test "package metadata is written to the correct metadata file" {
    pkg_prefix=${BATS_TMPNAME}_pkg_prefix
    mkdir -p ${pkg_prefix}
    # TODO (CM): use setup/teardown for this instead

#    pkg_prefix=${TEST_TEMP_DIR}

    pkgs=(core/foo
          core/bar)
    run _write_composite_metadata "${pkgs[@]}"
    assert_success

    metadata=$(cat ${pkg_prefix}/${COMPOSITE_PACKAGES_METADATA_FILE})
    expected=$(echo -e "core/foo\ncore/bar")
    assert_equal "${metadata}" "${expected}"
}

@test "sort an array" {
    input=("foo" "bar" "baz")

    actual=$(_sort_array "${input[@]}")
    assert_equal "${actual[@]}" "bar baz foo"
}
