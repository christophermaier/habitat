# Ensure that a composite package is internally consistent.
_validate_composite() {
    _assert_more_than_one_service
    _resolve_service_dependencies "${pkg_services[@]}"
    _validate_services

    # Validate all the bind mappings
    _resolve_all_exports
    _validate_bind_mappings
    # TODO (CM): handle optional binds?

    # Validate the sets
    _validate_pkg_sets
}



# TODO (CM): Add a helper function to assert that required global
# variables are present and set.


# TODO (CM): normalize names (assert/validate/ensure?)




# Create global variable for mapping a service name as given in plan.sh to
# the path-on-disk of the fully-qualified service that is resolved at plan
# build time
_setup_resolved_services(){
  _setup_global_associative_array resolved_services
}

# Create the pkg_export_map associative array.
_setup_pkg_export_map(){
  _setup_global_associative_array pkg_export_map
}

# Helper function to create, um... global associative arrays.
_setup_global_associative_array(){
  local var_name=${1}
  debug "Creating '${var_name}' global associative array"
  declare -A -g ${var_name}
}

_setup_composite_build_global_variables(){
    _setup_resolved_services
    _setup_pkg_export_map
}

# If you didn't specify any packages, why are you making a composite?
# If you only specified one, why aren't you just using that directly?
_assert_more_than_one_service() {
    if [ "${#pkg_services[@]}" -lt "2" ]; then
        exit_with "A composite package should have at least two services specified in \$pkg_services; otherwise just build a non-composite Habitat package" 1
    fi
}

# Pass in an array of service names from plan.sh and install them locally.
# Record in the global `resolved_services` associative array the mapping from
# the service as-given to the path-on-disk of the fully-resolved package.
#
# TODO (CM): borrowed from resolve_run_dependencies & company;
# consider further refactoring and consolidation
#
# TODO (CM): Consider just reading from pkg_services globally
_resolve_service_dependencies() {
    build_line "Resolving service dependencies"

    local services=("${@}")
    local resolved
    local service

    for service in "${services[@]}"; do
      build_line "Installing ${service} locally"
      _install_dependency "${service}"
      if resolved="$(_resolve_dependency $service)"; then
        build_line "Resolved service '$service' to $resolved"
        resolved_services[$service]=$resolved
      else
        exit_with "Resolving '$service' failed, should this be built first?" 1
      fi
    done
}

# Ensure that all the services are actually services.
_validate_services() {
    local resolved

    for rs in "${!resolved_services[@]}"
    do
        resolved=${resolved_services[$rs]}
        _assert_package_is_a_service "${resolved}"
    done
}

# Given the path to an expanded package on disk, determine if it's a service
# (i.e., a package that has a run script)
_assert_package_is_a_service() {
    local pkg_path="${1}"
    build_line "Verifying that ${pkg_path} is a service"
    if [ ! -e "${pkg_path}/run" ] && [ ! -e "${pkg_path}/hooks/run" ]; then
        exit_with "'${pkg_path}' is not a service. Only services are allowed in composite packages"
    fi
}

# Given a path to a package's directory on disk and the name of a package
# metadata file, returns the contents of that file on standard output.
_read_metadata_file_for() {
  local pkg_path="${1}"
  local filename="${2}"
  local full_path="${pkg_path}/${filename}"
  if [[ -f "${full_path}" ]]; then
    cat "${full_path}"
  else
    echo
  fi
}

# Assemble a list of all the exports from a given package and return the list on
# standard output.
_exports_for_pkg() {
  local path_to_pkg_on_disk=${1}
  local exports=()
  local line

  while read -r line; do
    exports+=("${line%%=*}")
  done < <(_read_metadata_file_for "${path_to_pkg_on_disk}" EXPORTS)

  echo "${exports[@]}"
}

# Grab all the exports for the universe of packages
# e.g. core/builder-api-proxy => "foo bar baz"
_resolve_all_exports() {
  local resolved
  local exports

  for rs in "${!resolved_services[@]}"; do
    resolved=${resolved_services[$rs]}
    exports=("$(_exports_for_pkg ${resolved})")
    pkg_export_map[$resolved]="${exports[@]}"
  done
}

