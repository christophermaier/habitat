pkg_origin="core"
pkg_name="builder"
pkg_type="composite"
pkg_version="1.0.0"

pkg_services=(core/builder-api
              core/builder-api-proxy
              core/builder-jobsrv)


# TODO (CM): So here, we probably want the keys to be
# origin/package-name, right? Would we want them to be the same as
# what's provided in pkg_services?

# TODO (CM): We could also detect cycles here... might need to do that
# in a Rust implementation, though
pkg_bind_map=(
    [core/builder-api-proxy]="http:core/builder-api"
    [core/builder-api]="router:core/builder-api-proxy"
)
