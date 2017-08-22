# Just a though-experiment

########################################################################
# Everything in here is what a user would put in their plan for the
# composite

pkg_name="composite-test"
pkg_origin="core"

# TODO (CM): don't love the name of this
composite_pkgs=(core/builder-api
                core/builder-api-proxy)

########################################################################

# Globals / Constants
pkg_prefix=$(pwd) # TODO (CM): just for now
COMPOSITE_PACKAGES_METADATA_FILE="COMPOSITE_PACKAGES"

# If you didn't specify any packages, why are you making a composite?
# If you only specified one, why aren't you just using that directly?
_ensure_more_than_one_package() {
    # TODO (CM): add STDERR warnings?
    pkgs=("$@")
    if [ "${#pkgs[@]}" -ge "2" ]
    then
        return 0
    else
        echo "NOOOOPE"
        return 1
    fi
}

_write_composite_metadata() {
    # TODO (CM): Hrmm... these will need to be resolved into specific
    # releases, not just what the user passed in. We'll need to steal
    # some of that logic from plan.sh.
    #
    # Wonder if there's a good way to split the helper functions out
    # from the overall hab-plan-build.sh script. That could be quite
    # useful; it'd also make it easier to test things!
    #
    # OR THIS
    #if [ "$BASH_SOURCE" == "$0" ]; then
    # code in here only gets executed if
    # script is run directly on the commandline
    #fi

    pkgs=("$@")
    for pkg in "${pkgs[@]}"
    do
        echo "${pkg}" >> "${pkg_prefix}/${COMPOSITE_PACKAGES_METADATA_FILE}"
    done
}

# COPIED FROM hab-plan-build.sh
_install_dependency() {
    local dep="${1}"
    if [[ -z "${NO_INSTALL_DEPS:-}" ]]; then
        $HAB_BIN install -u $HAB_DEPOT_URL --channel $HAB_DEPOT_CHANNEL "$dep" || {
            if [[ "$HAB_DEPOT_CHANNEL" != "$FALLBACK_CHANNEL" ]]; then
                build_line "Trying to install '$dep' from '$FALLBACK_CHANNEL'"
                $HAB_BIN install -u $HAB_DEPOT_URL --channel "$FALLBACK_CHANNEL" "$dep" || true
            fi
        }
    fi
    return 0
}

# COPIED FROM hab-plan-build.sh
_resolve_dependency() {
  local dep="$1"
  local dep_path
  if ! echo "$dep" | grep -q '\/' > /dev/null; then
    warn "Origin required for '$dep' in plan '$pkg_origin/$pkg_name' (example: acme/$dep)"
    return 1
  fi

  if dep_path=$(_latest_installed_package "$dep"); then
    echo "${dep_path}"
    return 0
  else
    return 1
  fi
}

# COPIED FROM hab-plan-build.sh
_latest_installed_package() {
  if [[ ! -d "$HAB_PKG_PATH/$1" ]]; then
    warn "No installed packages of '$1' were found"
    return 1
  fi

  # Count the number of slashes, and use it to make a choice
  # about what to return as the latest package.
  local latest_package_flags=$(echo $1 | grep -o '/' | wc -l)
  local depth
  local result
  case $(trim $latest_package_flags) in
    3) depth=1 ;;
    2) depth=2 ;;
    1) depth=3 ;;
  esac
  result=$(find $HAB_PKG_PATH/${1} -maxdepth $depth -type f -name MANIFEST \
    | $_sort_cmd --version-sort -r | head -n 1)
  if [[ -z "$result" ]]; then
    warn "Could not find a suitable installed package for '$1'"
    return 1
  else
    echo "$(dirname $result)"
    return 0
  fi
}

