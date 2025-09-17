#!/bin/bash

# Reflow Package Deployment Script
# Deploys npm packages, Rust crates, and Go modules to their respective registries

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
ENV_FILE=".env.package"
DRY_RUN=false
VERBOSE=false
DEPLOY_NPM=true
DEPLOY_RUST=true
DEPLOY_GO=true

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --dry-run)
      DRY_RUN=true
      shift
      ;;
    --verbose|-v)
      VERBOSE=true
      shift
      ;;
    --npm-only)
      DEPLOY_RUST=false
      DEPLOY_GO=false
      shift
      ;;
    --rust-only)
      DEPLOY_NPM=false
      DEPLOY_GO=false
      shift
      ;;
    --go-only)
      DEPLOY_NPM=false
      DEPLOY_RUST=false
      shift
      ;;
    --help|-h)
      echo "Usage: $0 [options]"
      echo "Options:"
      echo "  --dry-run       Run without actually publishing"
      echo "  --verbose, -v   Show detailed output"
      echo "  --npm-only      Only deploy npm packages"
      echo "  --rust-only     Only deploy Rust crates"
      echo "  --go-only       Only deploy Go modules"
      echo "  --help, -h      Show this help message"
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Load environment variables
if [ -f "$ENV_FILE" ]; then
  echo -e "${BLUE}Loading credentials from $ENV_FILE${NC}"
  export $(cat $ENV_FILE | grep -v '^#' | xargs)
else
  echo -e "${RED}Error: $ENV_FILE not found${NC}"
  echo "Please create $ENV_FILE with the following variables:"
  echo "  NPM_TOKEN=your_npm_token"
  echo "  CARGO_REGISTRY_TOKEN=your_cargo_token"
  exit 1
fi

# Validate credentials
validate_credentials() {
  local errors=0
  
  if [ "$DEPLOY_NPM" = true ] && [ -z "$NPM_TOKEN" ]; then
    echo -e "${RED}Error: NPM_TOKEN is not set${NC}"
    errors=$((errors + 1))
  fi
  
  if [ "$DEPLOY_RUST" = true ] && [ -z "$CARGO_REGISTRY_TOKEN" ]; then
    echo -e "${RED}Error: CARGO_REGISTRY_TOKEN is not set${NC}"
    errors=$((errors + 1))
  fi
  
  if [ $errors -gt 0 ]; then
    exit 1
  fi
}

# Function to run command (respects dry-run mode)
run_cmd() {
  if [ "$DRY_RUN" = true ]; then
    echo -e "${YELLOW}[DRY RUN] Would execute: $@${NC}"
  else
    if [ "$VERBOSE" = true ]; then
      echo -e "${BLUE}Executing: $@${NC}"
    fi
    "$@"
  fi
}

# Deploy npm packages
deploy_npm() {
  echo -e "${GREEN}=== Deploying NPM Packages ===${NC}"
  
  # Configure npm authentication
  if [ "$DRY_RUN" = false ]; then
    echo "//registry.npmjs.org/:_authToken=${NPM_TOKEN}" > ~/.npmrc
    echo -e "${GREEN}✓ NPM authentication configured${NC}"
  else
    echo -e "${YELLOW}[DRY RUN] Would configure NPM authentication${NC}"
  fi
  
  # Deploy main unified package
  if [ -d "npm-package" ]; then
    echo -e "${BLUE}Publishing @offbit-ai/reflow...${NC}"
    cd npm-package
    
    # Check current version
    CURRENT_VERSION=$(node -p "require('./package.json').version")
    echo "Current version: $CURRENT_VERSION"
    
    # Publish
    if [ "$DRY_RUN" = true ]; then
      run_cmd npm publish --dry-run --access public
    else
      run_cmd npm publish --access public || {
        echo -e "${YELLOW}Warning: npm publish failed. Package might already be published.${NC}"
      }
    fi
    
    cd ..
    echo -e "${GREEN}✓ @offbit-ai/reflow published${NC}"
  else
    echo -e "${YELLOW}Warning: npm-package directory not found${NC}"
  fi
  
  # Deploy Node.js native bindings if separate
  if [ -d "crates/reflow_node" ] && [ -f "crates/reflow_node/package.json" ]; then
    echo -e "${BLUE}Publishing @offbit-ai/reflow-nodejs...${NC}"
    cd crates/reflow_node
    
    # Check if it has its own package.json
    if [ -f "package.json" ]; then
      CURRENT_VERSION=$(node -p "require('./package.json').version")
      echo "Current version: $CURRENT_VERSION"
      
      if [ "$DRY_RUN" = true ]; then
        run_cmd npm publish --dry-run --access public
      else
        run_cmd npm publish --access public || {
          echo -e "${YELLOW}Warning: npm publish failed. Package might already be published.${NC}"
        }
      fi
    fi
    
    cd ../..
    echo -e "${GREEN}✓ @offbit-ai/reflow-nodejs published${NC}"
  fi
}

