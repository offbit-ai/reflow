#!/usr/bin/env node

/**
 * Simple, reliable build script for reflow_node
 * Replaces cargo-cp-artifact with a more reliable approach
 */

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function log(message) {
    console.log(`🔨 ${message}`);
}

function error(message) {
    console.error(`❌ ${message}`);
    process.exit(1);
}

function success(message) {
    console.log(`✅ ${message}`);
}

function findWorkspaceRoot() {
    let currentDir = process.cwd();
    
    while (currentDir !== path.dirname(currentDir)) {
        const cargoToml = path.join(currentDir, 'Cargo.toml');
        if (fs.existsSync(cargoToml)) {
            const content = fs.readFileSync(cargoToml, 'utf8');
            // Check if this is a workspace root
            if (content.includes('[workspace]')) {
                log(`Found workspace root: ${currentDir}`);
                return currentDir;
            }
        }
        currentDir = path.dirname(currentDir);
    }
    
    // If no workspace found, assume current directory
    return process.cwd();
}

function findTargetDir() {
    // First check if CARGO_TARGET_DIR is set
    if (process.env.CARGO_TARGET_DIR) {
        return process.env.CARGO_TARGET_DIR;
    }
    
    // Try workspace root first
    const workspaceRoot = findWorkspaceRoot();
    const workspaceTarget = path.join(workspaceRoot, 'target');
    
    if (fs.existsSync(workspaceTarget)) {
        log(`Using workspace target directory: ${workspaceTarget}`);
        return workspaceTarget;
    }
    
    // Fall back to local target
    const localTarget = path.join(process.cwd(), 'target');
    log(`Using local target directory: ${localTarget}`);
    return localTarget;
}

function main() {
    log('Starting workspace-aware reflow_node build...');
    
    // Clean previous builds
    log('Cleaning previous builds...');
    try {
        // Clean the specific package in workspace
        execSync('cargo clean -p reflow_node', { stdio: 'inherit' });
        if (fs.existsSync('index.node')) {
            fs.unlinkSync('index.node');
            log('Removed old index.node');
        }
    } catch (e) {
        log('Clean failed (might not matter), continuing...');
    }
    
    // Build the specific package
    const buildMode = process.argv[2] === 'debug' ? '' : '--release';
    const targetSubDir = process.argv[2] === 'debug' ? 'debug' : 'release';
    
    log(`Building reflow_node package in ${targetSubDir} mode...`);
    try {
        // Build the specific package in the workspace
        execSync(`cargo build -p reflow_node ${buildMode}`, { stdio: 'inherit' });
        success('Rust build completed');
    } catch (e) {
        error(`Cargo build failed: ${e.message}`);
    }
    
    // Find the target directory (workspace-aware)
    const targetDir = findTargetDir();
    const buildDir = path.join(targetDir, targetSubDir);
    
    log(`Looking for built library in: ${buildDir}`);
    
    // Determine the library file name based on platform
    const platform = process.platform;
    let libName;
    
    if (platform === 'win32') {
        libName = 'reflow_node.dll';
    } else if (platform === 'darwin') {
        libName = 'libreflow_node.dylib';
    } else {
        libName = 'libreflow_node.so';
    }
    
    const sourcePath = path.join(buildDir, libName);
    const targetPath = path.join(process.cwd(), 'index.node');
    
    // Check if the library was built
    if (!fs.existsSync(sourcePath)) {
        error(`Built library not found: ${sourcePath}`);
        
        // Debug: List what files are actually there
        if (fs.existsSync(buildDir)) {
            log('Files in build directory:');
            fs.readdirSync(buildDir).forEach(file => {
                const filePath = path.join(buildDir, file);
                const stats = fs.statSync(filePath);
                console.log(`  - ${file} (${stats.size} bytes)`);
            });
        } else {
            error(`Build directory doesn't exist: ${buildDir}`);
        }
        
        // Try to find any reflow_node files anywhere in target
        log('Searching for reflow_node files in target directory...');
        try {
            const findCommand = platform === 'win32' 
                ? `dir /s /b "${targetDir}\\*reflow_node*"`
                : `find "${targetDir}" -name "*reflow_node*" -type f`;
            
            const result = execSync(findCommand, { encoding: 'utf8' });
            if (result.trim()) {
                console.log('Found reflow_node files:');
                result.trim().split('\n').forEach(file => console.log(`  - ${file}`));
            } else {
                log('No reflow_node files found anywhere in target directory');
            }
        } catch (e) {
            log('Could not search for files');
        }
        
        return;
    }
    
    // Copy the library
    log(`Copying ${sourcePath} to ${targetPath}...`);
    try {
        fs.copyFileSync(sourcePath, targetPath);
        
        // Verify the copy
        const stats = fs.statSync(targetPath);
        success(`Successfully created index.node (${stats.size} bytes)`);
        
        // Test that the module can be loaded
        log('Testing module load...');
        try {
            const reflow = require(path.resolve(targetPath));
            const exports = Object.keys(reflow);
            success(`Module loads successfully! Exports: ${exports.join(', ')}`);
        } catch (e) {
            error(`Module failed to load: ${e.message}`);
        }
        
    } catch (e) {
        error(`Failed to copy library: ${e.message}`);
    }
}

if (require.main === module) {
    main();
}