#!/usr/bin/env bats

load 'helpers'

setup() {
#    run_only_test "changing an ident when force-loading with an update-strategy of none runs the latest applicable version"
    run_only_test "a bad strategy value is rejected"
    reset_hab_root
}

teardown() {
    stop_supervisor
}

@test "a bad topology value is rejected" {
    run ${hab} svc load --topology=beelzebub core/redis
    assert_failure
    [[ "${output}" =~ "Invalid value for '--topology <TOPOLOGY>'" ]]
}

@test "a bad strategy value is rejected" {
    run ${hab} svc load --strategy=beelzebub core/redis
    assert_failure
    [[ "${output}" =~ "Invalid value for '--strategy <STRATEGY>'" ]]
}


@test "load a service" {
    run ${hab} svc load core/redis
    assert_success

    latest_redis=$(latest_from_builder core/redis stable)
    assert_package_and_deps_installed "${latest_redis}"

    # TODO: Should we test that the service is running? If so, need to sleep
    assert_spec_exists_for redis

    assert_spec_value redis ident core/redis
    assert_spec_value redis group default
    assert_spec_value redis start_style persistent
    assert_spec_value redis channel stable
    assert_spec_value redis topology standalone
    assert_spec_value redis update_strategy none
    assert_spec_value redis desired_state up
    assert_spec_value redis bldr_url "https://bldr.habitat.sh"
}

@test "load a service with version" {
    run ${hab} svc load core/redis/3.2.4
    assert_success

    latest_redis=$(latest_from_builder core/redis stable)
    assert_package_and_deps_installed "${latest_redis}"
    assert_spec_exists_for redis

    assert_spec_value redis ident core/redis/3.2.4
    assert_spec_value redis group default
    assert_spec_value redis start_style persistent
    assert_spec_value redis channel stable
    assert_spec_value redis topology standalone
    assert_spec_value redis update_strategy none
    assert_spec_value redis desired_state up
    assert_spec_value redis bldr_url "https://bldr.habitat.sh"
}

@test "load a service from a fully-qualified identifier" {
    desired_version="core/redis/3.2.3/20160920131015"
    run ${hab} svc load "${desired_version}"
    assert_success

    assert_package_and_deps_installed "${desired_version}"
    assert_spec_exists_for redis

    assert_spec_value redis ident "${desired_version}"
    assert_spec_value redis group default
    assert_spec_value redis start_style persistent
    assert_spec_value redis channel stable
    assert_spec_value redis topology standalone
    assert_spec_value redis update_strategy none
    assert_spec_value redis desired_state up
    assert_spec_value redis bldr_url "https://bldr.habitat.sh"
}

@test "load a service loads from installed package" {
    desired_version="core/redis/3.2.3/20160920131015"
    # Pre-install this older package. Loading the service should not cause a
    # newer package to be installed.
    run ${hab} pkg install "${desired_version}"

    run ${hab} svc load "core/redis"
    assert_success

    assert_package_and_deps_installed "${desired_version}"
    assert_spec_exists_for redis
}

@test "load a service from hart file" {
    # First, grab a hart file!
    desired_version="core/redis/3.2.4/20170514150022"
    hart_path=$(download_hart_for "${desired_version}")
    reset_hab_root

    run ${hab} svc load "${hart_path}"
    assert_success
    assert_package_and_deps_installed "${desired_version}"
    assert_spec_exists_for redis

    assert_spec_value redis ident "${desired_version}"
    assert_spec_value redis group default
    assert_spec_value redis start_style persistent
    assert_spec_value redis channel stable
    assert_spec_value redis topology standalone
    assert_spec_value redis update_strategy none
    assert_spec_value redis desired_state up
    assert_spec_value redis bldr_url "https://bldr.habitat.sh"
}

@test "load a new service configuration with --force" {
    run ${hab} svc load core/redis
    assert_success

    # Assert the default values in the service spec
    assert_spec_value redis ident core/redis
    assert_spec_value redis group default
    assert_spec_value redis start_style persistent
    assert_spec_value redis channel stable
    assert_spec_value redis topology standalone
    assert_spec_value redis update_strategy none
    assert_spec_value redis desired_state up
    assert_spec_value redis bldr_url "https://bldr.habitat.sh"

    # Now, "reload" and change a few settings (chosen here arbitrarily)
    run ${hab} svc load --force --channel=unstable --strategy=at-once core/redis
    assert_success

    # Assert the spec values after the update
    assert_spec_value redis ident core/redis
    assert_spec_value redis group default
    assert_spec_value redis start_style persistent
    assert_spec_value redis channel unstable # <-- changed!
    assert_spec_value redis topology standalone
    assert_spec_value redis update_strategy at-once # <-- changed!
    assert_spec_value redis desired_state up
    assert_spec_value redis bldr_url "https://bldr.habitat.sh"
}