# Deploy Rust crates
deploy_rust() {
  echo -e "${GREEN}=== Deploying Rust Crates ===${NC}"
  
  # Configure cargo authentication
  if [ "$DRY_RUN" = false ]; then
    cargo login "$CARGO_REGISTRY_TOKEN"
    echo -e "${GREEN}✓ Cargo authentication configured${NC}"
  else
    echo -e "${YELLOW}[DRY RUN] Would configure Cargo authentication${NC}"
  fi
  
  # List of crates to publish in dependency order
  CRATES=(
    "crates/reflow_graph"
    "crates/reflow_tracing_protocol"
    "crates/reflow_tracing"
    "crates/actor_macro"
    "crates/reflow_actor"
    "crates/reflow_network"
    "crates/reflow_script"
    "crates/pyexec"
    "crates/reflow_wasm"
    "crates/reflow_wasm_go"
    "crates/reflow_node"
    "crates/reflow_server"
  )
  
  for crate_path in "${CRATES[@]}"; do
    if [ -d "$crate_path" ]; then
      crate_name=$(basename "$crate_path")
      echo -e "${BLUE}Publishing $crate_name...${NC}"
      
      cd "$crate_path"
      
      # Get current version
      CURRENT_VERSION=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')
      echo "Current version: $CURRENT_VERSION"
      
      # Build first to ensure everything compiles
      run_cmd cargo build --release
      
      # Publish
      if [ "$DRY_RUN" = true ]; then
        run_cmd cargo publish --dry-run --allow-dirty
      else
        run_cmd cargo publish --allow-dirty || {
          echo -e "${YELLOW}Warning: cargo publish failed for $crate_name. It might already be published.${NC}"
        }
        # Wait a bit between publishes to allow crates.io to process
        sleep 5
      fi
      
      cd - > /dev/null
      echo -e "${GREEN}✓ $crate_name published${NC}"
    else
      echo -e "${YELLOW}Warning: $crate_path not found, skipping${NC}"
    fi
  done
}

# Deploy Go modules
deploy_go() {
  echo -e "${GREEN}=== Deploying Go Modules ===${NC}"
  
  # Check for Go SDK in reflow_wasm_go/sdk
  GO_MODULE_PATH="crates/reflow_wasm_go/sdk"
  
  if [ -d "$GO_MODULE_PATH" ] && [ -f "$GO_MODULE_PATH/go.mod" ]; then
    echo -e "${BLUE}Publishing Go SDK module...${NC}"
    cd "$GO_MODULE_PATH"
    
    # Get module name
    MODULE_NAME=$(go mod edit -json | jq -r '.Module.Path')
    echo "Module: $MODULE_NAME"
    
    # For Go modules, we typically just tag and push to git
    # The Go proxy will automatically pick it up
    if command -v git &> /dev/null; then
      # Get current version from git tags or use default
      LATEST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0")
      echo "Latest tag: $LATEST_TAG"
      
      # Parse version and increment patch
      VERSION_PARTS=(${LATEST_TAG//v/})
      VERSION_PARTS=(${VERSION_PARTS//./ })
      MAJOR=${VERSION_PARTS[0]:-0}
      MINOR=${VERSION_PARTS[1]:-0}
      PATCH=${VERSION_PARTS[2]:-0}
      NEW_PATCH=$((PATCH + 1))
      NEW_VERSION="v${MAJOR}.${MINOR}.${NEW_PATCH}"
      
      echo "New version: $NEW_VERSION"
      
      if [ "$DRY_RUN" = true ]; then
        echo -e "${YELLOW}[DRY RUN] Would create git tag: $NEW_VERSION${NC}"
        echo -e "${YELLOW}[DRY RUN] Would push tag to origin${NC}"
      else
        # Create and push tag
        git tag "$NEW_VERSION" -m "Release $NEW_VERSION"
        git push origin "$NEW_VERSION" || {
          echo -e "${YELLOW}Warning: Failed to push tag. You may need to manually push.${NC}"
        }
      fi
      
      # Run go mod tidy to ensure dependencies are correct
      run_cmd go mod tidy
      
      # Verify the module builds
      run_cmd go build ./...
      
      echo -e "${GREEN}✓ Go module tagged as $NEW_VERSION${NC}"
      echo "The Go proxy will automatically index this version"
    else
      echo -e "${YELLOW}Warning: Git not available, cannot tag Go module${NC}"
    fi
    
    cd - > /dev/null
  else
    echo -e "${YELLOW}Warning: Go SDK module not found in $GO_MODULE_PATH${NC}"
  fi
}

# Main execution
echo -e "${GREEN}🚀 Reflow Package Deployment Script${NC}"
echo "======================================"

if [ "$DRY_RUN" = true ]; then
  echo -e "${YELLOW}Running in DRY RUN mode - no packages will be published${NC}"
fi

# Validate credentials
validate_credentials

# Deploy packages based on flags
if [ "$DEPLOY_NPM" = true ]; then
  deploy_npm
fi

if [ "$DEPLOY_RUST" = true ]; then
  deploy_rust
fi

if [ "$DEPLOY_GO" = true ]; then
  deploy_go
fi

echo ""
echo -e "${GREEN}✅ Deployment complete!${NC}"

if [ "$DRY_RUN" = true ]; then
  echo -e "${YELLOW}This was a dry run. To actually publish, run without --dry-run${NC}"
fi

# Clean up npm auth if we set it
if [ "$DRY_RUN" = false ] && [ "$DEPLOY_NPM" = true ]; then
  rm -f ~/.npmrc
fi