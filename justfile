database_url := ("sqlite://" + justfile_directory() + "/foghorn.db")

default: build-release

font:
  @[ -f Lucide.ttf ] || curl -fsSLO https://unpkg.com/lucide-static@latest/font/Lucide.ttf

clean:
  rm -f Lucide.ttf debug_log.json

clean-all: clean
  rm -f foghorn.db foghorn.db-shm foghorn.db-wal

build-debug *args: font
  cargo build -F iced/debug {{args}}

build-release *args: font
  cargo build --release {{args}}

run-debug *args: font
  env RUST_BACKTRACE=full cargo run -F iced/debug {{args}}

run-release *args: font
  env RUST_BACKTRACE=full cargo run --release {{args}}

prepare-sqlx: setup-sqlx-db font
    cargo sqlx prepare --workspace --database-url "{{database_url}}"

setup-sqlx-db:
    cargo sqlx database setup --database-url "{{database_url}}"

install-sqlx:
    cargo install sqlx-cli@0.8.6
