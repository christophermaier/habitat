#!/usr/bin/env bats

load "${BATS_TEST_DIRNAME}/../bin/public.sh"
load "${BATS_TEST_DIRNAME}/../bin/shared.sh"
load "${BATS_TEST_DIRNAME}/../bin/composite_build_functions.sh"

setup() {
    pkg_prefix=${BATS_TMPNAME}_pkg_prefix
    mkdir -p "${pkg_prefix}"
}

teardown() {
    rm -rf "${pkg_prefix}"
}

@test "Cannot supply zero composite services" {
    local pkg_services=()
    run _assert_more_than_one_service
    [ "$status" -eq 1 ]
    [[ "$output" =~ 'A composite package should have at least two services specified in $pkg_services; otherwise just build a non-composite Habitat package' ]]
}

@test "Cannot supply a single composite services" {
    local pkg_services=(core/foo)
    run _assert_more_than_one_service
    [ "$status" -eq 1 ]
    [[ "$output" =~ 'A composite package should have at least two services specified in $pkg_services; otherwise just build a non-composite Habitat package' ]]

}

@test "Must supply two or more composite services" {
    local pkg_services=(core/foo
                        core/bar)
    run _assert_more_than_one_service
    [ "$status" -eq 0 ]
    [ "$output" = "" ]
}

@test "SERVICES metadata file is sorted" {
    local pkg_services=(core/zzz
                        core/yyy
                        core/xxx)

    run _render_metadata_SERVICES
    [ "$status" -eq 0 ]

    run cat $pkg_prefix/SERVICES
    [ "${#lines[@]}" -eq 3 ]
    [[ "${lines[0]}" = "core/xxx" ]]
    [[ "${lines[1]}" = "core/yyy" ]]
    [[ "${lines[2]}" = "core/zzz" ]]
}

@test "RESOLVED_SERVICES metadata file is sorted" {
    local HAB_PKG_PATH="/hab/pkgs"
    local resolved_services=(/hab/pkgs/core/zzz/1.0.0/20170828000000
                             /hab/pkgs/core/yyy/1.0.0/20170828000000
                             /hab/pkgs/core/xxx/1.0.0/20170828000000)

    run _render_metadata_RESOLVED_SERVICES
    echo "${lines[@]}"
    [ "$status" -eq 0 ]

    run cat $pkg_prefix/RESOLVED_SERVICES
    echo "${lines[@]}"
    [ "${#lines[@]}" -eq 3 ]
    [[ "${lines[0]}" = "core/xxx/1.0.0/20170828000000" ]]
    [[ "${lines[1]}" = "core/yyy/1.0.0/20170828000000" ]]
    [[ "${lines[2]}" = "core/zzz/1.0.0/20170828000000" ]]
}