# Ensure that all the bind mappings supplied are valid. This means:
#
# * The binds are actually defined for the given package
# * The package that satisfies the bind actually exports what the bind requires
_validate_bind_mappings() {
  for pkg in "${!pkg_bind_map[@]}"; do
    warn "Resolving binds for ${pkg}"

    # TODO (CM): Here we are implicitly assuming that the values # in the
    # pkg_bind_map are exactly the same as given in # pkg_services. Is this
    # the right thing, or should we # normalize to `origin/package` instead,
    # regardless of what # was given in pkg_services?

    # Need to grab all the binds of `pkg` from its metadata on disk
    unset all_binds_for_pkg
    declare -A all_binds_for_pkg

    resolved="${resolved_services[$pkg]}"

    while read -r line; do
      IFS== read bind_name exports <<< "${line}"
      all_binds_for_pkg[$bind_name]="${exports[@]}"
    done < <(_read_metadata_file_for "${resolved}" BINDS)

    unset bind_mappings
    bind_mappings=("${pkg_bind_map[$pkg]}")
    warn "BIND MAPPINGS: ${bind_mappings[@]}"

    # This is space-delimited, so no quotes?
    for mapping in ${bind_mappings[@]}; do
      # Each mapping is of the form `bind_name:package`, like so:
      #     router:core/builder-router
      IFS=: read bind_name satisfying_package <<< "${mapping}"

      # Assert that the named bind exists
      debug "Verifying that ${resolved} has a bind named '${bind_name}'"
      if [ -z "${all_binds_for_pkg[$bind_name]}" ]; then
        exit_with "The bind '${bind_name}' specified in \$pkg_bind_map for the package '${pkg}' does not exist in ${resolved_services[$pkg]}."
        # TODO (CM): Why does adding this to the above exit
        # message cause it to crash?
        #
        #It currently requires the following binds: ${!all_binds_for_pkg[@]}"
      fi

      resolved_satisfying_package="${resolved_services[$satisfying_package]}"

      # TODO (CM): Need to handle when a package does not export anything
      satisfying_package_exports=("${pkg_export_map[$resolved_satisfying_package][@]}")

      debug "Checking that the bind '${bind_name}' for ${resolved} can be satisfied by ${resolved_satisfying_package}"

      # Assert that the mapped service satisfies all the exports
      # of this bind
      for required_exported_value in ${all_binds_for_pkg[$bind_name][@]}; do
        debug "REQUIRED EXPORTED VALUE FOR ${bind_name}: ${required_exported_value}"
        if ! _array_contains "$required_exported_value" ${satisfying_package_exports[@]}; then
          exit_with "${satisfying_package} does not export '${required_exported_value}', which is required by the '${bind_name}' bind of ${resolved}"
        fi
      done
    done
  done
}

# Each set must consist of services listed in `pkg_services`
_validate_pkg_sets() {
  local set
  local member

  for set in "${!pkg_sets[@]}"; do
    # The value of pkg_sets is a space-delimited string of entries, so don't quote
    for member in ${pkg_sets[$set]}; do
      if ! _array_contains "${member}" "${pkg_services[@]}"; then
        exit_with "Service set '$set' has '$member' as a member, but this was not listed in \$pkg_services"
      fi
    done
  done
}

# TODO (CM): Validate the default set is actually a set
# TODO (CM): Do we default to everything being a set? Write that into
# the metadata file?
#
# YES, we should default to everything. Builder is special in that
# it's one big system, as opposed to a service with its sidecars. In
# the latter case, of course you'll want to run all of them at once;
# why should you need to specify that?
#
# Maybe if you supply no sets, we default to everything. Adding sets
# can come later.
#
# TODO (CM): Should all services be accounted for in the union of all sets?
# TODO (CM): Should we allow one-member sets?

################################################################################
# Composite Package Metadata Rendering functions

_render_composite_metadata() {
    build_line "Building package metadata"

    _render_metadata_BIND_MAP
    _render_metadata_RESOLVED_SERVICES
    _render_metadata_SERVICE_SETS
    _render_metadata_SERVICES

    # TODO (CM): Consider renaming to reflect "common" metadata, or
    # just have the functions be robust enough so that we can just
    # render EVERYTHING and have it just work.

    # NOTE: These come from the shared.sh library
    _render_metadata_IDENT
    _render_metadata_TARGET
    _render_metadata_FILES
    _render_metadata_TYPE
}

# Render the services AS GIVEN IN THE PLAN; DO NOT perform any
# truncation based on HAB_PKG_PATH as with other dependency-type
# metadata files.
_render_metadata_SERVICES() {
  deps="$(printf '%s\n' "${pkg_services[@]}" | sort)"
  if [[ -n "$deps" ]]; then
    debug "Rendering SERVICES metadata file"
    echo "$deps" > $pkg_prefix/SERVICES
  fi
}

# Render the services AS RESOLVED... this is just for human understanding
_render_metadata_RESOLVED_SERVICES() {
    _render_dependency_metadata_file $pkg_prefix RESOLVED_SERVICES resolved_services
}

_render_metadata_BIND_MAP() {
  _render_associative_array_file ${pkg_prefix} BIND_MAP pkg_bind_map
}

_render_metadata_SERVICE_SETS() {
  _render_associative_array_file ${pkg_prefix} SERVICE_SETS pkg_sets
  # TODO (CM): Where to put the default set?
}
