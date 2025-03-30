.PHONY: all clean native wasm wasm-debug wasm-release test test-wasm install-tools

NETWORK_CRATE = crates/reflow_network

# Detect OS
ifeq ($(OS),Windows_NT)
    DETECTED_OS := Windows
else
    DETECTED_OS := $(shell uname -s)
endif

all: install-tools native wasm

install-tools: install-chromedriver
	cargo install wasm-pack
	cargo install cargo-watch

install-chromedriver:
ifeq ($(DETECTED_OS),Windows)
	powershell -Command "if (!(Get-Command chromedriver -ErrorAction SilentlyContinue)) { \
		winget install Google.ChromeDriver; \
	}"
else ifeq ($(DETECTED_OS),Darwin)
	if ! command -v chromedriver > /dev/null; then \
		brew install --cask chromedriver; \
	fi
else
	if ! command -v chromedriver > /dev/null; then \
		sudo apt-get update && sudo apt-get install -y chromium-chromedriver; \
	fi
endif

native:
	cd $(NETWORK_CRATE) && cargo build
	cd $(NETWORK_CRATE) && cargo build --release

wasm: wasm-debug wasm-release

wasm-debug:
	cd $(NETWORK_CRATE) && \
	mkdir -p pkg/debug && \
	cargo add wasm-bindgen-test --dev && \
	wasm-pack build --target web --dev --out-dir pkg/debug

wasm-release:
	cd $(NETWORK_CRATE) && \
	mkdir -p pkg/release && \
	wasm-pack build --target web --release --out-dir pkg/release && \
	cp pkg/debug/reflow_network.d.ts pkg/release/

test:
	cd $(NETWORK_CRATE) && cargo test

test-wasm: install-chromedriver
ifeq ($(DETECTED_OS),Windows)
	powershell -Command "Start-Process chromedriver -WindowStyle Hidden"
	cd $(NETWORK_CRATE) && wasm-pack test --headless --chrome
	powershell -Command "Stop-Process -Name chromedriver -Force -ErrorAction SilentlyContinue"
else ifeq ($(DETECTED_OS),Darwin)
	chromedriver --port=4444 & echo $$! > chromedriver.pid
	cd $(NETWORK_CRATE) && wasm-pack test --headless --chrome
	kill `cat chromedriver.pid` && rm chromedriver.pid
else
	chromedriver --port=4444 & echo $$! > chromedriver.pid
	cd $(NETWORK_CRATE) && wasm-pack test --headless --chrome
	kill `cat chromedriver.pid` && rm chromedriver.pid
endif

clean:
	cd $(NETWORK_CRATE) && cargo clean
	rm -rf $(NETWORK_CRATE)/pkg/

watch:
	cd $(NETWORK_CRATE) && cargo watch -x build

watch-wasm:
	cd $(NETWORK_CRATE) && \
	cargo watch -s "wasm-pack build --target web --dev --out-dir pkg/debug"