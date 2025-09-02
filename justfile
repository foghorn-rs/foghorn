database_url := ("sqlite://" + justfile_directory() + "/foghorn.db")

prepare-sqlx: setup-sqlx-db
    cargo sqlx prepare --workspace --database-url "{{database_url}}"

setup-sqlx-db:
    cargo sqlx database setup --database-url "{{database_url}}"

install-sqlx:
    cargo binstall sqlx-cli@0.8.3