# COPIED FROM hab-plan-build.sh
warn() {
  if [[ "${HAB_NOCOLORING:-}" == "true" ]]; then
    >&2 echo "   ${pkg_name}: WARN $1"
  else
    case "${TERM:-}" in
      *term | xterm-* | rxvt | screen | screen-*)
        >&2 echo -e "   \033[1;36m${pkg_name}: \033[1;33mWARN \033[1;37m$1\033[0m"
        ;;
      *)
        >&2 echo "   ${pkg_name}: WARN $1"
        ;;
    esac
  fi
  return 0
}

# COPIED FROM hab-plan-build.sh
# Print a line of build output. Takes the rest of the line as its only
# argument.
#
# ```sh
# build_line "Checksum verified - ${pkg_shasum}"
# ```
build_line() {
  if [[ "${HAB_NOCOLORING:-}" == "true" ]]; then
    echo "   ${pkg_name}: $1"
  else
    case "${TERM:-}" in
      *term | xterm-* | rxvt | screen | screen-*)
        echo -e "   \033[1;36m${pkg_name}: \033[1;37m$1\033[0m"
        ;;
      *)
        echo "   ${pkg_name}: $1"
        ;;
    esac
  fi
  return 0
}

# COPIED FROM hab-plan-build.sh
trim() {
  local var="$*"
  var="${var#"${var%%[![:space:]]*}"}"   # remove leading whitespace characters
  var="${var%"${var##*[![:space:]]}"}"   # remove trailing whitespace characters
  echo "$var"
}

# TODO (CM): other things could be rewritten in terms of this (DEPS,
# TDEPS, etc)
_read_metadata_file_for() {
    local pkg_path="${1}"
    local filename="${2}"
    local full_path="${pkg_path}/${filename}"

    if [[ -f "${full_path}" ]]
    then
        cat "${full_path}"
    else
        echo
    fi
}

_sort_array() {
    # Thanks https://stackoverflow.com/a/11789688
    local arr=("$@")
    IFS=$'\n' sorted=($(sort <<<"${arr[*]}"))
    unset IFS
    echo "${sorted[@]}"
}


########################################################################

if [ "${BASH_SOURCE}" == "$0" ]
then
    set -euo pipefail

    HAB_BIN="hab"
    HAB_DEPOT_URL="https://willem.habitat.sh/v1/depot"
    HAB_DEPOT_CHANNEL="stable"
    FALLBACK_CHANNEL="stable"
    HAB_PKG_PATH="/hab/pkgs"
#    _sort_cmd=/usr/bin/sort
    _sort_cmd=/hab/pkgs/core/coreutils/8.25/20170513213226/bin/sort

    _ensure_more_than_one_package "${composite_pkgs[@]}"



    for pkg in "${composite_pkgs[@]}"
    do
        echo "INSTALLING ${pkg}"
        _install_dependency "${pkg}"
    done

    resolved_packages=()
    for pkg in "${composite_pkgs[@]}"
    do
        echo "RESOLVING ${pkg}"
        if resolved=$(_resolve_dependency "${pkg}")
        then
            resolved_packages+=($resolved)
        else
            warn "LOLWUT"
        fi
    done

    echo "${resolved_packages[@]}"

    # Grab the binds and exports
    declare -A binds
    for pkg in "${resolved_packages[@]}"
    do
        echo "BINDS for ${pkg}"
        for line in $(_read_metadata_file_for "${pkg}" BINDS)
        do
            echo "LINE: ${line}"
            IFS== read key val <<< "${line}"
            echo "KEY: ${key}"
            echo "VAL: ${val}"
            binds[${key}]=${val}
        done
    done
#    echo "ALL THE BINDS: ${!binds[@]}"

    # Exports
    for pkg in "${resolved_packages[@]}"
    do
        echo "EXPORTS for ${pkg}"
        for line in $(_read_metadata_file_for "${pkg}" EXPORTS)
        do
            echo "A LINE: ${line}"
        done
    done

    # Optional binds
    for pkg in "${resolved_packages[@]}"
    do
        echo "BINDS_OPTIONAL for ${pkg}"
        _read_metadata_file_for "${pkg}" BINDS_OPTIONAL
    done



fi
