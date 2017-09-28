#!/usr/bin/env bats

# Extracts a value from the given service's spec file and asserts that
# its value is as expected.
assert_spec_value() {
    local service=${1}
    local key=${2}
    local expected=${3}

    local spec=$(spec_file_for ${service})
    run grep ${key} ${spec}
    assert_success

    assert_equal "${output}" "${key} = \"${expected}\""
}

# When installing a composite, assert that every service described in
# its RESOLVED_SERVICES file has been fully installed.
assert_composite_and_services_are_installed() {
    local composite_ident=${1} # fully-qualified

    assert_package_installed "${composite_ident}"

    local resolved_services_file="/hab/pkgs/${composite_ident}/RESOLVED_SERVICES"
    assert_file_exist "${resolved_services_file}"

    for service in $(cat "${resolved_services_file}"); do
        assert_package_and_deps_installed "${service}"
    done
}

# Return the identifiers the composite manages. Note that these
# identifiers are the ones that will appear in the spec files, and DO
# NOT need to be fully-qualified!
services_for_composite() {
    local composite_ident=${1} # fully-qualified
    local services_file="/hab/pkgs/${composite_ident}/SERVICES"
    assert_file_exist "${services_file}"

    cat "${services_file}"
}
