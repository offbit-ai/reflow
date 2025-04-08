use crate::error::ServiceError;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use tracing::{info, error, warn};
use directories::UserDirs;

// Global store for session-specific virtual environments
static SESSION_VENVS: Lazy<DashMap<Uuid, PathBuf>> = Lazy::new(DashMap::new);

// Global store for installed packages
pub static INSTALLED_PACKAGES: Lazy<Arc<Mutex<HashSet<String>>>> = 
    Lazy::new(|| Arc::new(Mutex::new(HashSet::new())));

// Base directory for all virtual environments
static VENV_BASE_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let base = std::env::temp_dir().join("pyexec_venvs");
    std::fs::create_dir_all(&base).expect("Failed to create venv base directory");
    base
});

// Shared venv directory
static SHARED_VENV_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let user_dirs = UserDirs::new().unwrap();

    let mut base = user_dirs.home_dir().to_path_buf(); 
    if !base.exists() {
      base =  std::env::temp_dir().join("pyexec_shared_venv")
    }else {
      base = base.join(".pyexec_shared_venv")
    }
    if !base.exists() {
        std::fs::create_dir_all(&base).expect("Failed to create shared venv directory");
    }
    base
});

// Track packages installed in the shared environment
static SHARED_INSTALLED_PACKAGES: Lazy<Arc<std::sync::Mutex<HashSet<String>>>> = 
    Lazy::new(|| Arc::new(std::sync::Mutex::new(HashSet::new())));

// Flag to determine if we should use the shared environment or session-specific
static USE_SHARED_VENV: Lazy<std::sync::Mutex<bool>> = Lazy::new(|| std::sync::Mutex::new(std::env::var("USE_SHARED_VENV").is_ok()));

/// Set whether to use the shared environment or not
pub fn set_use_shared_environment(use_shared: bool) {
    let mut flag = USE_SHARED_VENV.lock().unwrap();
    *flag = use_shared;
    info!("Shared environment usage set to: {}", use_shared);
}

/// Get current setting for shared environment usage
pub fn is_using_shared_environment() -> bool {
    let flag = USE_SHARED_VENV.lock().unwrap();
    *flag
}

/// Initialize a virtual environment for a session
pub  fn initialize_venv(session_id: &Uuid) -> Result<PathBuf, ServiceError> {
    // Check if we should use the shared environment
    if is_using_shared_environment() {
        return initialize_shared_venv()
    }
    
    // Otherwise use the session-specific environment
    
    // Check if venv already exists for this session
    if let Some(existing) = SESSION_VENVS.get(session_id) {
        return Ok(existing.clone());
    }
    
    // Create a new virtual environment
    let venv_path = VENV_BASE_DIR.join(session_id.to_string());
    
    // Create the venv directory if it doesn't exist
    if !venv_path.join("bin").exists() {
        std::fs::create_dir_all(&venv_path)
            .map_err(|e| ServiceError::Internal(format!("Failed to create venv directory: {}", e)))?;
           // Initialize virtual environment using Python's venv module
            let status = Command::new("python3")
            .args(&["-m", "venv", venv_path.to_str().unwrap()])
            .status()
            .map_err(|e| ServiceError::Internal(format!("Failed to create virtual environment: {}", e)))?;
            
            if !status.success() {
                return Err(ServiceError::Internal(
                    "Failed to create virtual environment".to_string(),
                ));
            }
       
    }

  
    
    // Store the venv path for this session
    SESSION_VENVS.insert(*session_id, venv_path.clone());
    
    Ok(venv_path)
}

// Add this static flag near your other static variables
static PACKAGES_SCANNED: Lazy<std::sync::Mutex<bool>> = Lazy::new(|| std::sync::Mutex::new(false));

