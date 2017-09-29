pkg_origin="core"
pkg_name="builder-tiny"
pkg_type="composite"
pkg_version="1.0.0"

pkg_services=(
    # core/builder-admin
    # core/builder-admin-proxy
    core/builder-api
    core/builder-api-proxy
#    core/builder-datastore
#    core/builder-jobsrv
#    core/builder-originsrv
    core/builder-router
 #   core/builder-scheduler
#    core/builder-sessionsrv
#    core/builder-worker
)

# TODO (CM): So here, we probably want the keys to be
# origin/package-name, right? Would we want them to be the same as
# what's provided in pkg_services?

# TODO (CM): We could also detect cycles here... might need to do that
# in a Rust implementation, though
pkg_bind_map=(
    [core/builder-api-proxy]="http:core/builder-api"
    [core/builder-api]="router:core/builder-router"
 #    [core/builder-admin]="router:core/builder-router"
#     [core/builder-admin-proxy]="http:core/builder-admin"
#     [core/builder-jobsrv]="router:core/builder-router datastore:core/builder-datastore"
#  #   [core/builder-originsrv]="router:core/builder-router datastore:core/builder-datastore"
#     [core/builder-scheduler]="router:core/builder-router datastore:core/builder-datastore"
#     [core/builder-sessionsrv]="router:core/builder-router datastore:core/builder-datastore"
#     [core/builder-worker]="jobsrv:core/builder-jobsrv depot:core/builder-api"
 )

# pkg_set_default=all
# pkg_sets=(
#   [all]="core/builder-admin core/builder-admin-proxy core/builder-api core/builder-api-proxy core/builder-datastore core/builder-jobsrv core/builder-originsrv core/builder-router core/builder-scheduler core/builder-sessionsrv core/builder-worker"
#   [frontend]="core/builder-api core/builder-api-proxy"
#   [admin]="core/builder-admin core/builder-admin-proxy"
#   [backend-services]="core/builder-jobsrv core/builder-originsrv core/builder-sessionsrv core/builder-sessionsrv"
#   [data]="core/builder-datastore"
#   [worker]="core/builder-worker"
# )
