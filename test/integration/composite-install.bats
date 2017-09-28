#!/usr/bin/env bats

load 'helpers'

setup() {
    reset_hab_root
}

teardown() {
    stop_supervisor
}

# TODO: Need to come up with a smaller composite to test with. Some
# small nginx + app?

@test "install a composite from a hart file" {
    composite_ident="core/builder/1.0.0/20170926215145"
    run ${hab} pkg install fixtures/core-builder-1.0.0-20170926215145-x86_64-linux.hart
    assert_success
    assert_composite_and_services_are_installed "${composite_ident}"
}

@test "trying to binlink with a composite doesn't blow up" {
    composite_ident="core/builder/1.0.0/20170926215145"
    run ${hab} pkg install fixtures/core-builder-1.0.0-20170926215145-x86_64-linux.hart --binlink
    assert_success
    assert_composite_and_services_are_installed "${composite_ident}"
}

@test "load a composite" {
    # background ${hab} run
    composite_ident="core/builder/1.0.0/20170926215145"
    ${hab} svc load fixtures/core-builder-1.0.0-20170926215145-x86_64-linux.hart
    # wait_for_service_to_run builder-router
    # wait_for_service_to_run builder-originsrv
    # wait_for_service_to_run builder-jobsrv
    # wait_for_service_to_run builder-api
    # wait_for_service_to_run builder-api-proxy
    # # etc...

    assert_composite_and_services_are_installed "${composite_ident}"

    assert_composite_spec "${composite_ident}"
    for service in $(services_for_composite "${composite_ident}"); do
        service_name=$(name_from_ident "${service}")
        assert_spec_exists_for "${service_name}"

        assert_spec_value "${service_name}" ident "${service}"
        assert_spec_value "${service_name}" group default
        assert_spec_value "${service_name}" composite builder
        assert_spec_value "${service_name}" start_style persistent
        assert_spec_value "${service_name}" channel stable
        assert_spec_value "${service_name}" topology standalone
        assert_spec_value "${service_name}" update_strategy none
        assert_spec_value "${service_name}" desired_state up
        assert_spec_value "${service_name}" bldr_url "https://bldr.habitat.sh"

        # Would be nice to assert on binds, too. Could probably just
        # assume that if the services are running, they're right,
        # though.
    done
 }

# @test "start a composite" {
#     skip
#     background ${hab} run

#     ${hab} svc start --composite fixtures/core-builder-1.0.0-20170926215145-x86_64-linux.hart
#     wait_for_service_to_run builder-router
#     wait_for_service_to_run builder-originsrv
#     wait_for_service_to_run builder-jobsrv
#     wait_for_service_to_run builder-api
#     wait_for_service_to_run builder-api-proxy
#     # etc...

#     assert_package_and_deps_installed $(latest_from_builder core/builder-router)
#     assert_package_and_deps_installed $(latest_from_builder core/builder-originsrv)
#     assert_package_and_deps_installed $(latest_from_builder core/builder-jobsrv)
#     assert_package_and_deps_installed $(latest_from_builder core/builder-api)
#     assert_package_and_deps_installed $(latest_from_builder core/builder-api-proxy)
# }