/// Initialize the shared virtual environment if it doesn't exist
pub fn initialize_shared_venv() -> Result<PathBuf, ServiceError> {
    let venv_path = SHARED_VENV_DIR.clone();
    info!("Shared venv path: {}", venv_path.display());
    // Create the venv directory if it doesn't exist
    if !venv_path.join("bin").exists() {
        info!("Initializing shared virtual environment at: {}", venv_path.display());
        std::fs::create_dir_all(&venv_path)
            .map_err(|e| ServiceError::Internal(format!("Failed to create shared venv directory: {}", e)))?;

         // Initialize virtual environment using Python's venv module
     let status = Command::new("python3")
     .args(&["-m", "venv", venv_path.to_str().unwrap()])
     .status()
     .map_err(|e| ServiceError::Internal(format!("Failed to create shared virtual environment: {}", e)))?;
     
        if !status.success() {
            return Err(ServiceError::Internal(
                "Failed to create shared virtual environment".to_string(),
            ));
        }
    }

    // Check if we've already scanned packages
    let already_scanned = {
        let scanned = PACKAGES_SCANNED.lock().unwrap();
        *scanned
    };
    
    // Only scan packages if we haven't done so already
    if !already_scanned {
        // Find the Python lib directory and version
        let lib_dir = venv_path.join("lib");
        if !lib_dir.exists() {
            info!("Initialized shared virtual environment at: {}", venv_path.display());
    
           return Ok(venv_path)
        }
        
        // Find the Python version directory (e.g., python3.8, python3.9, etc.)
        let entries = std::fs::read_dir(&lib_dir)
            .map_err(|e| ServiceError::Internal(format!("Failed to read venv lib directory: {}", e)))?;
            
        let mut python_dir = None;
        for entry in entries {
            if let Ok(entry) = entry {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("python") {
                    python_dir = Some(name);
                    break;
                }
            }
        }
        
        let site_packages_dir = match python_dir {
            Some(dir) => lib_dir.join(dir).join("site-packages"),
            None => return Err(ServiceError::Internal("Python directory not found in venv".to_string())),
        };
        
        if !site_packages_dir.exists() {
            return Err(ServiceError::Internal(
                format!("site-packages directory not found: {}", site_packages_dir.display())
            ));
        }
        
        info!("Scanning for installed packages in: {}", site_packages_dir.display());
        
        // Read the directory entries
        let entries = std::fs::read_dir(&site_packages_dir)
            .map_err(|e| ServiceError::Internal(format!("Failed to read site-packages directory: {}", e)))?;
            
        let mut shared_packages = SHARED_INSTALLED_PACKAGES.lock().map_err(|err| ServiceError::Internal(err.to_string()))?;
        let initial_count = shared_packages.len();
        
        // Process each entry in the directory
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                
                // Check if it's a directory and looks like a package
                if path.is_dir() {
                    let file_name = path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("");
                        
                    // Skip if it starts with . or _ (hidden or special directories)
                    if !file_name.starts_with('.') && !file_name.starts_with('_') {
                        // Check if it's a dist-info directory
                        if file_name.ends_with(".dist-info") {
                            if let Some(pkg_name) = file_name.split('-').next() {
                                if !shared_packages.contains(pkg_name) {
                                    shared_packages.insert(pkg_name.to_string());
                                    info!("Found installed package from dist-info: {}", pkg_name);
                                }
                            }
                        } else {
                            // Regular package directory
                            if !shared_packages.contains(file_name) {
                                shared_packages.insert(file_name.to_string());
                                info!("Found installed package: {}", file_name);
                            }
                        }
                    }
                }
            }
        }
        
        let new_packages_count = shared_packages.len() - initial_count;
        info!("Added {} new packages to shared environment registry (total: {})", 
              new_packages_count, shared_packages.len());
              
        // Mark that we've scanned packages
        let mut scanned = PACKAGES_SCANNED.lock().unwrap();
        *scanned = true;
    }
    
    info!("Initialized shared virtual environment at: {}", venv_path.display());
    
    Ok(venv_path)
}

pub fn force_rescan_packages() -> Result<(), ServiceError> {
    let mut scanned = PACKAGES_SCANNED.lock().unwrap();
    *scanned = false;
    Ok(())
}

/// Determine if a package is already installed in the appropriate environment
async fn is_package_installed(package: &str, session_id: &Uuid) -> bool {
    if is_using_shared_environment() {
        let installed = SHARED_INSTALLED_PACKAGES.lock().map_err(|err| ServiceError::Internal(err.to_string())).unwrap();
        installed.contains(package)
    } else {
        let installed = INSTALLED_PACKAGES.lock().await;
        installed.contains(package)
    }
}

