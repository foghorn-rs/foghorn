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

run-hot *args: font
  @command -v cargo-hot >/dev/null 2>&1 || just install-cargo-hot
  env RUST_BACKTRACE=full cargo hot -F iced/hot {{args}}

run-debug *args: font
  env RUST_BACKTRACE=full cargo run -F iced/debug {{args}}

run-release *args: font
  env RUST_BACKTRACE=full cargo run --release {{args}}

install-cargo-hot:
  cargo install cargo-hot --git https://github.com/hecrj/cargo-hot
