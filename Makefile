.PHONY: all clean native wasm wasm-debug wasm-release test test-wasm install-tools npm-package npm-publish npm-test watch watch-wasm build-nodejs create-unified-wrapper

NETWORK_CRATE = crates/reflow_network
NPM_PACKAGE_NAME = @offbit-ai/reflow
NPM_OUTPUT_DIR = npm-package

# Detect OS
ifeq ($(OS),Windows_NT)
    DETECTED_OS := Windows
else
    DETECTED_OS := $(shell uname -s)
endif

all: install-tools native wasm npm-package

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
	wasm-pack build --target bundler --dev --out-dir pkg/debug

wasm-release:
	cd $(NETWORK_CRATE) && \
	mkdir -p pkg/release && \
	wasm-pack build --target bundler --release --out-dir pkg/release --out-name reflow

# Build npm package with @offbit-ai/reflow name
npm-package: wasm-release build-nodejs create-unified-wrapper
	@echo "Building npm package $(NPM_PACKAGE_NAME)..."
	@mkdir -p $(NPM_OUTPUT_DIR)
	@# Extract version from git tag (if exists) or Cargo.toml
	@if git describe --tags --abbrev=0 2>/dev/null; then \
		VERSION=$$(git describe --tags --abbrev=0 | sed 's/^v//'); \
		echo "Using version from git tag: $$VERSION"; \
	else \
		VERSION=$$(grep '^version = ' $(NETWORK_CRATE)/Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'); \
		echo "Using version from Cargo.toml: $$VERSION"; \
	fi && \
	echo "Setting npm package version to $$VERSION..." && \
	cd npm-unified-wrapper && \
	npm version $$VERSION --allow-same-version --no-git-tag-version && \
	cd ..
	@# Create subfolders for each implementation
	@mkdir -p $(NPM_OUTPUT_DIR)/wasm
	@mkdir -p $(NPM_OUTPUT_DIR)/native
	@# Copy WASM build to wasm subfolder
	@echo "Copying WASM build files to wasm/..."
	@cp -r $(NETWORK_CRATE)/pkg/release/* $(NPM_OUTPUT_DIR)/wasm/
	@# Copy native build to native subfolder
	@echo "Copying native build files to native/..."
	@cp crates/reflow_node/index.node $(NPM_OUTPUT_DIR)/native/ 2>/dev/null || true
	@cp -r crates/reflow_node/lib/* $(NPM_OUTPUT_DIR)/native/ 2>/dev/null || true
	@cp crates/reflow_node/package.json $(NPM_OUTPUT_DIR)/native/package.json 2>/dev/null || true
	@# Fix the paths in the copied files to reference index.node in the same directory
	@echo "Fixing native module paths..."
	@if [ -f $(NPM_OUTPUT_DIR)/native/network.js ]; then \
		sed -i.bak "s|require('../index.node')|require('./index.node')|g" $(NPM_OUTPUT_DIR)/native/network.js && \
		rm $(NPM_OUTPUT_DIR)/native/network.js.bak; \
	fi
	@if [ -f $(NPM_OUTPUT_DIR)/native/graph.js ]; then \
		sed -i.bak "s|require('../index.node')|require('./index.node')|g" $(NPM_OUTPUT_DIR)/native/graph.js && \
		rm $(NPM_OUTPUT_DIR)/native/graph.js.bak; \
	fi
	@# Copy the unified wrapper files to root
	@echo "Adding unified wrapper..."
	@cp -r npm-unified-wrapper/* $(NPM_OUTPUT_DIR)/ 2>/dev/null || true
	@# Use the unified wrapper README
	@if [ -f npm-unified-wrapper/README.md ]; then \
		cp npm-unified-wrapper/README.md $(NPM_OUTPUT_DIR)/README.md; \
	elif [ -f npm-package-readme.md ]; then \
		cp npm-package-readme.md $(NPM_OUTPUT_DIR)/README.md; \
	fi
	@echo "NPM package created in $(NPM_OUTPUT_DIR)/"
	@echo "To publish, run: make npm-publish"

# Build Node.js native bindings
build-nodejs:
	@echo "Building Node.js native bindings..."
	@cd crates/reflow_node && npm run build-release

# Create unified wrapper that combines native and WASM implementations
create-unified-wrapper:
	@mkdir -p npm-unified-wrapper
	@echo "Creating package.json for unified package..."
	@node -e 'console.log(JSON.stringify({ \
		name: "@offbit-ai/reflow", \
		version: "0.1.0", \
		description: "Reflow - Flow-Based Programming Framework", \
		main: "index.js", \
		module: "index.mjs", \
		types: "index.d.ts", \
		exports: { \
			".": { \
				"import": "./index.mjs", \
				"require": "./index.js" \
			}, \
			"./native": { \
				"require": "./native/index.js", \
				"types": "./native/index.d.ts" \
			}, \
			"./wasm": { \
				"import": "./wasm/reflow.js", \
				"require": "./wasm/reflow.js", \
				"types": "./wasm/reflow.d.ts" \
			}, \
			"./auto": { \
				"require": "./auto.js", \
				"import": "./auto.mjs" \
			} \
		}, \
		files: [ \
			"index.js", \
			"index.mjs", \
			"auto.js", \
			"auto.mjs", \
			"wasm/", \
			"native/", \
			"package.json", \
			"README.md" \
		], \
		keywords: [ \
			"flow-based-programming", \
			"fbp", \
			"actor-model", \
			"wasm", \
			"webassembly", \
			"native-bindings" \
		], \
		author: "Offbit AI", \
		license: "MIT", \
		repository: { \
			type: "git", \
			url: "https://github.com/offbit-ai/reflow.git" \
		}, \
		bugs: { \
			url: "https://github.com/offbit-ai/reflow/issues" \
		}, \
		homepage: "https://github.com/offbit-ai/reflow#readme", \
		engines: { \
			node: ">=12.20.0" \
		} \
	}, null, 2))' > npm-unified-wrapper/package.json

# Publish npm package (requires npm login)
npm-publish: npm-package
	@echo "Publishing $(NPM_PACKAGE_NAME) to npm registry..."
	cd $(NPM_OUTPUT_DIR) && npm publish --access public

# Test npm package locally
npm-test: npm-package
	@echo "Testing npm package locally..."
	cd $(NPM_OUTPUT_DIR) && npm link
	@echo "Package linked. You can now use 'npm link $(NPM_PACKAGE_NAME)' in your test projects."

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
	rm -rf $(NPM_OUTPUT_DIR)

watch:
	cd $(NETWORK_CRATE) && cargo watch -x build

watch-wasm:
	cd $(NETWORK_CRATE) && \
	cargo watch -s "wasm-pack build --target web --dev --out-dir pkg/debug"

# Deployment targets
deploy: deploy-npm deploy-rust deploy-go
	@echo "All packages deployed successfully!"

deploy-npm:
	@echo "Deploying npm packages..."
	./scripts/deploy.sh --npm-only

deploy-rust:
	@echo "Deploying Rust crates..."
	./scripts/deploy.sh --rust-only

deploy-go:
	@echo "Deploying Go modules..."
	./scripts/deploy.sh --go-only

deploy-dry-run:
	@echo "Running deployment in dry-run mode..."
	./scripts/deploy.sh --dry-run

deploy-verbose:
	@echo "Running deployment with verbose output..."
	./scripts/deploy.sh --verbose