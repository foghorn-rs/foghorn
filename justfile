default: build-release

font:
  curl -fsSLO https://unpkg.com/lucide-static@latest/font/Lucide.ttf

clean:
  rm Lucide.ttf debug_log.json

clean-all: clean
  rm -r blobs conf db snap.*

build-debug *args: font
  cargo build {{args}}

build-release *args: (build-debug '--release' args)

run-debug *args: font
  env RUST_BACKTRACE=full cargo run {{args}}

run-release *args: (run-debug '--release' args)
