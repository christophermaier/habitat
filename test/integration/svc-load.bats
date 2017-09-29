#!/usr/bin/env bats

load 'helpers'

setup() {
    reset_hab_root
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
}