@test "loading an already loaded service without --force is an error" {
    run ${hab} svc load core/redis
    assert_success

    # Assert the contents of the spec file; we'll compare again later
    assert_spec_value redis ident core/redis
    assert_spec_value redis group default
    assert_spec_value redis start_style persistent
    assert_spec_value redis channel stable
    assert_spec_value redis topology standalone
    assert_spec_value redis update_strategy none
    assert_spec_value redis desired_state up
    assert_spec_value redis bldr_url "https://bldr.habitat.sh"

    # Now, try to load again, but without --force
    run ${hab} svc load --channel=unstable --strategy=at-once core/redis
    assert_failure

    # Check that the spec file values didn't change
    assert_spec_value redis ident core/redis
    assert_spec_value redis group default
    assert_spec_value redis start_style persistent
    assert_spec_value redis channel stable
    assert_spec_value redis topology standalone
    assert_spec_value redis update_strategy none
    assert_spec_value redis desired_state up
    assert_spec_value redis bldr_url "https://bldr.habitat.sh"
}

@test "application and environment are properly set in a spec" {
    run ${hab} svc load --application=myapp --environment=prod core/redis
    assert_success

    assert_spec_value redis ident core/redis
    assert_spec_value redis application_environment "myapp.prod"
    # TODO (CM): Need a way to assert a missing value in a spec so I
    # can add those above?
}

@test "spec idents can change when force-loading using a different ident" {
    vsn="core/redis/3.2.3/20160920131015"

    HAB_UPDATE_STRATEGY_FREQUENCY_MS=5000 background ${hab} run

    run ${hab} svc load "${vsn}"
    assert_success
    wait_for_service_to_run redis
    initial_pid=$(pid_of_service redis)

    assert_spec_value redis ident "${vsn}"

    run ${hab} svc load --channel=unstable --strategy=at-once --force core/redis
    assert_success

    # loading causes a restart anyway
    wait_for_service_to_restart redis "${initial_pid}"
    new_pid=$(pid_of_service redis)

    # The ident should have changed (among other things)
    assert_spec_value redis ident core/redis
    assert_spec_value redis channel unstable
    assert_spec_value redis update_strategy at-once

    # Wait for the new version to be installed
    wait_for_service_to_restart redis "${new_pid}"

    latest_redis=$(latest_from_builder core/redis unstable)
    assert_package_and_deps_installed "${latest_redis}"

    updated_running_version=$(current_running_version_for redis)
    assert_equal "${latest_redis}" "$updated_running_version"
    # assert latest redis is installed, though not necessarily
    # *running* (that's for the updater to do)
}

# TODO (CM): We need to download a new package if there was a change of spec, we
# don't have a satisfying package, and the new update strategy is NONE
@test "changing an ident when force-loading with an update-strategy of none runs the latest applicable version" {
    vsn="core/redis/3.2.3/20160920131015"

    HAB_UPDATE_STRATEGY_FREQUENCY_MS=5000 background ${hab} run

    run ${hab} svc load --channel=unstable "${vsn}"
    assert_success
    wait_for_service_to_run redis
    initial_pid=$(pid_of_service redis)

    assert_spec_value redis ident "${vsn}"
    assert_spec_value redis channel unstable

    run ${hab} svc load --strategy=none --force core/redis
    assert_success

    # # This should have downloaded a new version
    latest_redis=$(latest_from_builder core/redis unstable)
    # assert_package_and_deps_installed "${latest_redis}"

    wait_for_service_to_restart redis "${initial_pid}"

    # The ident should have changed (among other things)
    assert_spec_value redis ident core/redis
    assert_spec_value redis update_strategy none
    assert_spec_value redis channel unstable

    updated_running_version=$(current_running_version_for redis)
    assert_equal "$updated_running_version" "${latest_redis}"
}