/// Install packages for a specific session using pip
pub async fn install_packages(
    session_id: &Uuid, 
    requirements: &[String], 
    progress_sender: Option<Arc<tokio::sync::Mutex<mpsc::UnboundedSender<String>>>>
) -> Result<(), ServiceError> {
    // Get the virtual environment path
    let venv_path = if is_using_shared_environment() {
        initialize_shared_venv()?
    } else {
        initialize_venv(session_id)?
    };
    
    // Path to pip executable in the venv
    let pip_path = if cfg!(windows) {
        venv_path.join("Scripts").join("pip.exe")
    } else {
        venv_path.join("bin").join("pip")
    };
    
    // Make sure pip path exists
    if !pip_path.exists() {
        return Err(ServiceError::Internal(
            format!("pip not found at: {}", pip_path.display())
        ));
    }
    
    // Get currently installed packages to avoid reinstalling
    let packages_to_install: Vec<String>;
    
    if is_using_shared_environment() {
        let installed = SHARED_INSTALLED_PACKAGES.lock().map_err(|err| ServiceError::Internal(err.to_string()))?;
        packages_to_install = requirements
            .iter()
            .filter(|pkg| !installed.contains(*pkg))
            .cloned()
            .collect();
    } else {
        let installed = INSTALLED_PACKAGES.lock().await;
        packages_to_install = requirements
            .iter()
            .filter(|pkg| !installed.contains(*pkg))
            .cloned()
            .collect();
    }
    
    if packages_to_install.is_empty() {
        // All packages already installed
        if let Some(sender) = progress_sender {
            let _ = sender.lock().await.send(serde_json::to_string(&serde_json::json!({
                "type": "package_install",
                "status": "complete",
                "message": "All packages already installed"
            })).unwrap());
        }
        return Ok(());
    }
    
    // Send initial progress update
    if let Some(sender) = &progress_sender {
        let _ = sender.lock().await.send(serde_json::to_string(&serde_json::json!({
            "type": "package_install",
            "status": "start",
            "packages": packages_to_install,
            "message": format!("Installing {} packages", packages_to_install.len())
        })).unwrap());
    }
    
    // Install packages
    let progress_sender_clone = progress_sender.clone();
    let _session_id = session_id.clone();
    // Spawn a blocking task to install packages
    let installation_result = tokio::task::spawn_blocking(move || {
        let packages = packages_to_install.clone();
        let pip = pip_path.clone();
        let progress_sender = progress_sender_clone.clone();
        let using_shared = is_using_shared_environment();
        let session = _session_id;
        
        Box::pin(async move || {
            // Install each package
            for pkg in &packages {
                if using_shared {
                    info!("Installing package in shared environment: {}", pkg);
                } else {
                    info!("Installing package for session {}: {}", session, pkg);
                }
                
                // Send progress update
                if let Some(sender) = &progress_sender {
                    let _ = sender.lock().await.send(serde_json::to_string(&serde_json::json!({
                        "type": "package_install",
                        "status": "progress",
                        "package": pkg,
                        "message": format!("Installing {}", pkg)
                    })).unwrap());
                }
                
                let output = Command::new(&pip)
                    .args(&["install", pkg])
                    .output();
                
                match output {
                    Ok(output) => {
                        if !output.status.success() {
                            let error_msg = String::from_utf8_lossy(&output.stderr).to_string();
                            error!("Failed to install package {}: {}", pkg, error_msg);
                            
                            if let Some(sender) = &progress_sender {
                                let _ = sender.lock().await.send(serde_json::to_string(&serde_json::json!({
                                    "type": "package_install",
                                    "status": "error",
                                    "package": pkg,
                                    "message": format!("Failed to install {}: {}", pkg, error_msg)
                                })).unwrap());
                            }
                            
                            return Err(ServiceError::Internal(
                                format!("Failed to install package {}: {}", pkg, error_msg)
                            ));
                        } else {
                            // Mark package as installed in the appropriate registry
                            if using_shared {
                                let mut installed = SHARED_INSTALLED_PACKAGES.lock().map_err(|err| ServiceError::Internal(err.to_string()))?;
                                installed.insert(pkg.clone());
                                info!("Package {} installed in shared environment", pkg);
                            } else {
                                let mut installed = INSTALLED_PACKAGES.lock().await;
                                installed.insert(pkg.clone());
                                info!("Package {} installed for session {}", pkg, session);
                            }
                            
                            if let Some(sender) = &progress_sender {
                                let _ = sender.lock().await.send(serde_json::to_string(&serde_json::json!({
                                    "type": "package_install",
                                    "status": "success",
                                    "package": pkg,
                                    "message": format!("Successfully installed {}", pkg)
                                })).unwrap());
                            }
                        }
                    },
                    Err(e) => {
                        error!("Error executing pip command: {}", e);
                        
                        if let Some(sender) = &progress_sender {
                            let _ = sender.lock().await.send(serde_json::to_string(&serde_json::json!({
                                "type": "package_install",
                                "status": "error",
                                "package": pkg,
                                "message": format!("Error installing {}: {}", pkg, e)
                            })).unwrap());
                        }
                        
                        return Err(ServiceError::Internal(
                            format!("Error executing pip command: {}", e)
                        ));
                    }
                }
            }
            
            Ok(())
        })
    }).await;
    
    // Handle the result of the installation task
    match installation_result {
        Ok(result) => {
            match result().await {
                Ok(_) => {
                    // All packages installed successfully
                    if let Some(sender) = progress_sender.clone() {
                        let _ = sender.lock().await.send(serde_json::to_string(&serde_json::json!({
                            "type": "package_install",
                            "status": "complete",
                            "message": "All packages installed successfully"
                        })).unwrap());
                    }
                    
                    Ok(())
                },
                Err(e) => Err(e),
            }
        },
        Err(e) => {
            error!("Package installation task failed: {}", e);
            
            Err(ServiceError::Internal(
                format!("Package installation task failed: {}", e)
            ))
        },
    }
}

