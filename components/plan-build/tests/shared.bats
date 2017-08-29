#!/usr/bin/env bats

load ${BATS_TEST_DIRNAME}/../bin/public.sh
load ${BATS_TEST_DIRNAME}/../bin/shared.sh

setup() {
    pkg_prefix=${BATS_TMPNAME}_pkg_prefix
    mkdir -p "${pkg_prefix}"
}

teardown() {
    rm -rf "${pkg_prefix}"
}

@test "Can render an associative array to a file" {
    declare -A test_array=(
        ["a"]="ay"
        ["b"]="bee"
        ["c"]="see"
    )

    run _render_associative_array_file $pkg_prefix "TEST_FILE" test_array
    [ "$status" -eq 0 ]

    # We sort for deterministic testing
    run sort $pkg_prefix/TEST_FILE
    [ "${#lines[@]}" -eq 3 ]

    [[ "${lines[0]}" = "a=ay" ]]
    [[ "${lines[1]}" = "b=bee" ]]
    [[ "${lines[2]}" = "c=see" ]]
}

@test "An empty associative array renders no file" {
    declare -A test_array
    test_array=()

    run _render_associative_array_file $pkg_prefix "EMPTY_METADATA" test_array
    [ "$status" -eq 0 ]

    run ls $pkg_prefix/EMPTY_METADATA
    [ $status -eq 1 ]
    [[ "$output" =~ "No such file or directory" ]]
}

@test "dependency metadata files render correctly" {
    local HAB_PKG_PATH="/hab/pkgs"
    dependencies=("/hab/pkgs/core/one/1.0.0/20170828000000"
                  "/hab/pkgs/core/two/2.0.0/20170828000000"
                  "/hab/pkgs/core/three/3.0.0/20170828000000")

    run _render_dependency_metadata_file $pkg_prefix TEST_DEPS dependencies
    [ $status -eq 0 ]

    run cat $pkg_prefix/TEST_DEPS
    [ "${#lines[@]}" -eq 3 ]

    # Dependency files are sorted
    [[ "${lines[0]}" = "core/one/1.0.0/20170828000000" ]]
    [[ "${lines[1]}" = "core/three/3.0.0/20170828000000" ]]
    [[ "${lines[2]}" = "core/two/2.0.0/20170828000000" ]]
}
