#!/usr/bin/env bats

load 'helpers'

setup() {
    reset_hab_root
}

teardown() {
    stop_supervisor
}

@test "start a service: origin/name" {
    background ${hab} svc start core/redis
    wait_for_service_to_run redis

    latest_redis=$(latest_from_builder core/redis stable)
    assert_package_and_deps_installed "${latest_redis}"
    assert_service_running "${latest_redis}"
}

@test "start a service: origin/name/version" {
    background ${hab} svc start core/redis/3.2.4
    wait_for_service_to_run redis

    latest_redis=$(latest_from_builder core/redis/3.2.4 stable)
    assert_package_and_deps_installed "${latest_redis}"
    assert_service_running "${latest_redis}"
}

@test "start a service: origin/name/version/release" {
    desired_version="core/redis/3.2.3/20160920131015"
    background ${hab} svc start "${desired_version}"
    wait_for_service_to_run redis

    assert_package_and_deps_installed "${desired_version}"
    assert_service_running "${desired_version}"
}

@test "CAN start a service from hart file" {
    desired_version="core/redis/3.2.4/20170514150022"

    # First, grab a hart file! Then, because we're using hab to
    # download the file, reset the hab root, just to simulate the case
    # of starting with nothing but a hart file.
    hart_path=$(download_hart_for "${desired_version}")
    reset_hab_root

    background ${hab} svc start "${hart_path}"
    wait_for_service_to_run redis

    assert_package_and_deps_installed "${desired_version}"
    assert_service_running "${desired_version}"
}

@test "only start if it's a service" {
    skip "Haven't implemented this in the supervisor yet"
}