/// Get the Python executable path for a session's virtual environment
pub async  fn get_python_path(session_id: &Uuid) -> Result<PathBuf, ServiceError> {
    let venv_path = if is_using_shared_environment() {
        initialize_shared_venv()?
    } else {
        match SESSION_VENVS.get(session_id) {
            Some(path) => path.clone(),
            None => initialize_venv(session_id)?,
        }
    };
    
    // Path to Python executable in the venv
    let python_path = if cfg!(windows) {
        venv_path.join("Scripts").join("python.exe")
    } else {
        venv_path.join("bin").join("python")
    };
    
    if !python_path.exists() {
        return Err(ServiceError::Internal(
            format!("Python not found at: {}", python_path.display())
        ));
    }
    
    Ok(python_path)
}

/// Clean up a session's virtual environment
pub fn cleanup_venv(session_id: &Uuid) -> Result<(), ServiceError> {
    // Don't clean up the shared environment
    if is_using_shared_environment() {
        return Ok(());
    }
    
    if let Some((_, venv_path)) = SESSION_VENVS.remove(session_id) {
        std::fs::remove_dir_all(&venv_path)
            .map_err(|e| ServiceError::Internal(format!("Failed to remove venv directory: {}", e)))?;
    }
    
    Ok(())
}

/// Setup the Python interpreter to use the appropriate virtual environment
pub fn setup_venv_for_interpreter<'a>(py: Python<'a>, globals: &PyDict, session_id: &Uuid) -> Result<(), ServiceError> {
    // Get the virtual environment path
    let venv_path = if is_using_shared_environment() {
        initialize_shared_venv()?
    } else {
        match SESSION_VENVS.get(session_id) {
            Some(path) => path.clone(),
            None => initialize_venv(session_id)?,
        }
    };
    
    // Add the venv's site-packages to sys.path
    let site_packages = if cfg!(windows) {
        venv_path.join("Lib").join("site-packages")
    } else {
        let mut path = venv_path.join("lib");
        
        // Find the Python version directory (e.g., python3.8)
        let entries = std::fs::read_dir(&path)
            .map_err(|e| ServiceError::Internal(format!("Failed to read venv lib directory: {}", e)))?;
            
        let mut python_dir = None;
        for entry in entries {
            if let Ok(entry) = entry {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("python") {
                    python_dir = Some(name);
                    break;
                }
            }
        }
        
        match python_dir {
            Some(dir) => path.join(dir).join("site-packages"),
            None => return Err(ServiceError::Internal("Python directory not found in venv".to_string())),
        }
    };
    
    // Make sure the site-packages directory exists
    if !site_packages.exists() {
        return Err(ServiceError::Internal(
            format!("site-packages not found at: {}", site_packages.display())
        ));
    }
    
    // Add the site-packages directory to sys.path
    py.run(
        &format!(
            r#"
import sys
site_packages = r"{}"
if site_packages not in sys.path:
    sys.path.insert(0, site_packages)
"#,
            site_packages.to_string_lossy().replace('\\', "\\\\")
        ),
        None,
        Some(globals),
    )?;
    
    Ok(())
}

/// Get a list of all packages installed in the shared environment
pub async fn list_shared_packages() -> Vec<String> {
    let installed = SHARED_INSTALLED_PACKAGES.lock().map_err(|err| ServiceError::Internal(err.to_string())).expect("Failed to lock shared installed packages");
    installed.iter().cloned().collect()
}

/// Check if a package is installed in the shared environment
pub async fn is_shared_package_installed(package: &str) -> bool {
    let installed = SHARED_INSTALLED_PACKAGES.lock().map_err(|err| ServiceError::Internal(err.to_string())).expect("Failed to lock shared installed packages");
    installed.contains(package)
}